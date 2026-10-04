//! Now playing: art, title, artist, transport buttons and progress. Each is
//! placed, sized and styled on its own, wherever the config puts it.

use deskbeat_core::color::{Rgba, with_alpha};
use deskbeat_core::config::{Align, Label, PlayerBackground, PlayerPart};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::core::Result;

use super::{
    Action, Ctx, Part, Tick, Wake, Widget, clock_text, contains, draw_card, draw_placed,
    label_color, named_color, place_label,
};
use crate::gfx::{Gfx, ICON_FONT, TextStyle, rect};
use crate::media::BLUR_SIDE;

/// Extra distance above and below the bar that still counts as on it, for
/// clicking to seek and for grabbing it in edit mode.
const BAR_SLOP: f32 = 9.0;
/// Room for a track time such as 1:23:45, at any size.
const TIME_ROOM: f32 = 600.0;
/// Glyph size as a share of the button: the play glyph is drawn larger.
const GLYPH_SHARE: f32 = 0.47;
const PLAY_GLYPH_SHARE: f32 = 0.57;
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

impl Player {
    /// Draws one text element and records its box.
    fn text(
        &mut self,
        g: &mut Gfx,
        part: PlayerPart,
        text: &str,
        label: &Label,
        room: f32,
        ctx: &Ctx,
    ) -> Result<()> {
        let p = &ctx.cfg.player;
        let shadow = ctx.cfg.theme.text_shadow && p.background == PlayerBackground::None;
        let placed = place_label(g, text, label, ctx, room)?;
        let color = label_color(label, ctx, p.opacity);
        draw_placed(g, &placed, color, shadow);
        self.parts.push((part as u8, placed.rect));
        Ok(())
    }
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
            let art_radius = p.art_size * p.art_radius;
            match &self.art {
                Some(art) => g.fill_round_bitmap(art, frame, art_radius, o)?,
                None => {
                    g.fill_round(frame, art_radius, [1.0, 1.0, 1.0, 0.09 * o]);
                    let dim = with_alpha(ctx.text, 0.66 * o);
                    icon(g, ICON_NOTE, p.art_size * 0.34, frame, dim)?;
                }
            }
            self.parts.push((PlayerPart::Art as u8, frame));
        }

        if !media.present {
            // One line where the title goes, in the quieter artist style.
            let idle = Label {
                x: p.title.x,
                y: p.title.y,
                align: p.title.align,
                ..p.artist.clone()
            };
            return self.text(g, PlayerPart::Title, IDLE_MESSAGE, &idle, p.text_width, ctx);
        }
        self.text(
            g,
            PlayerPart::Title,
            &media.title,
            &p.title,
            p.text_width,
            ctx,
        )?;
        self.text(
            g,
            PlayerPart::Artist,
            &media.artist,
            &p.artist,
            p.text_width,
            ctx,
        )?;

        if p.show_controls {
            let size = p.button_size;
            let color = with_alpha(named_color(&p.button_color, ctx), o);
            let play = if media.playing { ICON_PAUSE } else { ICON_PLAY };
            let buttons = [
                (PlayerPart::Previous, p.previous, ICON_PREVIOUS, GLYPH_SHARE),
                (PlayerPart::Play, p.play, play, PLAY_GLYPH_SHARE),
                (PlayerPart::Next, p.next, ICON_NEXT, GLYPH_SHARE),
            ];
            for (i, (part, spot, glyph, share)) in buttons.into_iter().enumerate() {
                let button = rect(spot.x, spot.y, size, size);
                if self.hover == Some(i) {
                    g.fill_circle(
                        spot.x + size / 2.0,
                        spot.y + size / 2.0,
                        size / 2.0,
                        [1.0, 1.0, 1.0, 0.14 * o],
                    );
                }
                icon(g, glyph, size * share, button, color)?;
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
            let thick = p.bar_height;
            let bar = rect(p.bar.x, p.bar.y, p.bar_width, thick);
            g.fill_round(bar, thick / 2.0, [1.0, 1.0, 1.0, p.bar_track_opacity * o]);
            if fraction > 0.0 {
                let done = (p.bar_width * fraction).max(thick.min(p.bar_width));
                g.fill_round(
                    rect(p.bar.x, p.bar.y, done, thick),
                    thick / 2.0,
                    with_alpha(named_color(&p.bar_color, ctx), o),
                );
            }
            self.bar = bar;
            // A thin bar is hard to grab, so its handle is taller than it.
            let handle = rect(
                p.bar.x,
                p.bar.y - BAR_SLOP,
                p.bar_width,
                thick + 2.0 * BAR_SLOP,
            );
            self.parts.push((PlayerPart::Bar as u8, handle));

            let elapsed = clock_text(position);
            let total = clock_text(media.duration_ms);
            self.text(g, PlayerPart::Elapsed, &elapsed, &p.elapsed, TIME_ROOM, ctx)?;
            self.text(g, PlayerPart::Total, &total, &p.total, TIME_ROOM, ctx)?;
        }
        Ok(())
    }
}
