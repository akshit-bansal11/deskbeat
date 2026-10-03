//! The playback clock: free-runs between position reports and absorbs
//! corrections smoothly, so lyrics never visibly jump or chase jitter.

use crate::timing::{Line, Word};

/// Beyond this the difference is a seek or a track change, not drift.
pub const HARD_RESYNC_THRESHOLD_MS: f64 = 700.0;
/// Time constant of the correction ease. About 8% of the outstanding error
/// per 60 Hz frame, expressed in time so it does not depend on the tick rate.
pub const EASE_TAU_MS: f64 = 180.0;
/// Start each line slightly early. Reading a word a hair before it is sung
/// feels right; a hair after feels broken.
pub const LEAD_IN_MS: f64 = 60.0;
/// A reported position jitters by this much as a matter of course. A
/// correction smaller than this is chasing noise.
pub const DEADBAND_MS: f64 = 90.0;
/// Anchors remembered for the median. Odd, so the median is a real sample.
pub const SAMPLE_WINDOW: usize = 5;
/// Below this the residual correction is imperceptible; stop chasing it.
const SETTLED_MS: f64 = 1.0;

/// A position report from the player.
#[derive(Debug, Clone, Copy)]
pub struct Anchor {
    pub is_playing: bool,
    pub progress_ms: f64,
    /// When the report was true, on the same clock as `now` in [`Clock::apply_anchor`].
    pub sampled_at_ms: f64,
}

#[derive(Debug, Clone, Default)]
pub struct Clock {
    /// Currently displayed position, ms from track start.
    pub position_ms: f64,
    pub is_playing: bool,
    /// User-tunable. Positive makes lyrics appear earlier.
    pub offset_ms: f64,
    /// Outstanding correction, bled off as time passes.
    pub pending_error_ms: f64,
    /// Errors of the most recent anchors, oldest first.
    error_samples: Vec<f64>,
}

impl Clock {
    pub fn new(offset_ms: f64) -> Self {
        Self {
            offset_ms,
            ..Self::default()
        }
    }

    /// Move the clock forward by `dt_ms` of wall time.
    ///
    /// Corrections are eased rather than snapped, because a snap is a visible
    /// micro-jump.
    pub fn advance(&mut self, dt_ms: f64) {
        if self.is_playing {
            self.position_ms += dt_ms;
        }
        if self.pending_error_ms != 0.0 {
            let step = self.pending_error_ms * (1.0 - (-dt_ms / EASE_TAU_MS).exp());
            self.position_ms += step;
            self.pending_error_ms -= step;
            if self.pending_error_ms.abs() < SETTLED_MS {
                self.position_ms += self.pending_error_ms;
                self.pending_error_ms = 0.0;
            }
        }
    }

    /// Fold in a fresh position report.
    ///
    /// No single sample is trusted. The error against each of the last few
    /// anchors is kept, and only their median, and only when it clears the
    /// deadband, becomes a correction. One jittery report therefore moves
    /// nothing; a real, consistent drift is still caught within a few reports.
    pub fn apply_anchor(&mut self, anchor: Anchor, now_ms: f64) {
        let was_playing = self.is_playing;
        self.is_playing = anchor.is_playing;

        // A paused position does not age.
        let age = if anchor.is_playing {
            now_ms - anchor.sampled_at_ms
        } else {
            0.0
        };
        let truth = anchor.progress_ms + age;
        let error = truth - self.position_ms;

        // A seek, a track change, or a play/pause edge: the old samples describe
        // a timeline that no longer exists, so snap and start the window over.
        if error.abs() > HARD_RESYNC_THRESHOLD_MS || was_playing != anchor.is_playing {
            self.position_ms = truth;
            self.pending_error_ms = 0.0;
            self.error_samples.clear();
            return;
        }

        self.error_samples.push(error);
        if self.error_samples.len() > SAMPLE_WINDOW {
            self.error_samples.remove(0);
        }

        let drift = median(&self.error_samples);
        if drift.abs() <= DEADBAND_MS {
            return;
        }

        self.pending_error_ms = drift;
        // The correction is about to be absorbed into the position; shift the
        // samples by the same amount so they do not vote for it a second time.
        for sample in &mut self.error_samples {
            *sample -= drift;
        }
    }

    /// The position every widget should actually read.
    pub fn read(&self) -> f64 {
        self.position_ms + self.offset_ms + LEAD_IN_MS
    }

    /// True while a correction is still being absorbed.
    pub fn is_settling(&self) -> bool {
        self.pending_error_ms != 0.0
    }
}

fn median(values: &[f64]) -> f64 {
    let mut sorted = values.to_vec();
    sorted.sort_by(f64::total_cmp);
    let mid = sorted.len() / 2;
    match sorted.len() {
        0 => 0.0,
        n if n % 2 == 1 => sorted[mid],
        _ => (sorted[mid - 1] + sorted[mid]) / 2.0,
    }
}

/// Index of the last line that has started at time `t`, or `None` before the first.
pub fn find_line_index(lines: &[Line], t: i64) -> Option<usize> {
    lines.partition_point(|l| l.start_ms <= t).checked_sub(1)
}

/// Index of the word being sung at time `t`, or `None` when `t` falls outside every word.
pub fn find_word_index(words: &[Word], t: i64) -> Option<usize> {
    words.iter().position(|w| t >= w.start_ms && t < w.end_ms)
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: f64 = 1_000_000.0;

    fn anchor(is_playing: bool, progress_ms: f64, age_ms: f64) -> Anchor {
        Anchor {
            is_playing,
            progress_ms,
            sampled_at_ms: NOW - age_ms,
        }
    }

    /// A clock already playing at `position_ms`, with no history.
    fn playing_at(position_ms: f64) -> Clock {
        Clock {
            position_ms,
            is_playing: true,
            ..Clock::default()
        }
    }

    fn paused_at(position_ms: f64) -> Clock {
        Clock {
            position_ms,
            ..Clock::default()
        }
    }

    #[test]
    fn advances_with_elapsed_time_while_playing() {
        let mut clock = playing_at(0.0);
        clock.advance(16.0);
        clock.advance(16.0);
        assert_eq!(clock.position_ms, 32.0);
    }

    #[test]
    fn freezes_while_paused() {
        let mut clock = paused_at(5000.0);
        clock.advance(500.0);
        assert_eq!(clock.position_ms, 5000.0);
    }

    #[test]
    fn compensates_for_the_age_of_a_playing_sample() {
        let mut clock = Clock::default();
        // Sample taken 120ms ago reporting 10s: the true position is 10.12s.
        clock.apply_anchor(anchor(true, 10_000.0, 120.0), NOW);
        assert_eq!(clock.position_ms + clock.pending_error_ms, 10_120.0);
    }

    #[test]
    fn does_not_age_a_paused_sample() {
        let mut clock = paused_at(5000.0);
        clock.apply_anchor(anchor(false, 5000.0, 2000.0), NOW);
        assert_eq!(clock.position_ms, 5000.0);
        assert_eq!(clock.pending_error_ms, 0.0);
    }

    #[test]
    fn hard_resyncs_on_a_seek_in_either_direction() {
        let mut clock = playing_at(10_000.0);
        clock.apply_anchor(anchor(true, 60_000.0, 0.0), NOW);
        assert_eq!((clock.position_ms, clock.pending_error_ms), (60_000.0, 0.0));
        clock.apply_anchor(anchor(true, 1000.0, 0.0), NOW);
        assert_eq!((clock.position_ms, clock.pending_error_ms), (1000.0, 0.0));
    }

    #[test]
    fn snaps_on_resume_from_pause_rather_than_easing() {
        let mut clock = paused_at(5000.0);
        clock.apply_anchor(anchor(true, 5150.0, 0.0), NOW);
        assert_eq!((clock.position_ms, clock.pending_error_ms), (5150.0, 0.0));
    }

    #[test]
    fn ignores_jitter_inside_the_deadband() {
        let mut clock = playing_at(10_000.0);
        clock.apply_anchor(anchor(true, 10_000.0 + DEADBAND_MS - 10.0, 0.0), NOW);
        assert_eq!((clock.position_ms, clock.pending_error_ms), (10_000.0, 0.0));
    }

    #[test]
    fn does_not_let_one_outlier_move_the_clock() {
        let mut clock = playing_at(10_000.0);
        for jitter in [0.0, 10.0, 300.0, -10.0] {
            clock.apply_anchor(anchor(true, 10_000.0 + jitter, 0.0), NOW);
            assert_eq!(clock.pending_error_ms, 0.0);
        }
        assert_eq!(clock.position_ms, 10_000.0);
    }

    #[test]
    fn corrects_drift_that_every_sample_agrees_on() {
        let mut clock = playing_at(10_000.0);
        clock.apply_anchor(anchor(true, 10_200.0, 0.0), NOW);
        // Nothing moves until time passes; that is what makes it invisible.
        assert_eq!(clock.position_ms, 10_000.0);
        assert_eq!(clock.pending_error_ms, 200.0);
    }

    #[test]
    fn does_not_apply_the_same_correction_twice() {
        let mut clock = playing_at(10_000.0);
        clock.apply_anchor(anchor(true, 10_200.0, 0.0), NOW);
        for _ in 0..120 {
            clock.advance(16.0);
        }
        // The clock absorbed the 200ms while 1920ms of playback went by. A
        // reading that agrees with that is now on target.
        clock.apply_anchor(anchor(true, 10_200.0 + 1920.0, 0.0), NOW);
        assert!(clock.pending_error_ms.abs() < DEADBAND_MS);
        assert!((clock.position_ms - 12_120.0).abs() < 1.0);
    }

    #[test]
    fn converges_200ms_of_drift_within_twenty_frames() {
        let mut clock = paused_at(10_000.0);
        clock.apply_anchor(anchor(false, 10_200.0, 0.0), NOW);
        for _ in 0..20 {
            clock.advance(16.0);
        }
        assert!(clock.position_ms > 10_160.0);
        assert!((10_200.0 - clock.position_ms).abs() < 40.0);
    }

    #[test]
    fn treats_a_difference_just_over_the_threshold_as_a_seek() {
        let mut clock = playing_at(0.0);
        clock.apply_anchor(anchor(true, HARD_RESYNC_THRESHOLD_MS + 1.0, 0.0), NOW);
        assert_eq!(clock.pending_error_ms, 0.0);
        assert_eq!(clock.position_ms, HARD_RESYNC_THRESHOLD_MS + 1.0);
    }

    #[test]
    fn applies_the_user_offset_and_lead_in_when_read() {
        let mut clock = Clock::new(250.0);
        clock.position_ms = 1000.0;
        assert_eq!(clock.read(), 1000.0 + 250.0 + LEAD_IN_MS);
    }

    fn word(start_ms: i64, end_ms: i64) -> Word {
        Word {
            text: String::new(),
            start_ms,
            end_ms,
            synthesized: true,
        }
    }

    #[test]
    fn finds_the_word_whose_span_contains_the_time() {
        let words = [word(1000, 1400), word(1400, 1900), word(1900, 2600)];
        assert_eq!(find_word_index(&words, 999), None);
        assert_eq!(find_word_index(&words, 1000), Some(0));
        assert_eq!(find_word_index(&words, 1399), Some(0));
        assert_eq!(find_word_index(&words, 1400), Some(1));
        assert_eq!(find_word_index(&words, 2599), Some(2));
        assert_eq!(find_word_index(&words, 2600), None);
        assert_eq!(find_word_index(&[], 1000), None);
    }

    #[test]
    fn finds_the_last_line_that_has_started() {
        let lines: Vec<Line> = [0, 1000, 2000, 3000]
            .iter()
            .map(|&start_ms| Line {
                start_ms,
                end_ms: start_ms + 900,
                words: Vec::new(),
                text: String::new(),
            })
            .collect();
        assert_eq!(find_line_index(&lines, -1), None);
        assert_eq!(find_line_index(&lines, 0), Some(0));
        assert_eq!(find_line_index(&lines, 1500), Some(1));
        assert_eq!(find_line_index(&lines, 2000), Some(2));
        assert_eq!(find_line_index(&lines, 999_999), Some(3));
        assert_eq!(find_line_index(&[], 100), None);
    }
}
