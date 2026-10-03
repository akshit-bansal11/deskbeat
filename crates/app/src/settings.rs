//! The settings panel: a small immediate-mode UI drawn with the same
//! Direct2D context as the widgets. Each control reads and writes one config
//! field in place, so there is no second copy of the settings to keep in step.
//!
//! Free-form values (a custom hex colour, any installed font, a hand-written
//! clock format) are not editable here; the config file takes those.

use std::ffi::c_void;

use sonic_veil_core::color::{Rgba, hsv_to_rgb, parse_hex, rgb_to_hsv, to_hex, with_alpha};
use sonic_veil_core::config::*;
use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Direct2D::Common::D2D_RECT_F;
use windows::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, DestroyWindow, GetClientRect, SW_SHOWNORMAL, SetForegroundWindow,
    ShowWindow, WINDOW_EX_STYLE, WS_CAPTION, WS_MINIMIZEBOX, WS_OVERLAPPED, WS_SYSMENU,
};
use windows::core::{BOOL, Result, w};

use crate::gfx::{Gfx, Surface, TextStyle, rect};
use crate::widgets::Kind;
use crate::window::{self, Mouse};

pub const WIDTH: f32 = 470.0;
pub const HEIGHT: f32 = 680.0;
const MARGIN: f32 = 22.0;
const TABS_HEIGHT: f32 = 52.0;
const FOOTER_HEIGHT: f32 = 64.0;
const ROW: f32 = 38.0;
const COLOR_ROW: f32 = 62.0;
/// The picker's saturation and brightness square, and the hue bar under it.
const PICKER_SQUARE: (f32, f32) = (210.0, 120.0);
const HUE_BAR_HEIGHT: f32 = 16.0;
/// The six primaries a hue bar passes through, in order.
const HUE_STOPS: [Rgba; 7] = [
    [1.0, 0.0, 0.0, 1.0],
    [1.0, 1.0, 0.0, 1.0],
    [0.0, 1.0, 0.0, 1.0],
    [0.0, 1.0, 1.0, 1.0],
    [0.0, 0.0, 1.0, 1.0],
    [1.0, 0.0, 1.0, 1.0],
    [1.0, 0.0, 0.0, 1.0],
];
/// Share of a row's width given to the control rather than the label.
const CONTROL_SHARE: f32 = 0.56;
const WHEEL_STEP: f32 = 56.0;

const BACKGROUND: Rgba = [0.075, 0.075, 0.095, 1.0];
const TEXT: Rgba = [1.0, 1.0, 1.0, 0.92];
const DIM: Rgba = [1.0, 1.0, 1.0, 0.56];
const TRACK: Rgba = [1.0, 1.0, 1.0, 0.14];

pub const TABS: [&str; 5] = ["General", "Clock", "Player", "Lyrics", "Visualizer"];
const SWATCHES: [&str; 10] = [
    "#FFFFFF", "#1ED760", "#7C5CFF", "#21D4FD", "#FF3DCB", "#FF6B6B", "#FFB547", "#F9F871",
    "#101014", "#000000",
];
/// Fonts every Windows 11 has, offered for any piece of text. Fonts in the
/// app's own folder are listed ahead of these; anything else can be named
/// in the config file.
const SYSTEM_FONTS: [&str; 12] = [
    "Segoe UI Variable Display",
    "Segoe UI",
    "Bahnschrift",
    "Cascadia Code",
    "Consolas",
    "Georgia",
    "Arial",
    "Calibri",
    "Verdana",
    "Trebuchet MS",
    "Times New Roman",
    "Impact",
];
const FONTS: [(&str, &str); 7] = [
    ("Segoe UI Variable Display", "Segoe UI Variable"),
    ("Segoe UI", "Segoe UI"),
    ("Bahnschrift", "Bahnschrift"),
    ("Cascadia Code", "Cascadia Code"),
    ("Georgia", "Georgia"),
    ("Consolas", "Consolas"),
    ("Arial", "Arial"),
];
const TIME_FORMATS: [(&str, &str); 4] = [
    ("%H:%M", "21:47"),
    ("%H:%M:%S", "21:47:09"),
    ("%l:%M %p", "9:47 PM"),
    ("%l:%M", "9:47"),
];
const DAY_FORMATS: [(&str, &str); 4] = [
    ("%A", "Saturday"),
    ("%a", "Sat"),
    ("%A, %e %B", "Saturday, 3 October"),
    ("", "Hidden"),
];
const DATE_FORMATS: [(&str, &str); 5] = [
    ("%e %B %Y", "3 October 2026"),
    ("%B %e, %Y", "October 3, 2026"),
    ("%d/%m/%Y", "03/10/2026"),
    ("%Y-%m-%d", "2026-10-03"),
    ("", "Hidden"),
];
const ANCHORS: [(Anchor, &str); 9] = [
    (Anchor::TopLeft, "Top left"),
    (Anchor::Top, "Top"),
    (Anchor::TopRight, "Top right"),
    (Anchor::Left, "Left"),
    (Anchor::Center, "Centre"),
    (Anchor::Right, "Right"),
    (Anchor::BottomLeft, "Bottom left"),
    (Anchor::Bottom, "Bottom"),
    (Anchor::BottomRight, "Bottom right"),
];
/// Colours a lyric setting can follow by name, ahead of the swatches.
const THEME_COLORS: [(&str, &str); 2] = [("text", "Text"), ("accent", "Accent")];
const ALIGNS: [(Align, &str); 3] = [
    (Align::Left, "Left"),
    (Align::Center, "Centre"),
    (Align::Right, "Right"),
];

/// What the panel needs to know about the app to draw itself.
#[derive(Clone, Copy)]
pub struct Status {
    pub edit: bool,
    pub hidden: bool,
    pub autostart: bool,
}

/// What the user did in the panel that the app has to act on.
#[derive(Default)]
pub struct Outcome {
    /// A setting changed and `cfg` holds the new value.
    pub changed: bool,
    pub toggle_edit: bool,
    pub toggle_hidden: bool,
    pub toggle_autostart: bool,
    pub quit: bool,
    pub open_config: bool,
    /// This pass changed something the next pass has to show.
    pub redraw: bool,
}

/// The panel's contents and pointer state, apart from any window, so the
/// same code paints the real panel and the offscreen snapshot of it.
/// The colour picker, which belongs to at most one colour row at a time.
#[derive(Default)]
struct Picker {
    /// The row it is open under.
    open: Option<u32>,
    /// Hue in degrees, saturation, value. Kept here rather than read back
    /// from the colour: grey and black have no hue, so reading it back would
    /// lose the hue mid-drag.
    hsv: (f32, f32, f32),
    /// The hex field has the keyboard.
    typing: bool,
    /// The hex digits in the field, without the `#`.
    typed: String,
}

impl Picker {
    fn show(&mut self, color: Rgba) {
        self.hsv = rgb_to_hsv(color[0], color[1], color[2]);
        self.typed = to_hex(color)[1..].to_owned();
    }

    /// A typed character: hex digits fill the field, Backspace empties it
    /// one at a time, Enter and Escape give the keyboard back.
    fn key(&mut self, code: u32) {
        if !self.typing {
            return;
        }
        match char::from_u32(code) {
            Some('\u{8}') => {
                self.typed.pop();
            }
            Some('\r' | '\u{1b}') => self.typing = false,
            Some(digit) if digit.is_ascii_hexdigit() && self.typed.len() < 6 => {
                self.typed.push(digit.to_ascii_uppercase());
            }
            _ => {}
        }
    }
}

pub struct View {
    picker: Picker,
    tab: usize,
    scroll: f32,
    /// How far the current tab can scroll, as of the last paint.
    max_scroll: f32,
    mouse: (f32, f32),
    down: bool,
    /// The button went down, or came up, since the last paint.
    pressed: bool,
    released: bool,
    /// The slider being dragged.
    active: Option<u32>,
}

impl Default for View {
    fn default() -> Self {
        Self {
            tab: 0,
            scroll: 0.0,
            max_scroll: 0.0,
            // Off the panel, so nothing starts out hovered.
            mouse: (-1.0, -1.0),
            down: false,
            pressed: false,
            released: false,
            active: None,
            picker: Picker::default(),
        }
    }
}

impl View {
    /// A view on the General tab with the picker open under its first
    /// colour row, for the snapshot: nothing else can click it open there.
    pub fn with_picker_open() -> Self {
        let mut view = Self::default();
        view.picker.open = Some(0);
        view.picker.show([0.486, 0.361, 1.0, 1.0]);
        view
    }

    /// A view opened on one tab, with the pointer off the panel.
    pub fn on_tab(tab: usize) -> Self {
        Self {
            tab,
            ..Self::default()
        }
    }

    /// Paints a `w` by `h` panel at the origin and applies whatever the
    /// pointer did to `cfg`. `area` is the work area in display-independent
    /// pixels. The caller owns the frame: this neither begins nor ends it.
    pub fn paint(
        &mut self,
        gfx: &mut Gfx,
        cfg: &mut Config,
        (w, h): (f32, f32),
        accent: Rgba,
        status: Status,
        area: (i32, i32),
    ) -> Result<Outcome> {
        let mut outcome = Outcome::default();
        let font = cfg.theme.font.clone();
        let own_fonts = gfx.own_fonts();
        gfx.fill_rect(rect(0.0, 0.0, w, h), BACKGROUND);

        let view = (TABS_HEIGHT, h - FOOTER_HEIGHT);
        let mut ui = Ui {
            g: &mut *gfx,
            status,
            toggle_hidden: false,
            toggle_autostart: false,
            quit: false,
            font: &font,
            own_fonts: &own_fonts,
            accent,
            left: MARGIN,
            width: w - 2.0 * MARGIN,
            y: TABS_HEIGHT + 6.0 - self.scroll,
            mouse: self.mouse,
            pressed: self.pressed,
            down: self.down,
            released: self.released,
            active: &mut self.active,
            picker: &mut self.picker,
            next_id: 0,
            changed: false,
            view,
            area,
        };
        // The pointer's edges are consumed by this pass whatever happens next.
        self.pressed = false;
        self.released = false;

        // The clip is always popped, even if a row failed to draw: an
        // unbalanced clip would fail the whole frame.
        ui.g.push_clip(rect(0.0, view.0, w, view.1 - view.0));
        let built = match self.tab {
            0 => general(&mut ui, cfg),
            1 => clock(&mut ui, cfg),
            2 => player(&mut ui, cfg),
            3 => lyrics(&mut ui, cfg),
            _ => visualizer(&mut ui, cfg),
        };
        ui.g.pop_clip();
        built?;
        let content_height = ui.y + self.scroll - TABS_HEIGHT + 12.0;

        // Tabs and footer sit outside the scrolling area.
        ui.view = (0.0, h);
        let tab_width = w / TABS.len() as f32;
        for (i, name) in TABS.iter().enumerate() {
            let cell = rect(tab_width * i as f32, 0.0, tab_width, TABS_HEIGHT - 8.0);
            let selected = self.tab == i;
            if ui.clicked(cell) && !selected {
                self.tab = i;
                self.scroll = 0.0;
                ui.picker.open = None;
                outcome.redraw = true;
            }
            let color = if selected { TEXT } else { DIM };
            let weight = if selected { 600 } else { 400 };
            ui.text(name, cell, 13.0, weight, Align::Center, color)?;
            if selected {
                ui.g.fill_rect(
                    rect(cell.left + 14.0, cell.bottom - 2.0, tab_width - 28.0, 2.0),
                    accent,
                );
            }
        }
        ui.g.fill_rect(rect(0.0, TABS_HEIGHT - 8.0, w, 1.0), TRACK);

        let footer = h - FOOTER_HEIGHT;
        ui.g.fill_rect(rect(0.0, footer, w, 1.0), TRACK);
        let half = (w - 2.0 * MARGIN - 10.0) / 2.0;
        let label = if status.edit {
            "Done editing"
        } else {
            "Edit layout"
        };
        let button = rect(MARGIN, footer + 14.0, half, 36.0);
        outcome.toggle_edit = ui.button(label, button, status.edit)?;
        outcome.toggle_hidden = ui.toggle_hidden;
        outcome.toggle_autostart = ui.toggle_autostart;
        outcome.quit = ui.quit;
        outcome.open_config = ui.button(
            "Open config file",
            rect(MARGIN + half + 10.0, footer + 14.0, half, 36.0),
            false,
        )?;
        outcome.changed = ui.changed;

        let max_scroll = (content_height - (view.1 - view.0)).max(0.0);
        self.max_scroll = max_scroll;
        let clamped = self.scroll.clamp(0.0, max_scroll);
        outcome.redraw |= outcome.changed || clamped != self.scroll;
        self.scroll = clamped;
        Ok(outcome)
    }
}

/// The settings window.
pub struct Panel {
    pub hwnd: HWND,
    surface: Surface,
    size: (u32, u32),
    view: View,
    pub dirty: bool,
}

fn client_size(hwnd: HWND) -> (u32, u32) {
    let mut r = RECT::default();
    let _ = unsafe { GetClientRect(hwnd, &mut r) };
    (r.right.max(1) as u32, r.bottom.max(1) as u32)
}

impl Panel {
    pub fn open(gfx: &Gfx, scale: f32) -> Result<Self> {
        let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
        let mut outer = RECT {
            left: 0,
            top: 0,
            right: (WIDTH * scale) as i32,
            bottom: (HEIGHT * scale) as i32,
        };
        unsafe { AdjustWindowRectEx(&mut outer, style, false, WINDOW_EX_STYLE(0))? };
        let (w, h) = (outer.right - outer.left, outer.bottom - outer.top);
        let (ax, ay, aw, ah) = window::work_area();
        let bounds = (ax + (aw - w) / 2, ay + (ah - h) / 2, w, h);

        let hwnd = window::create_panel(w!("Sonic Veil settings"), style, bounds)?;
        let dark = BOOL(1);
        unsafe {
            // Cosmetic: without it the title bar is white above a dark panel.
            let _ = DwmSetWindowAttribute(
                hwnd,
                DWMWA_USE_IMMERSIVE_DARK_MODE,
                (&raw const dark).cast::<c_void>(),
                size_of::<BOOL>() as u32,
            );
            let _ = ShowWindow(hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(hwnd);
        }
        let size = client_size(hwnd);
        Ok(Self {
            hwnd,
            surface: gfx.surface(hwnd, size.0, size.1)?,
            size,
            view: View::default(),
            dirty: true,
        })
    }

    pub fn close(&self) {
        let _ = unsafe { DestroyWindow(self.hwnd) };
    }

    pub fn focus(&self) {
        unsafe {
            let _ = ShowWindow(self.hwnd, SW_SHOWNORMAL);
            let _ = SetForegroundWindow(self.hwnd);
        }
    }

    /// After a graphics device reset.
    pub fn rebuild(&mut self, gfx: &Gfx) -> Result<()> {
        self.surface = gfx.surface(self.hwnd, self.size.0, self.size.1)?;
        self.dirty = true;
        Ok(())
    }

    pub fn mouse(&mut self, kind: Mouse, x: f32, y: f32) {
        let view = &mut self.view;
        match kind {
            Mouse::Down => {
                view.down = true;
                view.pressed = true;
                view.mouse = (x, y);
            }
            Mouse::Up => {
                view.down = false;
                view.released = true;
                view.mouse = (x, y);
            }
            Mouse::Move => view.mouse = (x, y),
            Mouse::Leave => {
                if !view.down {
                    view.mouse = (-1.0, -1.0);
                }
            }
        }
        self.dirty = true;
    }

    /// A typed character, for the colour picker's hex field.
    pub fn key(&mut self, code: u32) {
        self.view.picker.key(code);
        self.dirty = true;
    }

    pub fn wheel(&mut self, delta: i32) {
        // Clamped here, before anything is drawn. Clamping after a paint drew
        // one frame scrolled past the end and the next one snapped back.
        let view = &mut self.view;
        let wanted = (view.scroll - delta as f32 / 120.0 * WHEEL_STEP).clamp(0.0, view.max_scroll);
        if wanted != view.scroll {
            view.scroll = wanted;
            self.dirty = true;
        }
    }

    /// Draws the panel into its window.
    pub fn draw(
        &mut self,
        gfx: &mut Gfx,
        cfg: &mut Config,
        scale: f32,
        accent: Rgba,
        status: Status,
        area: (i32, i32),
    ) -> Result<Outcome> {
        let size = (self.size.0 as f32 / scale, self.size.1 as f32 / scale);
        gfx.begin(&self.surface);
        gfx.set_transform(scale, 0.0, 0.0);
        let painted = self.view.paint(gfx, cfg, size, accent, status, area);
        // The frame is ended whether or not painting succeeded.
        let ended = gfx.end(&self.surface);
        let outcome = painted?;
        ended?;
        self.dirty = outcome.redraw;
        Ok(outcome)
    }
}

/// One frame of the immediate-mode UI.
struct Ui<'a> {
    g: &'a mut Gfx,
    status: Status,
    /// What the General tab's app controls asked for.
    toggle_hidden: bool,
    toggle_autostart: bool,
    quit: bool,
    font: &'a str,
    /// Families loaded from the app's own fonts folder.
    own_fonts: &'a [String],
    accent: Rgba,
    left: f32,
    width: f32,
    /// Top of the next row.
    y: f32,
    mouse: (f32, f32),
    pressed: bool,
    down: bool,
    released: bool,
    active: &'a mut Option<u32>,
    picker: &'a mut Picker,
    next_id: u32,
    changed: bool,
    /// The vertical band in which controls are visible and take input.
    view: (f32, f32),
    area: (i32, i32),
}

impl Ui<'_> {
    fn hot(&self, r: D2D_RECT_F) -> bool {
        let (x, y) = self.mouse;
        x >= r.left && x < r.right && y >= r.top.max(self.view.0) && y < r.bottom.min(self.view.1)
    }

    fn clicked(&self, r: D2D_RECT_F) -> bool {
        self.released && self.active.is_none() && self.hot(r)
    }

    /// Draws one line of text vertically centred in `r`.
    fn text(
        &mut self,
        text: &str,
        r: D2D_RECT_F,
        size: f32,
        weight: u32,
        align: Align,
        color: Rgba,
    ) -> Result<()> {
        let style = TextStyle {
            font: self.font,
            size,
            weight,
            align,
            wrap: false,
        };
        let (w, h) = (r.right - r.left, r.bottom - r.top);
        let layout = self.g.layout(text, &style, w, h)?;
        let height = Gfx::measure(&layout).1;
        self.g
            .draw_text(&layout, r.left, r.top + (h - height) / 2.0, color, false);
        Ok(())
    }

    /// Claims the next row. Returns the whole row and the control's part of it.
    fn row(&mut self, label: &str, height: f32) -> Result<(D2D_RECT_F, D2D_RECT_F)> {
        let row = rect(self.left, self.y, self.width, height);
        self.y += height;
        let label_width = self.width * (1.0 - CONTROL_SHARE);
        self.text(
            label,
            rect(row.left, row.top, label_width - 8.0, ROW),
            13.5,
            400,
            Align::Left,
            TEXT,
        )?;
        let control = rect(
            row.left + label_width,
            row.top,
            self.width - label_width,
            height,
        );
        Ok((row, control))
    }

    fn header(&mut self, title: &str) -> Result<()> {
        self.y += 10.0;
        let r = rect(self.left, self.y, self.width, 26.0);
        self.y += 28.0;
        self.text(
            &title.to_uppercase(),
            r,
            11.0,
            700,
            Align::Left,
            self.accent,
        )
    }

    fn button(&mut self, label: &str, r: D2D_RECT_F, lit: bool) -> Result<bool> {
        let fill = if lit {
            with_alpha(self.accent, 0.9)
        } else if self.hot(r) {
            [1.0, 1.0, 1.0, 0.2]
        } else {
            TRACK
        };
        self.g.fill_round(r, 9.0, fill);
        let color = if lit { [0.0, 0.0, 0.0, 0.9] } else { TEXT };
        self.text(label, r, 13.0, 600, Align::Center, color)?;
        Ok(self.clicked(r))
    }

    fn toggle(&mut self, label: &str, value: &mut bool) -> Result<()> {
        let (row, control) = self.row(label, ROW)?;
        if self.clicked(row) {
            *value = !*value;
            self.changed = true;
        }
        let mid = row.top + ROW / 2.0;
        let track = rect(control.right - 40.0, mid - 11.0, 40.0, 22.0);
        let fill = if *value { self.accent } else { TRACK };
        self.g.fill_round(track, 11.0, fill);
        let knob = if *value {
            track.right - 11.0
        } else {
            track.left + 11.0
        };
        self.g.fill_circle(knob, mid, 8.0, [1.0; 4]);
        Ok(())
    }

    /// A slider over `min..=max` that snaps to `step`.
    fn slider(
        &mut self,
        label: &str,
        value: &mut f32,
        min: f32,
        max: f32,
        step: f32,
    ) -> Result<()> {
        let id = self.next_id;
        self.next_id += 1;
        let (row, control) = self.row(label, ROW)?;
        let mid = row.top + ROW / 2.0;
        let track = rect(
            control.left,
            mid - 2.0,
            control.right - control.left - 56.0,
            4.0,
        );
        let grab = rect(
            track.left - 8.0,
            row.top,
            track.right - track.left + 16.0,
            ROW,
        );

        if self.pressed && self.active.is_none() && self.hot(grab) {
            *self.active = Some(id);
        }
        if *self.active == Some(id) {
            if self.down || self.released {
                let t = ((self.mouse.0 - track.left) / (track.right - track.left)).clamp(0.0, 1.0);
                let snapped = (min + ((max - min) * t / step).round() * step).clamp(min, max);
                if snapped != *value {
                    *value = snapped;
                    self.changed = true;
                }
            }
            if !self.down {
                *self.active = None;
            }
        }

        let t = ((*value - min) / (max - min)).clamp(0.0, 1.0);
        let knob = track.left + (track.right - track.left) * t;
        self.g.fill_round(track, 2.0, TRACK);
        self.g.fill_round(
            rect(track.left, track.top, knob - track.left, 4.0),
            2.0,
            self.accent,
        );
        self.g.fill_circle(knob, mid, 7.0, [1.0; 4]);

        let shown = if step >= 1.0 {
            format!("{value:.0}")
        } else {
            format!("{value:.2}")
        };
        let value_box = rect(control.right - 50.0, row.top, 50.0, ROW);
        self.text(&shown, value_box, 12.5, 400, Align::Right, DIM)
    }

    fn slider_u32(
        &mut self,
        label: &str,
        value: &mut u32,
        min: u32,
        max: u32,
        step: u32,
    ) -> Result<()> {
        let mut float = *value as f32;
        self.slider(label, &mut float, min as f32, max as f32, step as f32)?;
        *value = float.round() as u32;
        Ok(())
    }

    /// Steps through `options` with a click on either half of the control.
    fn stepper(
        &mut self,
        label: &str,
        current: Option<usize>,
        labels: &[&str],
    ) -> Result<Option<usize>> {
        let (_, control) = self.row(label, ROW)?;
        let pill = rect(
            control.left,
            control.top + 5.0,
            control.right - control.left,
            ROW - 10.0,
        );
        self.g.fill_round(
            pill,
            9.0,
            if self.hot(pill) {
                [1.0, 1.0, 1.0, 0.2]
            } else {
                TRACK
            },
        );
        let shown = current.map_or("Custom", |i| labels[i]);
        self.text(shown, pill, 13.0, 500, Align::Center, TEXT)?;
        let arrow = |x: f32| rect(x, pill.top, 28.0, pill.bottom - pill.top);
        self.text("‹", arrow(pill.left), 15.0, 400, Align::Center, DIM)?;
        self.text("›", arrow(pill.right - 28.0), 15.0, 400, Align::Center, DIM)?;

        if !self.clicked(pill) || labels.is_empty() {
            return Ok(None);
        }
        let count = labels.len();
        let forward = self.mouse.0 >= (pill.left + pill.right) / 2.0;
        Ok(Some(match (current, forward) {
            (Some(i), true) => (i + 1) % count,
            (Some(i), false) => (i + count - 1) % count,
            (None, _) => 0,
        }))
    }

    /// Picks one of a few values of an enum.
    fn choice<T: Copy + PartialEq>(
        &mut self,
        label: &str,
        value: &mut T,
        options: &[(T, &str)],
    ) -> Result<()> {
        let labels: Vec<&str> = options.iter().map(|(_, name)| *name).collect();
        let current = options.iter().position(|(option, _)| option == value);
        if let Some(index) = self.stepper(label, current, &labels)? {
            *value = options[index].0;
            self.changed = true;
        }
        Ok(())
    }

    /// Picks one of a few preset strings; anything else shows as "Custom".
    fn choice_text(
        &mut self,
        label: &str,
        value: &mut String,
        options: &[(&str, &str)],
    ) -> Result<()> {
        let labels: Vec<&str> = options.iter().map(|(_, name)| *name).collect();
        let current = options
            .iter()
            .position(|(option, _)| *option == value.as_str());
        if let Some(index) = self.stepper(label, current, &labels)? {
            *value = options[index].0.to_owned();
            self.changed = true;
        }
        Ok(())
    }

    /// A colour: named options such as "Accent", a row of swatches, and a
    /// last chip that opens a picker for any colour at all.
    fn color(&mut self, label: &str, value: &mut String, named: &[(&str, &str)]) -> Result<()> {
        // One id for the row, one each for the picker's square and hue bar.
        let id = self.next_id;
        self.next_id += 3;
        let (row, _) = self.row(label, COLOR_ROW)?;
        let mut x = row.left;
        let top = row.top + ROW - 4.0;

        for (option, name) in named {
            let pill = rect(x, top, 58.0, 22.0);
            let selected = value.as_str() == *option;
            if self.button(name, pill, selected)? && !selected {
                *value = (*option).to_owned();
                self.changed = true;
            }
            x += 64.0;
        }
        let mut preset = false;
        for swatch in SWATCHES {
            let chip = rect(x, top, 22.0, 22.0);
            let selected = value.eq_ignore_ascii_case(swatch);
            preset |= selected;
            if self.clicked(chip) && !selected {
                *value = swatch.to_owned();
                self.changed = true;
            }
            if selected {
                self.ring(chip);
            }
            self.g
                .fill_round(chip, 6.0, parse_hex(swatch).unwrap_or([1.0; 4]));
            self.g.stroke_round(chip, 6.0, [1.0, 1.0, 1.0, 0.25], 1.0);
            x += 26.0;
        }

        // The last chip is any other colour. It shows the current one when
        // that is not a swatch, and a spectrum otherwise.
        let chip = rect(x, top, 22.0, 22.0);
        let custom = parse_hex(value).filter(|_| !preset);
        let open = self.picker.open == Some(id);
        if self.clicked(chip) {
            self.picker.open = (!open).then_some(id);
            self.picker.typing = false;
            self.picker
                .show(parse_hex(value).unwrap_or([1.0, 1.0, 1.0, 1.0]));
        }
        match custom {
            Some(color) => self.g.fill_round(chip, 6.0, color),
            None => {
                let spectrum =
                    self.g
                        .gradient(chip.left, chip.right, HUE_STOPS[5], HUE_STOPS[3])?;
                self.g.fill_round_with(chip, 6.0, &spectrum);
            }
        }
        self.g.stroke_round(chip, 6.0, [1.0, 1.0, 1.0, 0.25], 1.0);
        if custom.is_some() || open {
            self.ring(chip);
        }
        if self.picker.open == Some(id) {
            self.picker_body(id, value)?;
        }
        Ok(())
    }

    /// The accent outline around a selected swatch.
    fn ring(&mut self, chip: D2D_RECT_F) {
        let around = rect(chip.left - 2.5, chip.top - 2.5, 27.0, 27.0);
        self.g.stroke_round(around, 8.0, self.accent, 2.0);
    }

    /// The open picker: a square for saturation and brightness, a bar for
    /// hue, and a hex field that takes typing.
    fn picker_body(&mut self, id: u32, value: &mut String) -> Result<()> {
        let (width, height) = PICKER_SQUARE;
        let top = self.y + 2.0;
        let square = rect(self.left, top, width, height);
        let bar = rect(self.left, top + height + 10.0, width, HUE_BAR_HEIGHT);
        let field = rect(
            self.left + width + 18.0,
            top,
            self.width - width - 18.0,
            34.0,
        );
        self.y = bar.bottom + 14.0;

        // Dragging in the square or along the bar.
        let (mx, my) = self.mouse;
        if self.pressed && self.active.is_none() {
            if self.hot(square) {
                *self.active = Some(id + 1);
            } else if self.hot(rect(bar.left, bar.top - 4.0, width, HUE_BAR_HEIGHT + 8.0)) {
                *self.active = Some(id + 2);
            }
            self.picker.typing = self.hot(field);
        }
        let across = ((mx - square.left) / width).clamp(0.0, 1.0);
        let dragging = *self.active == Some(id + 1) || *self.active == Some(id + 2);
        if dragging {
            if self.down || self.released {
                let (hue, saturation, brightness) = &mut self.picker.hsv;
                if *self.active == Some(id + 1) {
                    *saturation = across;
                    *brightness = 1.0 - ((my - square.top) / height).clamp(0.0, 1.0);
                } else {
                    *hue = across * 359.9;
                }
                let (hue, saturation, brightness) = self.picker.hsv;
                let (r, g, b) = hsv_to_rgb(hue, saturation, brightness);
                let hex = to_hex([r, g, b, 1.0]);
                self.picker.typed = hex[1..].to_owned();
                if hex != *value {
                    *value = hex;
                    self.changed = true;
                }
            }
            if !self.down {
                *self.active = None;
            }
        } else if self.picker.typing {
            // Six digits make a colour; fewer are still being typed.
            if self.picker.typed.len() == 6
                && let Some(color) = parse_hex(&self.picker.typed)
                && !to_hex(color).eq_ignore_ascii_case(value)
            {
                *value = to_hex(color);
                self.picker.hsv = rgb_to_hsv(color[0], color[1], color[2]);
                self.changed = true;
            }
        } else if let Some(color) = parse_hex(value)
            && !self.picker.typed.eq_ignore_ascii_case(&to_hex(color)[1..])
        {
            // The colour was changed some other way, by a swatch say.
            self.picker.show(color);
        }

        // The square: white to the pure hue across, darkening to black downward.
        let (hue, saturation, brightness) = self.picker.hsv;
        let (r, g, b) = hsv_to_rgb(hue, 1.0, 1.0);
        let across_brush = self
            .g
            .gradient(square.left, square.right, [1.0; 4], [r, g, b, 1.0])?;
        self.g.fill_round_with(square, 8.0, &across_brush);
        let down_brush = self.g.vertical_gradient(
            square.top,
            square.bottom,
            [0.0, 0.0, 0.0, 0.0],
            [0.0, 0.0, 0.0, 1.0],
        )?;
        self.g.fill_round_with(square, 8.0, &down_brush);
        let (knob_x, knob_y) = (
            square.left + saturation * width,
            square.top + (1.0 - brightness) * height,
        );
        self.g.fill_circle(knob_x, knob_y, 7.0, [1.0; 4]);
        let (r, g, b) = hsv_to_rgb(hue, saturation, brightness);
        self.g.fill_circle(knob_x, knob_y, 5.0, [r, g, b, 1.0]);

        // The hue bar, one gradient between each pair of primaries.
        let step = width / 6.0;
        for (i, pair) in HUE_STOPS.windows(2).enumerate() {
            let left = bar.left + step * i as f32;
            // A hair of overlap hides the seam between segments.
            let segment = rect(left, bar.top, step + 0.5, HUE_BAR_HEIGHT);
            let brush = self.g.gradient(left, left + step, pair[0], pair[1])?;
            self.g.fill_round_with(segment, 0.0, &brush);
        }
        let hue_x = bar.left + hue / 360.0 * width;
        self.g.fill_round(
            rect(hue_x - 3.0, bar.top - 3.0, 6.0, HUE_BAR_HEIGHT + 6.0),
            3.0,
            [1.0; 4],
        );

        // The hex field, with the colour itself beneath it.
        self.g.fill_round(field, 8.0, TRACK);
        if self.picker.typing {
            self.g.stroke_round(field, 8.0, self.accent, 1.5);
        }
        let caret = if self.picker.typing { "_" } else { "" };
        let shown = format!("#{}{caret}", self.picker.typed);
        self.text(&shown, field, 14.0, 600, Align::Center, TEXT)?;
        let preview = rect(
            field.left,
            field.bottom + 8.0,
            field.right - field.left,
            46.0,
        );
        self.g.fill_round(preview, 8.0, [r, g, b, 1.0]);
        self.g
            .stroke_round(preview, 8.0, [1.0, 1.0, 1.0, 0.25], 1.0);
        let hint = rect(
            field.left,
            preview.bottom + 6.0,
            field.right - field.left,
            34.0,
        );
        self.text(
            "Click the code to type one",
            hint,
            11.5,
            400,
            Align::Center,
            DIM,
        )
    }

    /// Picks a font: the theme's, one from the app's own folder, or one
    /// that ships with Windows.
    fn font_row(&mut self, label: &str, value: &mut String) -> Result<()> {
        let own = self.own_fonts;
        let options: Vec<(&str, &str)> = std::iter::once(("", "Theme font"))
            .chain(own.iter().map(|name| (name.as_str(), name.as_str())))
            .chain(SYSTEM_FONTS.iter().map(|name| (*name, *name)))
            .collect();
        self.choice_text(label, value, &options)
    }

    /// Every setting of one piece of text.
    fn label_rows(&mut self, label: &mut Label) -> Result<()> {
        self.font_row("Font", &mut label.font)?;
        self.slider("Size", &mut label.size, 6.0, 240.0, 1.0)?;
        self.slider_u32("Weight", &mut label.weight, 100, 900, 100)?;
        self.color("Colour", &mut label.color, &THEME_COLORS)?;
        self.slider("Opacity", &mut label.opacity, 0.0, 1.0, 0.02)?;
        self.slider("Letter spacing", &mut label.spacing, 0.0, 1.0, 0.02)?;
        self.toggle("Capitals", &mut label.uppercase)?;
        self.choice("Grows from its", &mut label.align, &ALIGNS)
    }

    /// A labelled row with one button. Returns whether it was clicked.
    fn action(&mut self, label: &str, button: &str) -> Result<bool> {
        let (_, control) = self.row(label, ROW)?;
        let pill = rect(
            control.left,
            control.top + 5.0,
            control.right - control.left,
            ROW - 10.0,
        );
        self.button(button, pill, false)
    }

    /// A line of small print.
    fn note(&mut self, text: &str) -> Result<()> {
        let line = rect(self.left, self.y, self.width, 22.0);
        self.y += 26.0;
        self.text(text, line, 12.0, 400, Align::Left, DIM)
    }

    /// The rows every widget shares: where it sits and how solid it is.
    fn placement(
        &mut self,
        kind: Kind,
        cfg: &mut Config,
        opacity: fn(&mut Config) -> &mut f32,
    ) -> Result<()> {
        self.header("Placement")?;
        let frame = kind.frame_mut(cfg);
        let before = frame.origin(self.area.0, self.area.1);
        let mut anchor = frame.anchor;
        self.choice("Measured from", &mut anchor, &ANCHORS)?;
        if anchor != frame.anchor {
            // Keep the widget where it is; only what its offsets mean changes.
            frame.anchor = anchor;
            frame.set_origin(before.0, before.1, self.area.0, self.area.1);
        }
        // The clock and the player are as big as their elements make them.
        if !kind.fitted() {
            // Up to the size of the screen itself.
            let (max_w, max_h) = (self.area.0.max(80) as u32, self.area.1.max(40) as u32);
            self.slider_u32("Width", &mut frame.w, 80, max_w, 10)?;
            self.slider_u32("Height", &mut frame.h, 40, max_h, 10)?;
        }
        self.slider("Opacity", opacity(cfg), 0.0, 1.0, 0.05)
    }
}

fn general(ui: &mut Ui, cfg: &mut Config) -> Result<()> {
    ui.header("Looks")?;
    let width = (ui.width - 30.0) / 4.0;
    for (i, preset) in Preset::ALL.into_iter().enumerate() {
        let button = rect(ui.left + (width + 10.0) * i as f32, ui.y, width, 34.0);
        if ui.button(preset.name(), button, false)? {
            cfg.apply_preset(preset);
            ui.changed = true;
        }
    }
    ui.y += 42.0;

    ui.header("Theme")?;
    let theme = &mut cfg.theme;
    ui.choice_text("Font", &mut theme.font, &FONTS)?;
    ui.color("Accent", &mut theme.accent, &[("auto", "Album")])?;
    ui.color("Text", &mut theme.text, &[])?;
    ui.color("Card", &mut theme.card_color, &[])?;
    ui.slider("Card opacity", &mut theme.card_opacity, 0.0, 1.0, 0.02)?;
    ui.slider("Card corner", &mut theme.card_radius, 0.0, 40.0, 1.0)?;
    ui.slider("Card border", &mut theme.card_border, 0.0, 0.5, 0.01)?;
    ui.toggle("Shadow under text", &mut theme.text_shadow)?;

    ui.header("Behaviour")?;
    let general = &mut cfg.general;
    ui.choice(
        "Layer",
        &mut general.layer,
        &[
            (Layer::Desktop, "On the desktop"),
            (Layer::Normal, "Normal window"),
            (Layer::Top, "Always on top"),
        ],
    )?;
    ui.toggle(
        "Pause behind fullscreen apps",
        &mut general.pause_on_fullscreen,
    )?;
    ui.toggle(
        "Hide when Spotify is closed",
        &mut general.hide_without_spotify,
    )?;
    ui.toggle("Global hotkeys", &mut general.hotkeys)?;

    // The tray icon has no menu, so what a menu would hold lives here.
    ui.header("App")?;
    let (mut hidden, mut autostart) = (ui.status.hidden, ui.status.autostart);
    ui.toggle("Hide all widgets", &mut hidden)?;
    ui.toggle("Start with Windows", &mut autostart)?;
    ui.toggle_hidden = hidden != ui.status.hidden;
    ui.toggle_autostart = autostart != ui.status.autostart;
    if ui.action("Sonic Veil", "Quit")? {
        ui.quit = true;
    }
    Ok(())
}

fn clock(ui: &mut Ui, cfg: &mut Config) -> Result<()> {
    ui.header("Clock")?;
    let c = &mut cfg.clock;
    ui.toggle("Show", &mut c.enabled)?;
    ui.toggle("Card behind it", &mut c.card)?;
    ui.note("In Edit layout, drag the day, time and date separately.")?;
    if ui.action("Positions", "Reset")? {
        c.reset_positions();
        ui.changed = true;
    }

    ui.header("Day")?;
    ui.toggle("Show", &mut c.show_day)?;
    ui.choice_text("Shows", &mut c.day_format, &DAY_FORMATS)?;
    ui.label_rows(&mut c.day)?;

    ui.header("Time")?;
    ui.toggle("Show", &mut c.show_time)?;
    ui.choice_text("Shows", &mut c.time_format, &TIME_FORMATS)?;
    ui.label_rows(&mut c.time)?;

    ui.header("Date")?;
    ui.toggle("Show", &mut c.show_date)?;
    ui.choice_text("Shows", &mut c.date_format, &DATE_FORMATS)?;
    ui.label_rows(&mut c.date)?;

    ui.placement(Kind::Clock, cfg, |cfg| &mut cfg.clock.opacity)
}

fn player(ui: &mut Ui, cfg: &mut Config) -> Result<()> {
    ui.header("Player")?;
    let p = &mut cfg.player;
    ui.toggle("Show", &mut p.enabled)?;
    ui.choice(
        "Background",
        &mut p.background,
        &[
            (PlayerBackground::ArtBlur, "Blurred album art"),
            (PlayerBackground::Card, "Card"),
            (PlayerBackground::None, "None"),
        ],
    )?;

    ui.header("Arrangement")?;
    ui.note("In Edit layout, drag any element on its own.")?;
    for (layout, label) in [
        (PlayerLayout::Row, "Art on the left"),
        (PlayerLayout::Centered, "Centred stack"),
    ] {
        if ui.action(label, "Arrange")? {
            p.arrange(layout);
            ui.changed = true;
        }
    }

    ui.header("Album art")?;
    ui.toggle("Show", &mut p.show_art)?;
    ui.slider("Size", &mut p.art_size, 16.0, 600.0, 2.0)?;
    ui.slider("Corner rounding", &mut p.art_radius, 0.0, 0.5, 0.02)?;

    ui.header("Progress bar")?;
    ui.toggle("Show bar and times", &mut p.show_progress)?;
    ui.slider("Length", &mut p.bar_width, 20.0, 1200.0, 2.0)?;
    ui.slider("Thickness", &mut p.bar_height, 1.0, 40.0, 1.0)?;
    ui.color("Colour", &mut p.bar_color, &THEME_COLORS)?;
    ui.slider("Unplayed part", &mut p.bar_track_opacity, 0.0, 1.0, 0.02)?;

    ui.header("Buttons")?;
    ui.toggle("Show", &mut p.show_controls)?;
    ui.slider("Size", &mut p.button_size, 14.0, 120.0, 1.0)?;
    ui.color("Colour", &mut p.button_color, &THEME_COLORS)?;

    ui.header("Title")?;
    ui.label_rows(&mut p.title)?;
    ui.header("Artist")?;
    ui.label_rows(&mut p.artist)?;
    ui.slider(
        "Room for title and artist",
        &mut p.text_width,
        60.0,
        1200.0,
        10.0,
    )?;
    ui.header("Time played")?;
    ui.label_rows(&mut p.elapsed)?;
    ui.header("Track length")?;
    ui.label_rows(&mut p.total)?;

    ui.placement(Kind::Player, cfg, |cfg| &mut cfg.player.opacity)
}

fn lyrics(ui: &mut Ui, cfg: &mut Config) -> Result<()> {
    ui.header("Lyrics")?;
    let l = &mut cfg.lyrics;
    ui.toggle("Show", &mut l.enabled)?;
    ui.toggle("Card behind them", &mut l.card)?;
    ui.choice(
        "Highlight",
        &mut l.mode,
        &[
            (LyricsMode::Line, "Line by line"),
            (LyricsMode::Word, "Word by word"),
        ],
    )?;
    ui.choice("Align", &mut l.align, &ALIGNS)?;
    ui.font_row("Font", &mut l.font)?;
    ui.slider("Text size", &mut l.size, 12.0, 96.0, 1.0)?;
    ui.slider_u32("Weight", &mut l.weight, 100, 900, 100)?;
    ui.slider_u32("Lines above", &mut l.lines_before, 0, 6, 1)?;
    ui.slider_u32("Lines below", &mut l.lines_after, 0, 6, 1)?;
    ui.slider("Line spacing", &mut l.line_gap, 0.0, 2.0, 0.05)?;

    ui.header("Colours")?;
    ui.color("Current line", &mut l.active_color, &THEME_COLORS)?;
    ui.color("Current word", &mut l.word_color, &THEME_COLORS)?;
    ui.color("Other lines", &mut l.inactive_color, &THEME_COLORS)?;
    ui.slider(
        "Other lines opacity",
        &mut l.inactive_opacity,
        0.0,
        1.0,
        0.02,
    )?;

    ui.header("Outline and shadow")?;
    ui.slider("Outline width", &mut l.stroke_width, 0.0, 6.0, 0.5)?;
    ui.color("Outline colour", &mut l.stroke_color, &[])?;
    ui.slider("Shadow strength", &mut l.shadow_opacity, 0.0, 1.0, 0.02)?;
    ui.color("Shadow colour", &mut l.shadow_color, &[])?;

    ui.header("Timing")?;
    let mut offset = l.offset_ms as f32;
    ui.slider("Show earlier (ms)", &mut offset, -3000.0, 3000.0, 50.0)?;
    l.offset_ms = offset as i32;
    ui.slider_u32("Scroll time (ms)", &mut l.scroll_ms, 0, 1000, 20)?;
    ui.placement(Kind::Lyrics, cfg, |cfg| &mut cfg.lyrics.opacity)
}

fn visualizer(ui: &mut Ui, cfg: &mut Config) -> Result<()> {
    ui.header("Visualizer")?;
    let v = &mut cfg.visualizer;
    ui.toggle("Show", &mut v.enabled)?;
    ui.toggle("Card behind it", &mut v.card)?;
    ui.choice(
        "Style",
        &mut v.style,
        &[
            (VisualizerStyle::Bars, "Bars"),
            (VisualizerStyle::Mirror, "Mirrored bars"),
            (VisualizerStyle::Wave, "Wave"),
        ],
    )?;
    ui.choice(
        "Listens to",
        &mut v.source,
        &[
            (AudioSource::Spotify, "Spotify only"),
            (AudioSource::System, "Everything"),
        ],
    )?;
    ui.toggle("Span the whole screen", &mut v.full_width)?;
    ui.slider_u32("Bars", &mut v.bars, 4, MAX_BARS, 1)?;
    ui.slider("Gap", &mut v.gap, 0.0, 0.9, 0.02)?;
    ui.slider("Roundness", &mut v.radius, 0.0, 0.5, 0.05)?;
    ui.toggle("Bass in the middle", &mut v.symmetric)?;
    ui.toggle("Flip left to right", &mut v.flip_x)?;
    ui.toggle("Flip upside down", &mut v.flip_y)?;

    ui.header("Colour")?;
    ui.choice(
        "Colour",
        &mut v.color,
        &[
            (VisualizerColor::Accent, "Accent"),
            (VisualizerColor::Solid, "One colour"),
            (VisualizerColor::Gradient, "Gradient"),
        ],
    )?;
    ui.color("First colour", &mut v.color_a, &[])?;
    ui.color("Second colour", &mut v.color_b, &[])?;

    ui.header("Motion")?;
    ui.slider("Sensitivity (dB)", &mut v.sensitivity, -20.0, 30.0, 1.0)?;
    ui.slider("Rise (ms)", &mut v.attack_ms, 0.0, 200.0, 2.0)?;
    ui.slider("Fall (ms)", &mut v.decay_ms, 20.0, 1000.0, 10.0)?;
    ui.slider_u32("Frames per second", &mut v.fps, 15, 144, 1)?;
    ui.slider("Treble boost", &mut v.tilt, 0.0, 9.0, 0.5)?;
    ui.slider("Lowest pitch (Hz)", &mut v.min_hz, 20.0, 500.0, 10.0)?;
    ui.slider("Highest pitch (Hz)", &mut v.max_hz, 4000.0, 20_000.0, 500.0)?;
    ui.placement(Kind::Visualizer, cfg, |cfg| &mut cfg.visualizer.opacity)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn typing(start: &str) -> Picker {
        Picker {
            typing: true,
            typed: start.to_owned(),
            ..Picker::default()
        }
    }

    #[test]
    fn the_hex_field_takes_six_hex_digits_and_nothing_else() {
        let mut picker = typing("");
        for ch in "#1d b9zz54ff".chars() {
            picker.key(ch as u32);
        }
        assert_eq!(picker.typed, "1DB954");
    }

    #[test]
    fn backspace_removes_a_digit_and_enter_gives_the_keyboard_back() {
        let mut picker = typing("1DB954");
        picker.key(8);
        assert_eq!(picker.typed, "1DB95");
        picker.key(13);
        assert!(!picker.typing);
        // No longer typing: keys are ignored.
        picker.key('A' as u32);
        assert_eq!(picker.typed, "1DB95");
    }

    #[test]
    fn showing_a_colour_fills_the_field_and_keeps_its_hue() {
        let mut picker = Picker::default();
        picker.show([1.0, 0.0, 0.0, 1.0]);
        assert_eq!(picker.typed, "FF0000");
        assert_eq!(picker.hsv, (0.0, 1.0, 1.0));
    }
}
