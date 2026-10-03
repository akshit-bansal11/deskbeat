//! Day, time and date.

use sonic_veil_core::color::with_alpha;
use sonic_veil_core::timefmt::{format, shows_seconds};
use windows::core::Result;

use super::{Ctx, Tick, Wake, Widget, draw_card};
use crate::gfx::{Gfx, TextStyle};

/// The time row's line box is much taller than its digits; this fraction of
/// it is kept so the day and date sit close to the numbers.
const TIME_ROW_TIGHTEN: f32 = 0.84;

#[derive(Default)]
pub struct Clock {
    /// Day, time and date as currently drawn.
    shown: [String; 3],
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

    fn draw(&mut self, g: &mut Gfx, w: f32, h: f32, ctx: &Ctx) -> Result<()> {
        let c = &ctx.cfg.clock;
        let theme = &ctx.cfg.theme;
        if c.card {
            draw_card(g, w, h, ctx, c.opacity);
        }
        let pad = if c.card { 26.0 } else { 4.0 };

        // A font that is missing would be substituted by something unrelated;
        // the theme font is the better stand-in.
        let pick = |wanted: &'_ str| -> String {
            if !wanted.is_empty() && g.has_font(wanted) {
                wanted.to_owned()
            } else {
                theme.font.clone()
            }
        };
        let (day_font, time_font) = (pick(&c.day_font), pick(&c.time_font));

        // Text, font, size, weight, colour, letter spacing, fraction of the row height kept.
        let specs = [
            (
                &self.shown[0],
                &day_font,
                c.day_size,
                600,
                with_alpha(ctx.accent, c.opacity),
                c.day_size * 0.16,
                1.0,
            ),
            (
                &self.shown[1],
                &time_font,
                c.time_size,
                c.time_weight,
                with_alpha(ctx.text, c.opacity),
                0.0,
                TIME_ROW_TIGHTEN,
            ),
            (
                &self.shown[2],
                &time_font,
                c.text_size,
                400,
                with_alpha(ctx.text, 0.74 * c.opacity),
                0.0,
                1.0,
            ),
        ];

        let mut rows = Vec::with_capacity(3);
        let mut total = 0.0;
        for (text, font, size, weight, color, spacing, keep) in specs {
            if text.is_empty() {
                continue;
            }
            let style = TextStyle {
                font,
                size,
                weight,
                align: c.align,
                wrap: false,
            };
            let layout = g.layout(text, &style, w - 2.0 * pad, h)?;
            if spacing > 0.0 {
                Gfx::letter_space(&layout, text.encode_utf16().count() as u32, spacing)?;
            }
            let height = Gfx::measure(&layout).1;
            total += height * keep;
            rows.push((layout, height, keep, color));
        }

        let mut y = (h - total) / 2.0;
        for (layout, height, keep, color) in rows {
            let trimmed = height * (1.0 - keep);
            g.draw_text(
                &layout,
                pad,
                y - trimmed / 2.0,
                color,
                theme.text_shadow && !c.card,
            );
            y += height * keep;
        }
        Ok(())
    }
}
