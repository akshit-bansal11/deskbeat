//! Day, time and date, each placed and styled on its own.

use deskbeat_core::config::ClockRow;
use deskbeat_core::timefmt::{format, shows_seconds};
use windows::core::Result;

use super::{Ctx, Part, Tick, Wake, Widget, draw_card, label_color, place_label};
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
            format(&c.day_format, &ctx.time),
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
        if c.card {
            draw_card(g, w, h, ctx, c.opacity);
        }
        let shadow = ctx.cfg.theme.text_shadow && !c.card;

        self.parts.clear();
        for row in ClockRow::ALL {
            let shown = match row {
                ClockRow::Day => c.show_day,
                ClockRow::Time => c.show_time,
                ClockRow::Date => c.show_date,
            };
            let text = &self.shown[row as usize];
            if !shown || text.is_empty() {
                continue;
            }
            let label = c.label(row);
            let placed = place_label(g, text, label, ctx, ROOM)?;
            let color = label_color(label, ctx, c.opacity);
            g.draw_text(&placed.layout, placed.x, label.y, color, shadow);
            self.parts.push((row as u8, placed.rect));
        }
        Ok(())
    }
}
