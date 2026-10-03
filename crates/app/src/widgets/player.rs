//! Now playing: art, title, artist, transport buttons and a progress bar.

use sonic_veil_core::color::{Rgba, with_alpha};
use sonic_veil_core::config::{Align, PlayerBackground, PlayerLayout, PlayerPart};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Direct2D::ID2D1Bitmap;
use windows::Win32::Graphics::DirectWrite::IDWriteTextLayout;
use windows::core::Result;

use super::{Action, Ctx, Tick, Wake, Widget, clock_text, draw_card};
use crate::gfx::{Gfx, ICON_FONT, TextStyle, rect};
use crate::media::BLUR_SIDE;

const PAD: f32 = 14.0;
const GAP: f32 = 14.0;
const BUTTON: f32 = 30.0;
const BUTTON_GAP: f32 = 4.0;
/// Space between the parts of the centred layout.
const STACK_GAP: f32 = 10.0;
/// Album art smaller than this is left out rather than drawn as a speck.
const MIN_ART_SIDE: f32 = 24.0;
const TIME_SIZE: f32 = 12.0;
const IDLE_MESSAGE: &str = "Play something on Spotify";
const BAR_HEIGHT: f32 = 4.0;
/// Extra distance above and below the bar that still counts as a click on it.
const BAR_SLOP: f32 = 9.0;

// Segoe Fluent Icons code points.
const ICON_PREVIOUS: &str = "\u{E892}";
const ICON_PLAY: &str = "\u{E768}";
const ICON_PAUSE: &str = "\u{E769}";
const ICON_NEXT: &str = "\u{E893}";
const ICON_NOTE: &str = "\u{E8D6}";

fn contains(r: &D2D_RECT_F, x: f32, y: f32) -> bool {
    x >= r.left && x < r.right && y >= r.top && y < r.bottom
}

pub struct Player {
    art: Option<ID2D1Bitmap>,
    blur: Option<ID2D1Bitmap>,
    /// The art generation the bitmaps were made from.
    art_gen: Option<u64>,
    seen_update: Option<u64>,
    shown_second: i64,
    hover: Option<usize>,
    /// Hit boxes from the last draw: previous, play/pause, next.
    buttons: [D2D_RECT_F; 3],
    bar: D2D_RECT_F,
}

impl Default for Player {
    fn default() -> Self {
        Self {
            art: None,
            blur: None,
            art_gen: None,
            seen_update: None,
            shown_second: -1,
            hover: None,
            buttons: [D2D_RECT_F::default(); 3],
            bar: D2D_RECT_F::default(),
        }
    }
}

impl Widget for Player {
    fn tick(&mut self, ctx: &Ctx) -> Tick {
        let media = ctx.media;
        let position = media.position_now(ctx.now_ms);
        let second = (position / 1000.0) as i64;
        let dirty = self.seen_update != Some(media.update_gen) || second != self.shown_second;
        self.seen_update = Some(media.update_gen);
        self.shown_second = second;

        // The only thing that moves on its own is the progress bar, once a second.
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

        // Background.
        let card = rect(0.0, 0.0, w, h);
        let radius = theme.card_radius.min(h / 2.0);
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
        self.buttons = [D2D_RECT_F::default(); 3];
        self.bar = D2D_RECT_F::default();
        let look = Look {
            text: with_alpha(ctx.text, o),
            dim: with_alpha(ctx.text, 0.66 * o),
            shadow: theme.text_shadow && p.background == PlayerBackground::None,
            opacity: o,
            art_radius: (radius - PAD / 2.0).max(4.0),
        };
        match p.layout {
            PlayerLayout::Row => self.draw_row(g, w, h, ctx, &look),
            PlayerLayout::Centered => self.draw_centered(g, w, h, ctx, &look),
        }
    }
}

/// Colours and flags shared by every part of one draw.
struct Look {
    text: Rgba,
    dim: Rgba,
    shadow: bool,
    opacity: f32,
    art_radius: f32,
}

fn style<'a>(ctx: &'a Ctx, size: f32, weight: u32, align: Align) -> TextStyle<'a> {
    TextStyle {
        font: &ctx.cfg.theme.font,
        size,
        weight,
        align,
        wrap: false,
    }
}

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

impl Player {
    fn draw_art(&self, g: &mut Gfx, frame: D2D_RECT_F, look: &Look) -> Result<()> {
        let side = frame.right - frame.left;
        let radius = look.art_radius.min(side / 2.0);
        match &self.art {
            Some(art) => g.fill_round_bitmap(art, frame, radius, look.opacity),
            None => {
                g.fill_round(frame, radius, [1.0, 1.0, 1.0, 0.09 * look.opacity]);
                icon(g, ICON_NOTE, side * 0.34, frame, look.dim)
            }
        }
    }

    /// The three transport buttons in a row starting at `x`.
    fn draw_controls(
        &mut self,
        g: &mut Gfx,
        x: f32,
        top: f32,
        ctx: &Ctx,
        look: &Look,
    ) -> Result<()> {
        let play = if ctx.media.playing {
            ICON_PAUSE
        } else {
            ICON_PLAY
        };
        for (i, glyph) in [ICON_PREVIOUS, play, ICON_NEXT].into_iter().enumerate() {
            let button = rect(x + i as f32 * (BUTTON + BUTTON_GAP), top, BUTTON, BUTTON);
            if self.hover == Some(i) {
                g.fill_circle(
                    button.left + BUTTON / 2.0,
                    top + BUTTON / 2.0,
                    BUTTON / 2.0,
                    [1.0, 1.0, 1.0, 0.14 * look.opacity],
                );
            }
            let size = if i == 1 { 17.0 } else { 14.0 };
            icon(g, glyph, size, button, look.text)?;
            self.buttons[i] = button;
        }
        Ok(())
    }

    /// The progress bar with its top edge at `top`. Returns the position shown.
    fn draw_bar(
        &mut self,
        g: &mut Gfx,
        x: f32,
        top: f32,
        width: f32,
        ctx: &Ctx,
        look: &Look,
    ) -> f64 {
        let media = ctx.media;
        let position = media.position_now(ctx.now_ms);
        let fraction = if media.duration_ms > 0.0 {
            (position / media.duration_ms).clamp(0.0, 1.0) as f32
        } else {
            0.0
        };
        let bar = rect(x, top, width, BAR_HEIGHT);
        g.fill_round(bar, BAR_HEIGHT / 2.0, [1.0, 1.0, 1.0, 0.18 * look.opacity]);
        if fraction > 0.0 {
            let filled = rect(x, top, (width * fraction).max(BAR_HEIGHT), BAR_HEIGHT);
            g.fill_round(
                filled,
                BAR_HEIGHT / 2.0,
                with_alpha(ctx.accent, look.opacity),
            );
        }
        self.bar = bar;
        position
    }

    /// Title above artist.
    fn draw_text_block(g: &mut Gfx, x: f32, y: f32, block: &TextBlock, look: &Look) {
        g.draw_text(&block.title, x, y, look.text, look.shadow);
        if block.artist_h > 0.0 {
            g.draw_text(&block.artist, x, y + block.title_h, look.dim, look.shadow);
        }
    }

    /// Art on the left; the other parts stacked beside it in the configured
    /// order, spread over the height of the card.
    fn draw_row(&mut self, g: &mut Gfx, w: f32, h: f32, ctx: &Ctx, look: &Look) -> Result<()> {
        let p = &ctx.cfg.player;
        let media = ctx.media;

        let mut x = PAD;
        if p.show_art {
            let side = h - 2.0 * PAD;
            self.draw_art(g, rect(PAD, PAD, side, side), look)?;
            x += side + GAP;
        }
        let width = (w - x - PAD).max(1.0);
        let block = TextBlock::new(g, ctx, width, h, Align::Left)?;

        // Each part beside the art, with the height it needs.
        let available = h - 2.0 * PAD;
        let mut parts: Vec<(PlayerPart, f32)> = p
            .order
            .iter()
            .filter_map(|&part| match part {
                PlayerPart::Text => Some((part, block.height())),
                PlayerPart::Controls if p.show_controls && media.present => Some((part, BUTTON)),
                PlayerPart::Progress if p.show_progress && media.present => {
                    Some((part, TIME_SIZE + 6.0 + BAR_HEIGHT))
                }
                _ => None,
            })
            .collect();
        let total =
            |parts: &[(PlayerPart, f32)]| parts.iter().map(|(_, height)| height).sum::<f32>();
        if total(&parts) > available {
            // A short card has no room for everything; the buttons go first.
            parts.retain(|(part, _)| *part != PlayerPart::Controls);
        }
        let spare = (available - total(&parts)).max(0.0);
        let (mut y, gap) = match parts.len() {
            0 | 1 => (PAD + spare / 2.0, 0.0),
            count => (PAD, spare / (count - 1) as f32),
        };

        for (part, height) in parts {
            match part {
                PlayerPart::Text => Self::draw_text_block(g, x, y, &block, look),
                PlayerPart::Controls => self.draw_controls(g, x - 6.0, y, ctx, look)?,
                PlayerPart::Progress => {
                    let position = self.draw_bar(g, x, y + height - BAR_HEIGHT, width, ctx, look);
                    let label = format!(
                        "{} / {}",
                        clock_text(position),
                        clock_text(media.duration_ms)
                    );
                    let time =
                        g.layout(&label, &style(ctx, TIME_SIZE, 400, Align::Right), width, h)?;
                    g.draw_text(&time, x, y - 2.0, look.dim, look.shadow);
                }
                PlayerPart::Art => {}
            }
            y += height + gap;
        }
        Ok(())
    }

    /// Everything on the centre line, top to bottom in the configured order.
    /// The art takes whatever height the other parts leave.
    fn draw_centered(&mut self, g: &mut Gfx, w: f32, h: f32, ctx: &Ctx, look: &Look) -> Result<()> {
        let p = &ctx.cfg.player;
        let media = ctx.media;
        let inner = (w - 2.0 * PAD).max(1.0);
        let block = TextBlock::new(g, ctx, inner, h, Align::Center)?;

        let mut parts: Vec<(PlayerPart, f32)> = p
            .order
            .iter()
            .filter_map(|&part| match part {
                PlayerPart::Art if p.show_art => Some((part, 0.0)),
                PlayerPart::Text => Some((part, block.height())),
                PlayerPart::Controls if p.show_controls && media.present => Some((part, BUTTON)),
                PlayerPart::Progress if p.show_progress && media.present => {
                    Some((part, BAR_HEIGHT + 6.0 + TIME_SIZE + 4.0))
                }
                _ => None,
            })
            .collect();
        let gaps = STACK_GAP * parts.len().saturating_sub(1) as f32;
        let fixed: f32 = parts.iter().map(|(_, height)| height).sum();
        let side = inner.min(h - 2.0 * PAD - fixed - gaps).max(0.0);
        for (part, height) in &mut parts {
            if *part == PlayerPart::Art {
                *height = side;
            }
        }
        // An art square squeezed to nothing is dropped, with its gap.
        parts.retain(|(part, height)| *part != PlayerPart::Art || *height >= MIN_ART_SIDE);

        let total: f32 = parts.iter().map(|(_, height)| height).sum::<f32>()
            + STACK_GAP * parts.len().saturating_sub(1) as f32;
        let mut y = ((h - total) / 2.0).max(PAD);
        for (part, height) in parts {
            match part {
                PlayerPart::Art => {
                    self.draw_art(g, rect((w - height) / 2.0, y, height, height), look)?;
                }
                PlayerPart::Text => Self::draw_text_block(g, PAD, y, &block, look),
                PlayerPart::Controls => {
                    let row = 3.0 * BUTTON + 2.0 * BUTTON_GAP;
                    self.draw_controls(g, (w - row) / 2.0, y, ctx, look)?;
                }
                PlayerPart::Progress => {
                    let position = self.draw_bar(g, PAD, y, inner, ctx, look);
                    for (text, align) in [
                        (clock_text(position), Align::Left),
                        (clock_text(media.duration_ms), Align::Right),
                    ] {
                        let layout = g.layout(
                            &text,
                            &style(ctx, TIME_SIZE, 400, align),
                            inner,
                            TIME_SIZE + 4.0,
                        )?;
                        g.draw_text(&layout, PAD, y + BAR_HEIGHT + 6.0, look.dim, look.shadow);
                    }
                }
            }
            y += height + STACK_GAP;
        }
        Ok(())
    }
}

/// The title and artist laid out for one width, or the idle message when
/// nothing is playing.
struct TextBlock {
    title: IDWriteTextLayout,
    artist: IDWriteTextLayout,
    title_h: f32,
    /// Zero when there is no artist line to show.
    artist_h: f32,
}

impl TextBlock {
    fn new(g: &mut Gfx, ctx: &Ctx, width: f32, h: f32, align: Align) -> Result<Self> {
        let p = &ctx.cfg.player;
        let media = ctx.media;
        let (title_text, artist_text, title_weight) = if media.present {
            (media.title.as_str(), media.artist.as_str(), 600)
        } else {
            (IDLE_MESSAGE, "", 400)
        };
        let title_size = if media.present {
            p.title_size
        } else {
            p.artist_size
        };
        let mut title_style = style(ctx, title_size, title_weight, align);
        // The idle message is a sentence and may need a second line.
        title_style.wrap = !media.present;
        let title = g.layout(title_text, &title_style, width, h)?;
        let artist = g.layout(
            artist_text,
            &style(ctx, p.artist_size, 400, align),
            width,
            h,
        )?;
        Ok(Self {
            title_h: Gfx::measure(&title).1,
            artist_h: if artist_text.is_empty() {
                0.0
            } else {
                Gfx::measure(&artist).1
            },
            title,
            artist,
        })
    }

    fn height(&self) -> f32 {
        self.title_h + self.artist_h
    }
}
