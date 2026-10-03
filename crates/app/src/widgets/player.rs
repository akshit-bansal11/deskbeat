//! Now playing: art, title, artist, transport buttons and progress. Each is
//! placed on its own, wherever the config puts it.

use sonic_veil_core::color::{Rgba, with_alpha};
use sonic_veil_core::config::{Align, PlayerBackground, PlayerPart};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::core::Result;

use super::{Action, Ctx, Part, Tick, Wake, Widget, clock_text, contains, draw_card, place_text};
use crate::gfx::{Gfx, ICON_FONT, TextStyle, rect};
use crate::media::BLUR_SIDE;

const BUTTON: f32 = 30.0;
const BAR_HEIGHT: f32 = 4.0;
/// Extra distance above and below the bar that still counts as on it, for
/// clicking to seek and for grabbing it in edit mode.
const BAR_SLOP: f32 = 9.0;
const TIME_SIZE: f32 = 12.0;
const IDLE_MESSAGE: &str = "Play something on Spotify";

// Segoe Fluent Icons code points.
const ICON_PREVIOUS: &str = "\u{E892}";
const ICON_PLAY: &str = "\u{E768}";
const ICON_PAUSE: &str = "\u{E769}";
const ICON_NEXT: &str = "\u{E893}";
const ICON_NOTE: &str = "\u{E8D6}";

fn icon(g: &mut Gfx, glyph: &str, size: f32, at: D2D_RECT_F, color: Rgba) -> Result<()> {
    let style = TextStyle {
        font: ICON_FONT,
        size,
        weight: 400,
        align: Align::Center,
        wrap: false,
    };
    let (bw, bh) = (at.right - at.left, at.bottom - at.top);
    let layout = g.layout(glyph, &style, bw, bh)?;
    let height = Gfx::measure(&layout).1;
    g.draw_text(&layout, at.left, at.top + (bh - height) / 2.0, color, false);
    Ok(())
}

#[derive(Default)]
pub struct Player {
    art: Option<ID2D1Bitmap>,
    blur: Option<ID2D1Bitmap>,
    /// The art generation the bitmaps were made from.
    art_gen: Option<u64>,
    seen_update: Option<u64>,
    shown_second: Option<i64>,
    hover: Option<usize>,
    /// Hit boxes from the last draw: previous, play/pause, next.
    buttons: [D2D_RECT_F; 3],
    bar: D2D_RECT_F,
    /// Every element drawn last time, for dragging in edit mode.
    parts: Vec<Part>,
}

impl Widget for Player {
    fn tick(&mut self, ctx: &Ctx) -> Tick {
        let media = ctx.media;
        let position = media.position_now(ctx.now_ms);
        let second = (position / 1000.0) as i64;
        let dirty = self.seen_update != Some(media.update_gen) || self.shown_second != Some(second);
        self.seen_update = Some(media.update_gen);
        self.shown_second = Some(second);

        // The only thing that moves on its own is the progress, once a second.
        let wake = if media.playing && ctx.cfg.player.show_progress {
            Wake::after_ms(1000.0 - position % 1000.0 + 5.0)
        } else {
            Wake::Idle
        };
        Tick { dirty, wake }
    }

    fn reset(&mut self) {
        *self = Self::default();
    }

    fn parts(&self) -> &[Part] {
        &self.parts
    }

    fn hover(&mut self, at: Option<(f32, f32)>) -> bool {
        let over = at.and_then(|(x, y)| self.buttons.iter().position(|b| contains(b, x, y)));
        let changed = over != self.hover;
        self.hover = over;
        changed
    }

    fn click(&mut self, x: f32, y: f32) -> Option<Action> {
        match self.buttons.iter().position(|b| contains(b, x, y)) {
            Some(0) => Some(Action::Previous),
            Some(1) => Some(Action::PlayPause),
            Some(_) => Some(Action::Next),
            None => {
                let bar = &self.bar;
                let on_bar = bar.right > bar.left
                    && x >= bar.left
                    && x <= bar.right
                    && y >= bar.top - BAR_SLOP
                    && y <= bar.bottom + BAR_SLOP;
                on_bar.then(|| Action::Seek(f64::from((x - bar.left) / (bar.right - bar.left))))
            }
        }
    }

    fn draw(&mut self, g: &mut Gfx, w: f32, h: f32, ctx: &Ctx) -> Result<()> {
        let p = &ctx.cfg.player;
        let theme = &ctx.cfg.theme;
        let media = ctx.media;
        let o = p.opacity;
        let text = with_alpha(ctx.text, o);
        let dim = with_alpha(ctx.text, 0.66 * o);
        let shadow = theme.text_shadow && p.background == PlayerBackground::None;

        if self.art_gen != Some(media.art_gen) {
            self.art_gen = Some(media.art_gen);
            let art = media.art.as_ref();
            self.art = art.and_then(|a| g.bitmap(a.w, a.h, &a.bgra).ok());
            self.blur = art.and_then(|a| g.bitmap(BLUR_SIDE, BLUR_SIDE, &a.blur).ok());
        }
        self.parts.clear();
        self.buttons = [D2D_RECT_F::default(); 3];
        self.bar = D2D_RECT_F::default();

        // Background.
        let card = rect(0.0, 0.0, w, h);
        let radius = theme.card_radius.min(w.min(h) / 2.0);
        match (p.background, &self.blur) {
            (PlayerBackground::None, _) => {}
            (PlayerBackground::ArtBlur, Some(blur)) => {
                g.fill_round_bitmap(blur, card, radius, o)?;
                // Darkened so white text reads over any album.
                g.fill_round(card, radius, [0.0, 0.0, 0.0, 0.46 * o]);
                if theme.card_border > 0.0 {
                    g.stroke_round(
                        rect(0.5, 0.5, w - 1.0, h - 1.0),
                        radius,
                        [1.0, 1.0, 1.0, theme.card_border * o],
                        1.0,
                    );
                }
            }
            _ => draw_card(g, w, h, ctx, o),
        }

        if p.show_art && p.art_size > 0.0 {
            let frame = rect(p.art.x, p.art.y, p.art_size, p.art_size);
            let art_radius = (radius - 7.0).clamp(4.0, p.art_size / 2.0);
            match &self.art {
                Some(art) => g.fill_round_bitmap(art, frame, art_radius, o)?,
                None => {
                    g.fill_round(frame, art_radius, [1.0, 1.0, 1.0, 0.09 * o]);
                    icon(g, ICON_NOTE, p.art_size * 0.34, frame, dim)?;
                }
            }
            self.parts.push((PlayerPart::Art as u8, frame));
        }

        // Title and artist, or one line saying nothing is playing.
        let (title, title_size, title_weight, title_color) = if media.present {
            (media.title.as_str(), p.title_size, 600, text)
        } else {
            (IDLE_MESSAGE, p.artist_size, 400, dim)
        };
        let font = theme.font.as_str();
        let placed = place_text(g, title, (font, title_size, title_weight), &p.title, w, 0.0)?;
        g.draw_text(&placed.layout, placed.x, p.title.y, title_color, shadow);
        self.parts.push((PlayerPart::Title as u8, placed.rect));
        if !media.present {
            return Ok(());
        }
        let placed = place_text(
            g,
            &media.artist,
            (font, p.artist_size, 400),
            &p.artist,
            w,
            0.0,
        )?;
        g.draw_text(&placed.layout, placed.x, p.artist.y, dim, shadow);
        self.parts.push((PlayerPart::Artist as u8, placed.rect));

        if p.show_controls {
            let play = if media.playing { ICON_PAUSE } else { ICON_PLAY };
            let buttons = [
                (PlayerPart::Previous, p.previous, ICON_PREVIOUS, 14.0),
                (PlayerPart::Play, p.play, play, 17.0),
                (PlayerPart::Next, p.next, ICON_NEXT, 14.0),
            ];
            for (i, (part, spot, glyph, size)) in buttons.into_iter().enumerate() {
                let button = rect(spot.x, spot.y, BUTTON, BUTTON);
                if self.hover == Some(i) {
                    g.fill_circle(
                        spot.x + BUTTON / 2.0,
                        spot.y + BUTTON / 2.0,
                        BUTTON / 2.0,
                        [1.0, 1.0, 1.0, 0.14 * o],
                    );
                }
                icon(g, glyph, size, button, text)?;
                self.buttons[i] = button;
                self.parts.push((part as u8, button));
            }
        }

        if p.show_progress {
            let position = media.position_now(ctx.now_ms);
            let fraction = if media.duration_ms > 0.0 {
                (position / media.duration_ms).clamp(0.0, 1.0) as f32
            } else {
                0.0
            };
            let bar = rect(p.bar.x, p.bar.y, p.bar_width, BAR_HEIGHT);
            g.fill_round(bar, BAR_HEIGHT / 2.0, [1.0, 1.0, 1.0, 0.18 * o]);
            if fraction > 0.0 {
                let done = (p.bar_width * fraction).max(BAR_HEIGHT);
                g.fill_round(
                    rect(p.bar.x, p.bar.y, done, BAR_HEIGHT),
                    BAR_HEIGHT / 2.0,
                    with_alpha(ctx.accent, o),
                );
            }
            self.bar = bar;
            // Four pixels are too thin to grab, so its handle is taller.
            let handle = rect(
                p.bar.x,
                p.bar.y - BAR_SLOP,
                p.bar_width,
                BAR_HEIGHT + 2.0 * BAR_SLOP,
            );
            self.parts.push((PlayerPart::Bar as u8, handle));

            for (part, spot, label) in [
                (PlayerPart::Elapsed, &p.elapsed, clock_text(position)),
                (PlayerPart::Total, &p.total, clock_text(media.duration_ms)),
            ] {
                let placed = place_text(g, &label, (font, TIME_SIZE, 400), spot, w, 0.0)?;
                g.draw_text(&placed.layout, placed.x, spot.y, dim, shadow);
                self.parts.push((part as u8, placed.rect));
            }
        }
        Ok(())
    }
}
