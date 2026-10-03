//! The settings file. Every field has a default, so a partial or outdated
//! file still loads, and unknown keys are ignored.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    pub general: General,
    pub theme: Theme,
    pub clock: ClockCfg,
    pub player: PlayerCfg,
    pub lyrics: LyricsCfg,
    pub visualizer: VisualizerCfg,
}

/// Where the widgets sit among other windows.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Layer {
    /// On the wallpaper, under every window.
    #[default]
    Desktop,
    /// An ordinary window: whatever is clicked covers it.
    Normal,
    /// Above everything.
    Top,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct General {
    pub layer: Layer,
    /// Stop drawing while a fullscreen app or game is in front.
    pub pause_on_fullscreen: bool,
    /// Hide the player, lyrics and visualizer while Spotify is not running.
    pub hide_without_spotify: bool,
    /// Global hotkeys: Ctrl+Alt+E edit layout, Ctrl+Alt+H hide,
    /// Ctrl+Alt+S settings, Ctrl+Alt+Up/Down lyric offset.
    pub hotkeys: bool,
}

impl Default for General {
    fn default() -> Self {
        Self {
            layer: Layer::Desktop,
            pause_on_fullscreen: true,
            hide_without_spotify: true,
            hotkeys: true,
        }
    }
}

/// Colours are `#RRGGBB` or `#RRGGBBAA`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Theme {
    pub font: String,
    /// `"auto"` takes the colour from the album art; otherwise a hex colour.
    pub accent: String,
    pub text: String,
    pub card_color: String,
    pub card_opacity: f32,
    pub card_radius: f32,
    /// Opacity of the hairline border around cards. 0 removes it.
    pub card_border: f32,
    /// A soft dark shadow under text, so it reads on a bright wallpaper.
    pub text_shadow: bool,
}

impl Default for Theme {
    fn default() -> Self {
        Self {
            font: "Segoe UI Variable Display".to_owned(),
            accent: "auto".to_owned(),
            text: "#FFFFFF".to_owned(),
            card_color: "#101014".to_owned(),
            card_opacity: 0.62,
            card_radius: 22.0,
            card_border: 0.10,
            text_shadow: true,
        }
    }
}

/// The screen corner or edge a widget's position is measured from, so a
/// layout survives a resolution change.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Anchor {
    #[default]
    TopLeft,
    Top,
    TopRight,
    Left,
    Center,
    Right,
    BottomLeft,
    Bottom,
    BottomRight,
}

/// A widget's place on screen, in display-independent pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Frame {
    pub anchor: Anchor,
    /// Inward distance from the anchor's edge; for a centred axis, an offset from centre.
    pub x: i32,
    pub y: i32,
    pub w: u32,
    pub h: u32,
}

impl Default for Frame {
    fn default() -> Self {
        Self::new(Anchor::TopLeft, 48, 48, 400, 160)
    }
}

impl Frame {
    pub const fn new(anchor: Anchor, x: i32, y: i32, w: u32, h: u32) -> Self {
        Self { anchor, x, y, w, h }
    }

    /// Top-left corner inside a work area of `area_w` by `area_h`.
    pub fn origin(&self, area_w: i32, area_h: i32) -> (i32, i32) {
        use Anchor::*;
        let (w, h) = (self.w as i32, self.h as i32);
        let x = match self.anchor {
            TopLeft | Left | BottomLeft => self.x,
            Top | Center | Bottom => (area_w - w) / 2 + self.x,
            TopRight | Right | BottomRight => area_w - w - self.x,
        };
        let y = match self.anchor {
            TopLeft | Top | TopRight => self.y,
            Left | Center | Right => (area_h - h) / 2 + self.y,
            BottomLeft | Bottom | BottomRight => area_h - h - self.y,
        };
        (x, y)
    }

    /// The inverse of [`Frame::origin`]: after a drag to `(left, top)`, the
    /// offsets that keep the current anchor.
    pub fn set_origin(&mut self, left: i32, top: i32, area_w: i32, area_h: i32) {
        let (x, y) = self.origin(area_w, area_h);
        // Moving an offset by one moves the origin by +1 or -1, depending on
        // which edge the anchor measures from.
        let probe = Frame {
            x: self.x + 1,
            y: self.y + 1,
            ..*self
        };
        let (px, py) = probe.origin(area_w, area_h);
        self.x += (left - x) * (px - x);
        self.y += (top - y) * (py - y);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Align {
    #[default]
    Left,
    Center,
    Right,
}

/// Where one element sits inside its widget, in display-independent pixels
/// from the widget's top-left corner. Every element of the clock and the
/// player has one, and each can be dragged on its own in edit mode.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct Spot {
    pub x: f32,
    pub y: f32,
    /// For text: which edge of the text `x` marks. `center` keeps a title
    /// centred on `x` whatever its length.
    pub align: Align,
}

impl Spot {
    pub const fn at(x: f32, y: f32) -> Self {
        Self {
            x,
            y,
            align: Align::Left,
        }
    }

    pub const fn anchored(x: f32, y: f32, align: Align) -> Self {
        Self { x, y, align }
    }

    fn sanitize(&mut self) {
        for value in [&mut self.x, &mut self.y] {
            *value = if value.is_finite() {
                value.clamp(-4000.0, 8000.0)
            } else {
                0.0
            };
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClockRow {
    Day,
    Time,
    Date,
}

impl ClockRow {
    pub const ALL: [ClockRow; 3] = [ClockRow::Day, ClockRow::Time, ClockRow::Date];
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ClockCfg {
    pub enabled: bool,
    pub card: bool,
    pub opacity: f32,
    /// See `timefmt::format` for the specifiers.
    pub day_format: String,
    pub time_format: String,
    pub date_format: String,
    /// Font of the day row. Empty, or a font that is not installed, uses the
    /// theme font.
    pub day_font: String,
    /// Font of the time and date rows, with the same fallback.
    pub time_font: String,
    pub day_size: f32,
    pub time_size: f32,
    /// Size of the date row.
    pub text_size: f32,
    /// Font weight of the time, 100 to 900.
    pub time_weight: u32,
    pub show_day: bool,
    pub show_time: bool,
    pub show_date: bool,
    pub day: Spot,
    pub time: Spot,
    pub date: Spot,
    pub frame: Frame,
}

impl Default for ClockCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            frame: Frame::new(Anchor::TopLeft, 56, 48, 460, 200),
            card: false,
            opacity: 1.0,
            day_format: "%A".to_owned(),
            time_format: "%H:%M".to_owned(),
            date_format: "%e %B %Y".to_owned(),
            // The pairing the Mond Rainmeter skin uses.
            day_font: "Anurati".to_owned(),
            time_font: "Quicksand".to_owned(),
            day_size: 34.0,
            time_size: 96.0,
            text_size: 20.0,
            time_weight: 300,
            show_day: true,
            show_time: true,
            show_date: true,
            day: Spot::at(4.0, 18.0),
            time: Spot::at(4.0, 46.0),
            date: Spot::at(4.0, 156.0),
        }
    }
}

impl ClockCfg {
    pub fn spot_mut(&mut self, row: ClockRow) -> &mut Spot {
        match row {
            ClockRow::Day => &mut self.day,
            ClockRow::Time => &mut self.time,
            ClockRow::Date => &mut self.date,
        }
    }

    /// Puts the three rows back where they start.
    pub fn reset_spots(&mut self) {
        let fresh = ClockCfg::default();
        (self.day, self.time, self.date) = (fresh.day, fresh.time, fresh.date);
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum PlayerBackground {
    /// The theme's card.
    Card,
    /// The album art, blurred and darkened.
    #[default]
    ArtBlur,
    None,
}

/// The separately placed elements of the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerPart {
    Art,
    Title,
    Artist,
    Previous,
    Play,
    Next,
    /// The progress bar.
    Bar,
    /// Time played so far.
    Elapsed,
    /// Length of the track.
    Total,
}

impl PlayerPart {
    pub const ALL: [PlayerPart; 9] = [
        PlayerPart::Art,
        PlayerPart::Title,
        PlayerPart::Artist,
        PlayerPart::Previous,
        PlayerPart::Play,
        PlayerPart::Next,
        PlayerPart::Bar,
        PlayerPart::Elapsed,
        PlayerPart::Total,
    ];
}

/// A ready-made placement of every player element, as a starting point.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlayerLayout {
    /// Art on the left, text and controls beside it.
    Row,
    /// Art, title, artist, controls and progress stacked on the centre line.
    Centered,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PlayerCfg {
    pub enabled: bool,
    pub opacity: f32,
    pub background: PlayerBackground,
    pub show_art: bool,
    pub show_progress: bool,
    pub show_controls: bool,
    pub title_size: f32,
    pub artist_size: f32,
    /// Side of the album art square.
    pub art_size: f32,
    /// Length of the progress bar.
    pub bar_width: f32,
    /// The most room the title and artist take before being cut short.
    pub text_width: f32,
    pub art: Spot,
    pub title: Spot,
    pub artist: Spot,
    pub previous: Spot,
    pub play: Spot,
    pub next: Spot,
    pub bar: Spot,
    pub elapsed: Spot,
    pub total: Spot,
    pub frame: Frame,
}

impl Default for PlayerCfg {
    fn default() -> Self {
        let mut cfg = Self {
            enabled: true,
            // Sits just above the full-width visualizer.
            frame: Frame::new(Anchor::BottomLeft, 56, 196, 420, 132),
            opacity: 1.0,
            background: PlayerBackground::ArtBlur,
            show_art: true,
            show_progress: true,
            show_controls: true,
            title_size: 18.0,
            artist_size: 14.0,
            art_size: 0.0,
            bar_width: 0.0,
            text_width: 270.0,
            art: Spot::default(),
            title: Spot::default(),
            artist: Spot::default(),
            previous: Spot::default(),
            play: Spot::default(),
            next: Spot::default(),
            bar: Spot::default(),
            elapsed: Spot::default(),
            total: Spot::default(),
        };
        cfg.arrange(PlayerLayout::Row);
        cfg
    }
}

impl PlayerCfg {
    pub fn spot_mut(&mut self, part: PlayerPart) -> &mut Spot {
        match part {
            PlayerPart::Art => &mut self.art,
            PlayerPart::Title => &mut self.title,
            PlayerPart::Artist => &mut self.artist,
            PlayerPart::Previous => &mut self.previous,
            PlayerPart::Play => &mut self.play,
            PlayerPart::Next => &mut self.next,
            PlayerPart::Bar => &mut self.bar,
            PlayerPart::Elapsed => &mut self.elapsed,
            PlayerPart::Total => &mut self.total,
        }
    }

    /// Resizes the card and places every element for one of the ready-made
    /// layouts. Elements can be dragged anywhere afterwards.
    pub fn arrange(&mut self, layout: PlayerLayout) {
        use Align::{Center, Right};
        match layout {
            PlayerLayout::Row => {
                (self.frame.w, self.frame.h) = (420, 132);
                self.art_size = 104.0;
                self.bar_width = 274.0;
                self.art = Spot::at(14.0, 14.0);
                self.title = Spot::at(132.0, 11.0);
                self.artist = Spot::at(132.0, 36.0);
                self.previous = Spot::at(126.0, 60.0);
                self.play = Spot::at(160.0, 60.0);
                self.next = Spot::at(194.0, 60.0);
                self.elapsed = Spot::at(132.0, 94.0);
                self.total = Spot::anchored(406.0, 94.0, Right);
                self.bar = Spot::at(132.0, 114.0);
            }
            PlayerLayout::Centered => {
                (self.frame.w, self.frame.h) = (260, 400);
                self.art_size = 232.0;
                self.bar_width = 232.0;
                self.art = Spot::at(14.0, 14.0);
                self.title = Spot::anchored(130.0, 256.0, Center);
                self.artist = Spot::anchored(130.0, 281.0, Center);
                self.previous = Spot::at(81.0, 308.0);
                self.play = Spot::at(115.0, 308.0);
                self.next = Spot::at(149.0, 308.0);
                self.bar = Spot::at(14.0, 352.0);
                self.elapsed = Spot::at(14.0, 362.0);
                self.total = Spot::anchored(246.0, 362.0, Right);
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum LyricsMode {
    /// Highlight the current line. Exact for LRCLIB, which carries line timing.
    #[default]
    Line,
    /// Also highlight the current word. Word timing is usually an estimate.
    Word,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LyricsCfg {
    pub enabled: bool,
    pub card: bool,
    pub opacity: f32,
    pub mode: LyricsMode,
    pub align: Align,
    pub size: f32,
    /// Font weight, 100 to 900.
    pub weight: u32,
    /// Lines shown above and below the current one.
    pub lines_before: u32,
    pub lines_after: u32,
    /// Space between lines, as a fraction of the text size.
    pub line_gap: f32,
    /// Colour of the line being sung. Every lyric colour is `"text"`,
    /// `"accent"`, or a hex colour.
    pub active_color: String,
    /// Colour of the word being sung, in word mode.
    pub word_color: String,
    /// Colour of every other line, before `inactive_opacity` is applied.
    pub inactive_color: String,
    pub inactive_opacity: f32,
    /// Width of the outline around the text. 0 draws none.
    pub stroke_width: f32,
    pub stroke_color: String,
    pub shadow_color: String,
    /// How strong the shadow under the text is. 0 draws none.
    pub shadow_opacity: f32,
    /// Positive shows lyrics earlier.
    pub offset_ms: i32,
    /// Duration of the scroll between lines. 0 disables it.
    pub scroll_ms: u32,
    pub frame: Frame,
}

impl Default for LyricsCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            frame: Frame::new(Anchor::Right, 72, -80, 620, 370),
            card: false,
            opacity: 1.0,
            mode: LyricsMode::Line,
            align: Align::Right,
            size: 30.0,
            weight: 700,
            lines_before: 2,
            lines_after: 2,
            line_gap: 0.55,
            active_color: "text".to_owned(),
            word_color: "accent".to_owned(),
            inactive_color: "text".to_owned(),
            inactive_opacity: 0.38,
            stroke_width: 0.0,
            stroke_color: "#000000".to_owned(),
            shadow_color: "#000000".to_owned(),
            shadow_opacity: 0.38,
            offset_ms: 0,
            scroll_ms: 380,
        }
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisualizerStyle {
    /// Bars rising from the bottom edge.
    #[default]
    Bars,
    /// Bars growing both ways from the centre line.
    Mirror,
    /// A filled curve.
    Wave,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum VisualizerColor {
    /// The theme accent, fading out toward the base.
    #[default]
    Accent,
    /// `color_a` only.
    Solid,
    /// `color_a` at the bass end to `color_b` at the treble end.
    Gradient,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum AudioSource {
    /// Only Spotify's audio.
    #[default]
    Spotify,
    /// Everything the PC plays.
    System,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct VisualizerCfg {
    pub enabled: bool,
    pub card: bool,
    pub opacity: f32,
    pub style: VisualizerStyle,
    pub source: AudioSource,
    pub bars: u32,
    /// Gap between bars as a fraction of the bar pitch.
    pub gap: f32,
    /// Corner radius as a fraction of the bar width.
    pub radius: f32,
    /// Bass in the middle, treble at both edges.
    pub symmetric: bool,
    /// Treble on the left instead of the right.
    pub flip_x: bool,
    /// Bars hang from the top edge instead of standing on the bottom.
    pub flip_y: bool,
    /// Span the whole width of the screen, ignoring the frame's width and
    /// horizontal offset.
    pub full_width: bool,
    pub color: VisualizerColor,
    pub color_a: String,
    pub color_b: String,
    /// Gain in dB. Raise it if the bars barely move.
    pub sensitivity: f32,
    pub attack_ms: f32,
    pub decay_ms: f32,
    pub fps: u32,
    pub min_hz: f32,
    pub max_hz: f32,
    /// Treble boost in dB per octave.
    pub tilt: f32,
    pub frame: Frame,
}

impl Default for VisualizerCfg {
    fn default() -> Self {
        Self {
            enabled: true,
            frame: Frame::new(Anchor::Bottom, 0, 0, 760, 170),
            card: false,
            opacity: 0.9,
            style: VisualizerStyle::Bars,
            source: AudioSource::Spotify,
            bars: 120,
            gap: 0.38,
            radius: 0.5,
            symmetric: false,
            flip_x: false,
            flip_y: false,
            full_width: true,
            color: VisualizerColor::Accent,
            color_a: "#7C5CFF".to_owned(),
            color_b: "#21D4FD".to_owned(),
            sensitivity: 0.0,
            attack_ms: 28.0,
            decay_ms: 190.0,
            fps: 60,
            min_hz: 40.0,
            max_hz: 15_000.0,
            tilt: 3.5,
        }
    }
}

/// More bars than this are thinner than a pixel on any current display.
pub const MAX_BARS: u32 = 512;

impl VisualizerCfg {
    /// Where the visualizer goes on a screen `area_w` wide: its own frame,
    /// or that frame stretched edge to edge when `full_width` is set.
    pub fn placed(&self, area_w: i32) -> Frame {
        if !self.full_width {
            return self.frame;
        }
        use Anchor::*;
        Frame {
            anchor: match self.frame.anchor {
                TopLeft | Top | TopRight => TopLeft,
                Left | Center | Right => Left,
                BottomLeft | Bottom | BottomRight => BottomLeft,
            },
            x: 0,
            w: area_w.max(1) as u32,
            ..self.frame
        }
    }
}

/// One-click looks. A preset only touches appearance, never positions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Preset {
    Glass,
    Minimal,
    Neon,
    Mono,
}

impl Preset {
    pub const ALL: [Preset; 4] = [Preset::Glass, Preset::Minimal, Preset::Neon, Preset::Mono];

    pub fn name(self) -> &'static str {
        match self {
            Preset::Glass => "Glass",
            Preset::Minimal => "Minimal",
            Preset::Neon => "Neon",
            Preset::Mono => "Mono",
        }
    }
}

impl Config {
    pub fn from_toml(text: &str) -> Result<Self, String> {
        toml::from_str::<Config>(text)
            .map(Config::sanitized)
            .map_err(|e| e.to_string())
    }

    pub fn to_toml(&self) -> String {
        // Serializing plain structs of scalars cannot fail.
        toml::to_string_pretty(self).unwrap_or_default()
    }

    /// Clamps every number to a range the renderer can draw. The file is
    /// hand-editable, so a typo must not be able to hang or blank a widget.
    pub fn sanitized(mut self) -> Self {
        fn unit(v: &mut f32) {
            *v = if v.is_finite() {
                v.clamp(0.0, 1.0)
            } else {
                1.0
            };
        }
        fn range(v: &mut f32, lo: f32, hi: f32, fallback: f32) {
            *v = if v.is_finite() {
                v.clamp(lo, hi)
            } else {
                fallback
            };
        }
        fn frame(f: &mut Frame) {
            f.w = f.w.clamp(80, 7680);
            f.h = f.h.clamp(40, 4320);
            f.x = f.x.clamp(-7680, 7680);
            f.y = f.y.clamp(-4320, 4320);
        }

        let t = &mut self.theme;
        unit(&mut t.card_opacity);
        unit(&mut t.card_border);
        range(&mut t.card_radius, 0.0, 80.0, 22.0);

        let c = &mut self.clock;
        frame(&mut c.frame);
        unit(&mut c.opacity);
        range(&mut c.day_size, 8.0, 300.0, 34.0);
        range(&mut c.time_size, 12.0, 400.0, 96.0);
        range(&mut c.text_size, 8.0, 120.0, 20.0);
        c.time_weight = c.time_weight.clamp(100, 900);
        for row in ClockRow::ALL {
            c.spot_mut(row).sanitize();
        }

        let p = &mut self.player;
        frame(&mut p.frame);
        unit(&mut p.opacity);
        range(&mut p.title_size, 8.0, 72.0, 18.0);
        range(&mut p.artist_size, 8.0, 72.0, 14.0);
        range(&mut p.art_size, 0.0, 1000.0, 104.0);
        range(&mut p.bar_width, 0.0, 4000.0, 274.0);
        range(&mut p.text_width, 60.0, 4000.0, 270.0);
        for part in PlayerPart::ALL {
            p.spot_mut(part).sanitize();
        }

        let l = &mut self.lyrics;
        frame(&mut l.frame);
        unit(&mut l.opacity);
        unit(&mut l.inactive_opacity);
        unit(&mut l.shadow_opacity);
        range(&mut l.stroke_width, 0.0, 6.0, 0.0);
        range(&mut l.size, 10.0, 160.0, 30.0);
        range(&mut l.line_gap, 0.0, 3.0, 0.55);
        l.weight = l.weight.clamp(100, 900);
        l.lines_before = l.lines_before.min(8);
        l.lines_after = l.lines_after.min(8);
        l.offset_ms = l.offset_ms.clamp(-10_000, 10_000);
        l.scroll_ms = l.scroll_ms.min(2000);

        let v = &mut self.visualizer;
        frame(&mut v.frame);
        unit(&mut v.opacity);
        range(&mut v.gap, 0.0, 0.9, 0.38);
        range(&mut v.radius, 0.0, 0.5, 0.5);
        range(&mut v.sensitivity, -30.0, 40.0, 0.0);
        range(&mut v.attack_ms, 0.0, 2000.0, 28.0);
        range(&mut v.decay_ms, 0.0, 5000.0, 190.0);
        range(&mut v.min_hz, 20.0, 2000.0, 40.0);
        range(&mut v.max_hz, 2000.0, 22_000.0, 15_000.0);
        range(&mut v.tilt, 0.0, 9.0, 3.5);
        v.bars = v.bars.clamp(4, MAX_BARS);
        v.fps = v.fps.clamp(10, 240);

        self
    }

    pub fn apply_preset(&mut self, preset: Preset) {
        let defaults = Config::default();
        self.theme = Theme {
            font: std::mem::take(&mut self.theme.font),
            ..defaults.theme
        };
        self.clock.card = false;
        self.lyrics.card = false;
        self.visualizer.card = false;
        self.player.background = PlayerBackground::ArtBlur;
        self.visualizer.color = VisualizerColor::Accent;
        self.visualizer.style = VisualizerStyle::Bars;
        self.lyrics.active_color = "text".to_owned();

        match preset {
            Preset::Glass => {
                self.lyrics.card = true;
                self.clock.card = true;
            }
            Preset::Minimal => {
                self.player.background = PlayerBackground::None;
                self.theme.accent = "#FFFFFF".to_owned();
                self.visualizer.style = VisualizerStyle::Mirror;
            }
            Preset::Neon => {
                self.theme.card_color = "#0A0420".to_owned();
                self.theme.card_opacity = 0.78;
                self.theme.accent = "#FF3DCB".to_owned();
                self.visualizer.color = VisualizerColor::Gradient;
                self.visualizer.color_a = "#FF3DCB".to_owned();
                self.visualizer.color_b = "#21D4FD".to_owned();
                self.lyrics.active_color = "accent".to_owned();
                self.player.background = PlayerBackground::Card;
            }
            Preset::Mono => {
                self.theme.accent = "#FFFFFF".to_owned();
                self.theme.card_color = "#000000".to_owned();
                self.theme.card_radius = 6.0;
                self.player.background = PlayerBackground::Card;
                self.visualizer.color = VisualizerColor::Solid;
                self.visualizer.color_a = "#FFFFFF".to_owned();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_empty_file_is_the_default_config() {
        assert_eq!(Config::from_toml("").unwrap(), Config::default());
    }

    #[test]
    fn the_default_config_round_trips_through_toml() {
        let text = Config::default().to_toml();
        assert!(text.contains("[visualizer.frame]"), "{text}");
        assert_eq!(Config::from_toml(&text).unwrap(), Config::default());
    }

    #[test]
    fn a_partial_file_keeps_defaults_and_ignores_unknown_keys() {
        let cfg = Config::from_toml(
            "[lyrics]\nmode = \"word\"\nfuture_key = 1\n\n[visualizer]\nstyle = \"wave\"\nbars = 24\n",
        )
        .unwrap();
        assert_eq!(cfg.lyrics.mode, LyricsMode::Word);
        assert_eq!(cfg.lyrics.size, LyricsCfg::default().size);
        assert_eq!(cfg.visualizer.style, VisualizerStyle::Wave);
        assert_eq!(cfg.visualizer.bars, 24);
        assert_eq!(cfg.clock, ClockCfg::default());
    }

    #[test]
    fn a_syntax_error_is_reported_not_swallowed() {
        assert!(Config::from_toml("[lyrics\nmode = ").is_err());
        assert!(Config::from_toml("[lyrics]\nmode = \"sideways\"").is_err());
    }

    #[test]
    fn out_of_range_numbers_are_clamped() {
        let cfg = Config::from_toml(
            "[visualizer]\nbars = 100000\nfps = 0\ngap = 7.5\n[lyrics]\nsize = -4.0\n[clock.frame]\nw = 1\n",
        )
        .unwrap();
        assert_eq!(cfg.visualizer.bars, MAX_BARS);
        assert_eq!(cfg.visualizer.fps, 10);
        assert_eq!(cfg.visualizer.gap, 0.9);
        assert_eq!(cfg.lyrics.size, 10.0);
        assert_eq!(cfg.clock.frame.w, 80);
    }

    #[test]
    fn anchors_place_a_frame_from_the_right_edge() {
        let area = (1920, 1040);
        let at = |anchor| Frame::new(anchor, 50, 30, 400, 100).origin(area.0, area.1);
        assert_eq!(at(Anchor::TopLeft), (50, 30));
        assert_eq!(at(Anchor::TopRight), (1470, 30));
        assert_eq!(at(Anchor::BottomRight), (1470, 910));
        assert_eq!(at(Anchor::Center), (810, 500));
        assert_eq!(at(Anchor::Bottom), (810, 910));
    }

    #[test]
    fn a_drag_keeps_the_anchor_and_lands_where_dropped() {
        for anchor in [
            Anchor::TopLeft,
            Anchor::Top,
            Anchor::TopRight,
            Anchor::Left,
            Anchor::Center,
            Anchor::Right,
            Anchor::BottomLeft,
            Anchor::Bottom,
            Anchor::BottomRight,
        ] {
            let mut frame = Frame::new(anchor, 50, 30, 400, 100);
            frame.set_origin(123, 456, 1920, 1040);
            assert_eq!(frame.anchor, anchor);
            assert_eq!(frame.origin(1920, 1040), (123, 456), "{anchor:?}");
        }
    }

    #[test]
    fn the_default_layout_fits_a_small_screen_without_overlap() {
        // A 1080p laptop at 150% scaling, less the taskbar: the smallest
        // desktop this is likely to meet.
        let (w, h) = (1280, 672);
        let cfg = Config::default();
        let boxes: Vec<[i32; 4]> = [
            cfg.clock.frame,
            cfg.player.frame,
            cfg.lyrics.frame,
            cfg.visualizer.placed(w),
        ]
        .iter()
        .map(|frame| {
            let (x, y) = frame.origin(w, h);
            [x, y, x + frame.w as i32, y + frame.h as i32]
        })
        .collect();

        for (i, a) in boxes.iter().enumerate() {
            assert!(
                a[0] >= 0 && a[1] >= 0 && a[2] <= w && a[3] <= h,
                "{a:?} is off screen"
            );
            for b in &boxes[i + 1..] {
                let apart = a[2] <= b[0] || b[2] <= a[0] || a[3] <= b[1] || b[3] <= a[1];
                assert!(apart, "{a:?} overlaps {b:?}");
            }
        }
    }

    #[test]
    fn a_full_width_visualizer_spans_the_screen_at_its_own_height() {
        let mut v = VisualizerCfg {
            frame: Frame::new(Anchor::Bottom, 40, 12, 300, 90),
            ..VisualizerCfg::default()
        };
        assert!(v.full_width);
        let placed = v.placed(1920);
        assert_eq!(placed.origin(1920, 1040), (0, 1040 - 90 - 12));
        assert_eq!((placed.w, placed.h), (1920, 90));

        v.full_width = false;
        assert_eq!(v.placed(1920), v.frame);
    }

    #[test]
    fn a_config_from_before_free_placement_still_loads() {
        let cfg = Config::from_toml(
            "[clock]\nalign = \"left\"\norder = [\"date\"]\n[player]\nlayout = \"centered\"\norder = [\"art\"]\n",
        )
        .unwrap();
        assert_eq!(cfg.clock, ClockCfg::default());
        assert_eq!(cfg.player, PlayerCfg::default());
    }

    #[test]
    fn one_element_moves_without_the_others() {
        let cfg = Config::from_toml("[player.title]\nx = 300.0\ny = 5.0\n").unwrap();
        let fresh = PlayerCfg::default();
        assert_eq!((cfg.player.title.x, cfg.player.title.y), (300.0, 5.0));
        assert_eq!(cfg.player.artist, fresh.artist);
        assert_eq!(cfg.player.art, fresh.art);
    }

    #[test]
    fn the_centred_arrangement_centres_text_on_the_card() {
        let mut p = PlayerCfg::default();
        p.arrange(PlayerLayout::Centered);
        assert_eq!(p.title.align, Align::Center);
        assert_eq!(p.title.x * 2.0, p.frame.w as f32);
        assert_eq!(p.art.x * 2.0 + p.art_size, p.frame.w as f32);

        p.arrange(PlayerLayout::Row);
        assert_eq!(p, PlayerCfg::default());
    }

    #[test]
    fn a_position_that_is_not_a_number_is_reset() {
        let cfg = Config::from_toml("[clock.day]\nx = nan\ny = 1e9\n").unwrap();
        assert_eq!((cfg.clock.day.x, cfg.clock.day.y), (0.0, 8000.0));
    }

    #[test]
    fn the_bar_count_is_capped() {
        let cfg = Config::from_toml("[visualizer]\nbars = 100000\n").unwrap();
        assert_eq!(cfg.visualizer.bars, MAX_BARS);
    }

    #[test]
    fn presets_change_the_look_but_never_the_layout_or_font() {
        for preset in Preset::ALL {
            let mut cfg = Config::default();
            cfg.theme.font = "Inter".to_owned();
            cfg.lyrics.frame.x = 999;
            cfg.apply_preset(preset);
            assert_eq!(cfg.theme.font, "Inter", "{}", preset.name());
            assert_eq!(cfg.lyrics.frame.x, 999);
            assert_eq!(cfg.clone().sanitized(), cfg);
        }
    }
}
