//! Day, time and date, each placed on its own.

use sonic_veil_core::color::with_alpha;
use sonic_veil_core::config::ClockRow;
use sonic_veil_core::timefmt::{format, shows_seconds};
use windows::core::Result;

use super::{Ctx, Part, Tick, Wake, Widget, draw_card, place_text};
use crate::gfx::Gfx;

/// A clock row is never cut short: the window grows to hold it.
const ROOM: f32 = 4000.0;

#[derive(Default)]
pub struct Clock {
    /// Day, time and date as currently drawn.
    shown: [String; 3],
    /// Every row drawn last time, for dragging in edit mode.
    parts: Vec<Part>,
}

impl Widget for Clock {
    fn tick(&mut self, ctx: &Ctx) -> Tick {
        let c = &ctx.cfg.clock;
        let rows = [
            format(&c.day_format, &ctx.time).to_uppercase(),
            format(&c.time_format, &ctx.time),
            format(&c.date_format, &ctx.time),
        ];
        let dirty = rows != self.shown;
        self.shown = rows;

        // Sleep until the next second or minute turns over, not a frame sooner.
        let every_second = [&c.day_format, &c.time_format, &c.date_format]
            .iter()
            .any(|f| shows_seconds(f));
        let whole_seconds = if every_second {
            0
        } else {
            59u32.saturating_sub(ctx.time.second)
        };
        let wait = ctx.ms_to_next_second + whole_seconds * 1000 + 20;
        Tick {
            dirty,
            wake: Wake::after_ms(f64::from(wait)),
        }
    }

    fn parts(&self) -> &[Part] {
        &self.parts
    }

    fn draw(&mut self, g: &mut Gfx, w: f32, h: f32, ctx: &Ctx) -> Result<()> {
        let c = &ctx.cfg.clock;
        let theme = &ctx.cfg.theme;
        if c.card {
            draw_card(g, w, h, ctx, c.opacity);
        }

        // A font that is missing would be substituted by something unrelated;
        // the theme font is the better stand-in.
        let pick = |wanted: &str| -> String {
            if !wanted.is_empty() && g.has_font(wanted) {
                wanted.to_owned()
            } else {
                theme.font.clone()
            }
        };
        let (day_font, time_font) = (pick(&c.day_font), pick(&c.time_font));
        let shadow = theme.text_shadow && !c.card;

        self.parts.clear();
        for row in ClockRow::ALL {
            // Shown, font, size, weight, colour, letter spacing, position.
            let (shown, font, size, weight, color, spacing, spot) = match row {
                ClockRow::Day => (
                    c.show_day,
                    &day_font,
                    c.day_size,
                    600,
                    with_alpha(ctx.accent, c.opacity),
                    c.day_size * 0.16,
                    &c.day,
                ),
                ClockRow::Time => (
                    c.show_time,
                    &time_font,
                    c.time_size,
                    c.time_weight,
                    with_alpha(ctx.text, c.opacity),
                    0.0,
                    &c.time,
                ),
                ClockRow::Date => (
                    c.show_date,
                    &time_font,
                    c.text_size,
                    400,
                    with_alpha(ctx.text, 0.74 * c.opacity),
                    0.0,
                    &c.date,
                ),
            };
            let text = &self.shown[row as usize];
            if !shown || text.is_empty() {
                continue;
            }
            let placed = place_text(g, text, (font.as_str(), size, weight), spot, ROOM, spacing)?;
            g.draw_text(&placed.layout, placed.x, spot.y, color, shadow);
            self.parts.push((row as u8, placed.rect));
        }
        Ok(())
    }
}
