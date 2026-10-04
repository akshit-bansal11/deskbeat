//! The tray icon's menu: a small card the app draws itself, in its own three
//! colours, with the name and version, a link to the repository, the
//! start-with-Windows switch and Quit. A right-click on the icon opens it;
//! clicking anywhere else, or Escape, closes it.

use deskbeat_core::color::{Rgba, with_alpha};
use deskbeat_core::config::Align;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::UI::WindowsAndMessaging::{
    DestroyWindow, SW_SHOWNORMAL, SetForegroundWindow, ShowWindow,
};
use windows::core::Result;

use crate::gfx::{Gfx, ICON_FONT, Surface, TextStyle, rect};
use crate::window::{self, Mouse};

/// The three colours everything of the app's own is drawn in.
pub const INK: Rgba = [0.075, 0.075, 0.086, 1.0];
pub const PAPER: Rgba = [0.961, 0.957, 0.941, 1.0];
pub const EMBER: Rgba = [1.0, 0.42, 0.271, 1.0];

pub const WIDTH: f32 = 264.0;
pub const HEIGHT: f32 = HEADER + 3.0 * ROW + 2.0 * (GAP + PAD);
const PAD: f32 = 8.0;
const HEADER: f32 = 56.0;
const ROW: f32 = 36.0;
/// The space a dividing line sits in the middle of.
const GAP: f32 = 9.0;
const RADIUS: f32 = 14.0;
const FONT: &str = "Segoe UI Variable Display";
const NAME: &str = "Deskbeat";
const VERSION: &str = concat!("Version ", env!("CARGO_PKG_VERSION"));
const OPEN_GLYPH: &str = "\u{E8A7}";
const ESCAPE: u32 = 27;

/// The mark's bars in its 108-unit drawing: left edge and height. Each is 13
/// wide and centred on the middle line, as in `assets/deskbeat-dark.svg`.
const MARK_BARS: [(f32, f32); 4] = [(21.0, 70.0), (39.0, 64.0), (57.0, 50.0), (75.0, 26.0)];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pick {
    GitHub,
    Autostart,
    Quit,
}

const ROWS: [(Pick, &str, f32); 3] = [
    (Pick::GitHub, "GitHub", PAD + HEADER + GAP),
    (
        Pick::Autostart,
        "Start with Windows",
        PAD + HEADER + GAP + ROW,
    ),
    (Pick::Quit, "Quit", PAD + HEADER + 2.0 * GAP + 2.0 * ROW),
];

fn row_box(top: f32) -> D2D_RECT_F {
    rect(PAD, top, WIDTH - 2.0 * PAD, ROW)
}

/// The row at a point of the card.
fn pick_at((x, y): (f32, f32)) -> Option<Pick> {
    ROWS.iter()
        .find(|(_, _, top)| {
            let r = row_box(*top);
            x >= r.left && x < r.right && y >= r.top && y < r.bottom
        })
        .map(|(pick, _, _)| *pick)
}

/// One line of text vertically centred in `r`.
fn text(
    g: &mut Gfx,
    text: &str,
    r: D2D_RECT_F,
    (font, size, weight): (&str, f32, u32),
    align: Align,
    color: Rgba,
) -> Result<()> {
    let style = TextStyle {
        font,
        size,
        weight,
        align,
        wrap: false,
    };
    let (w, h) = (r.right - r.left, r.bottom - r.top);
    let layout = g.layout(text, &style, w, h)?;
    let height = Gfx::measure(&layout).1;
    g.draw_text(&layout, r.left, r.top + (h - height) / 2.0, color, false);
    Ok(())
}

/// The Deskbeat mark, `side` across, with its top-left corner at `(x, y)`.
fn mark(g: &Gfx, x: f32, y: f32, side: f32) {
    let unit = side / 108.0;
    g.fill_round(rect(x, y, side, side), 26.0 * unit, PAPER);
    for (i, (left, height)) in MARK_BARS.into_iter().enumerate() {
        let color = if i == MARK_BARS.len() - 1 { EMBER } else { INK };
        let bar = rect(
            x + left * unit,
            y + (54.0 - height / 2.0) * unit,
            13.0 * unit,
            height * unit,
        );
        g.fill_round(bar, 6.5 * unit, color);
    }
}

/// Paints the card at the origin, `WIDTH` by `HEIGHT`. `mouse` is the pointer
/// inside it. The caller owns the frame.
pub fn paint(g: &mut Gfx, mouse: (f32, f32), autostart: bool) -> Result<()> {
    g.fill_round(rect(0.0, 0.0, WIDTH, HEIGHT), RADIUS, INK);
    g.stroke_round(
        rect(0.5, 0.5, WIDTH - 1.0, HEIGHT - 1.0),
        RADIUS,
        with_alpha(PAPER, 0.12),
        1.0,
    );

    let side = 32.0;
    let left = PAD + 10.0;
    mark(g, left, PAD + (HEADER - side) / 2.0, side);
    let beside = left + side + 12.0;
    let line = rect(beside, PAD + 9.0, WIDTH - beside - PAD, 20.0);
    text(g, NAME, line, (FONT, 15.0, 600), Align::Left, PAPER)?;
    let dim = with_alpha(PAPER, 0.56);
    let line = rect(beside, PAD + 28.0, WIDTH - beside - PAD, 18.0);
    text(g, VERSION, line, (FONT, 12.0, 400), Align::Left, dim)?;

    let hairline = with_alpha(PAPER, 0.1);
    for top in [PAD + HEADER, PAD + HEADER + GAP + 2.0 * ROW] {
        g.fill_rect(rect(PAD, top + GAP / 2.0, WIDTH - 2.0 * PAD, 1.0), hairline);
    }

    let hot = pick_at(mouse);
    for (pick, label, top) in ROWS {
        let row = row_box(top);
        if hot == Some(pick) {
            g.fill_round(row, 8.0, with_alpha(PAPER, 0.09));
        }
        let inner = rect(row.left + 10.0, row.top, row.right - row.left - 20.0, ROW);
        text(g, label, inner, (FONT, 13.5, 400), Align::Left, PAPER)?;
        let mid = row.top + ROW / 2.0;
        match pick {
            Pick::GitHub => text(
                g,
                OPEN_GLYPH,
                inner,
                (ICON_FONT, 12.0, 400),
                Align::Right,
                dim,
            )?,
            Pick::Autostart => {
                // The same switch as the settings panel's, in the accent.
                let track = rect(inner.right - 34.0, mid - 10.0, 34.0, 20.0);
                let fill = if autostart {
                    EMBER
                } else {
                    with_alpha(PAPER, 0.16)
                };
                g.fill_round(track, 10.0, fill);
                let knob = if autostart {
                    track.right - 10.0
                } else {
                    track.left + 10.0
                };
                g.fill_circle(knob, mid, 7.5, PAPER);
            }
            Pick::Quit => {}
        }
    }
    Ok(())
}

/// The menu while it is open.
pub struct Menu {
    pub hwnd: HWND,
    surface: Surface,
    mouse: (f32, f32),
    pub dirty: bool,
}

impl Menu {
    /// Opens the card with its bottom-right corner at the pointer, kept
    /// inside the work area.
    pub fn open(gfx: &Gfx, scale: f32) -> Result<Self> {
        let (w, h) = (
            (WIDTH * scale).ceil() as i32,
            (HEIGHT * scale).ceil() as i32,
        );
        let (x, y) = window::pointer();
        let (ax, ay, aw, ah) = window::work_area();
        let left = (x - w).clamp(ax, (ax + aw - w).max(ax));
        let top = (y - h).clamp(ay, (ay + ah - h).max(ay));
        let hwnd = window::create_menu((left, top, w, h))?;
        let surface = gfx.surface(hwnd, w as u32, h as u32)?;
        unsafe {
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            // It closes when it stops being the active window, so it has to
            // be the active window first.
            let _ = SetForegroundWindow(hwnd);
        }
        Ok(Self {
            hwnd,
            surface,
            // Off the card, so nothing starts out hovered.
            mouse: (-1.0, -1.0),
            dirty: true,
        })
    }

    pub fn close(&self) {
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }

    /// Follows the pointer. Returns the row a click was released on.
    pub fn mouse(&mut self, kind: Mouse, x: f32, y: f32) -> Option<Pick> {
        let at = if kind == Mouse::Leave {
            (-1.0, -1.0)
        } else {
            (x, y)
        };
        self.dirty |= pick_at(at) != pick_at(self.mouse);
        self.mouse = at;
        if kind == Mouse::Up { pick_at(at) } else { None }
    }

    /// Whether a typed character closes the menu.
    pub fn closes_on(code: u32) -> bool {
        code == ESCAPE
    }

    pub fn draw(&mut self, gfx: &mut Gfx, scale: f32, autostart: bool) -> Result<()> {
        self.dirty = false;
        gfx.begin(&self.surface);
        gfx.set_transform(scale, 0.0, 0.0);
        let painted = paint(gfx, self.mouse, autostart);
        // The frame is ended whether or not painting succeeded.
        gfx.end(&self.surface).and(painted)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn each_row_answers_for_its_own_strip_and_the_header_for_nothing() {
        assert_eq!(pick_at((40.0, PAD + HEADER / 2.0)), None);
        assert_eq!(
            pick_at((40.0, PAD + HEADER + GAP + 1.0)),
            Some(Pick::GitHub)
        );
        assert_eq!(
            pick_at((40.0, PAD + HEADER + GAP + ROW + 1.0)),
            Some(Pick::Autostart)
        );
        assert_eq!(pick_at((40.0, HEIGHT - PAD - 1.0)), Some(Pick::Quit));
        assert_eq!(pick_at((-1.0, -1.0)), None);
        assert_eq!(pick_at((40.0, HEIGHT - 1.0)), None);
    }
}
