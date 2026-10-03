//! The four widgets and what they share.

pub mod clock;
pub mod lyrics;
pub mod player;
pub mod visualizer;

use std::time::Duration;

use sonic_veil_core::color::{Rgba, with_alpha};
use sonic_veil_core::config::{Config, Frame};
use sonic_veil_core::timefmt::LocalTime;
use windows::core::Result;

use crate::capture::Audio;
use crate::gfx::{Gfx, rect};
use crate::lyrics::Lyrics;
use crate::media::MediaState;

/// When a widget next needs to run. The main loop sleeps for as long as the
/// most impatient widget allows, and forever if every widget is `Idle`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Wake {
    /// Nothing will change until an event arrives.
    Idle,
    After(Duration),
    /// Something is animating: run again on the next display refresh.
    Frame,
}

impl Wake {
    pub fn sooner(self, other: Wake) -> Wake {
        match (self, other) {
            (Wake::Frame, _) | (_, Wake::Frame) => Wake::Frame,
            (Wake::After(a), Wake::After(b)) => Wake::After(a.min(b)),
            (Wake::After(a), Wake::Idle) | (Wake::Idle, Wake::After(a)) => Wake::After(a),
            (Wake::Idle, Wake::Idle) => Wake::Idle,
        }
    }

    pub fn after_ms(ms: f64) -> Wake {
        Wake::After(Duration::from_secs_f64(ms.max(1.0) / 1000.0))
    }
}

pub struct Tick {
    /// The widget looks different from what is on screen.
    pub dirty: bool,
    pub wake: Wake,
}

/// Everything a widget may read. Widgets own no app state.
pub struct Ctx<'a> {
    pub cfg: &'a Config,
    pub media: &'a MediaState,
    pub lyrics: &'a Lyrics,
    /// Bumped whenever `lyrics` is replaced.
    pub lyrics_gen: u64,
    pub audio: &'a Audio,
    /// Milliseconds on the app's monotonic clock.
    pub now_ms: f64,
    pub time: LocalTime,
    pub ms_to_next_second: u32,
    pub text: Rgba,
    pub card: Rgba,
    /// The theme accent, already resolved from the album art when set to auto.
    pub accent: Rgba,
}

pub enum Action {
    PlayPause,
    Next,
    Previous,
    /// Seek to this fraction of the track.
    Seek(f64),
}

pub trait Widget {
    /// Advances the widget's state. Cheap, and called far more often than `draw`.
    fn tick(&mut self, ctx: &Ctx) -> Tick;

    /// Draws into a `w` by `h` box whose top-left corner is the origin.
    fn draw(&mut self, g: &mut Gfx, w: f32, h: f32, ctx: &Ctx) -> Result<()>;

    /// Drops everything derived from the config or the graphics device.
    fn reset(&mut self) {}

    /// The pointer moved to `at`, or left. Returns whether to redraw.
    fn hover(&mut self, _at: Option<(f32, f32)>) -> bool {
        false
    }

    fn click(&mut self, _x: f32, _y: f32) -> Option<Action> {
        None
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Clock,
    Player,
    Lyrics,
    Visualizer,
}

impl Kind {
    pub const ALL: [Kind; 4] = [Kind::Visualizer, Kind::Lyrics, Kind::Clock, Kind::Player];

    pub fn label(self) -> &'static str {
        match self {
            Kind::Clock => "Clock",
            Kind::Player => "Player",
            Kind::Lyrics => "Lyrics",
            Kind::Visualizer => "Visualizer",
        }
    }

    pub fn create(self) -> Box<dyn Widget> {
        match self {
            Kind::Clock => Box::new(clock::Clock::default()),
            Kind::Player => Box::new(player::Player::default()),
            Kind::Lyrics => Box::new(lyrics::LyricsView::default()),
            Kind::Visualizer => Box::new(visualizer::Visualizer::new()),
        }
    }

    /// Where the widget goes on a screen `area_w` wide.
    pub fn placed(self, cfg: &Config, area_w: i32) -> Frame {
        match self {
            Kind::Clock => cfg.clock.frame,
            Kind::Player => cfg.player.frame,
            Kind::Lyrics => cfg.lyrics.frame,
            Kind::Visualizer => cfg.visualizer.placed(area_w),
        }
    }

    pub fn frame_mut(self, cfg: &mut Config) -> &mut Frame {
        match self {
            Kind::Clock => &mut cfg.clock.frame,
            Kind::Player => &mut cfg.player.frame,
            Kind::Lyrics => &mut cfg.lyrics.frame,
            Kind::Visualizer => &mut cfg.visualizer.frame,
        }
    }

    pub fn enabled_mut(self, cfg: &mut Config) -> &mut bool {
        match self {
            Kind::Clock => &mut cfg.clock.enabled,
            Kind::Player => &mut cfg.player.enabled,
            Kind::Lyrics => &mut cfg.lyrics.enabled,
            Kind::Visualizer => &mut cfg.visualizer.enabled,
        }
    }

    pub fn enabled(self, cfg: &Config) -> bool {
        match self {
            Kind::Clock => cfg.clock.enabled,
            Kind::Player => cfg.player.enabled,
            Kind::Lyrics => cfg.lyrics.enabled,
            Kind::Visualizer => cfg.visualizer.enabled,
        }
    }

    /// Widgets with nothing to show while Spotify is closed.
    pub fn needs_spotify(self) -> bool {
        self != Kind::Clock
    }

    /// Widgets that take clicks; the rest are click-through.
    pub fn interactive(self) -> bool {
        self == Kind::Player
    }
}

/// The theme's translucent rounded card, filling the widget.
pub fn draw_card(g: &Gfx, w: f32, h: f32, ctx: &Ctx, opacity: f32) {
    let theme = &ctx.cfg.theme;
    let radius = theme.card_radius.min(w.min(h) / 2.0);
    g.fill_round(
        rect(0.0, 0.0, w, h),
        radius,
        with_alpha(ctx.card, theme.card_opacity * opacity),
    );
    if theme.card_border > 0.0 {
        g.stroke_round(
            rect(0.5, 0.5, w - 1.0, h - 1.0),
            radius,
            [1.0, 1.0, 1.0, theme.card_border * opacity],
            1.0,
        );
    }
}

/// `m:ss`, for track positions.
pub fn clock_text(ms: f64) -> String {
    let seconds = (ms.max(0.0) / 1000.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_most_impatient_wake_wins() {
        let short = Wake::After(Duration::from_millis(10));
        let long = Wake::After(Duration::from_millis(900));
        assert_eq!(short.sooner(long), short);
        assert_eq!(Wake::Idle.sooner(long), long);
        assert_eq!(long.sooner(Wake::Frame), Wake::Frame);
        assert_eq!(Wake::Idle.sooner(Wake::Idle), Wake::Idle);
    }

    #[test]
    fn formats_track_positions() {
        assert_eq!(clock_text(0.0), "0:00");
        assert_eq!(clock_text(83_900.0), "1:23");
        assert_eq!(clock_text(-5.0), "0:00");
        assert_eq!(clock_text(3_600_000.0), "60:00");
    }
}
