//! Turns raw parsed lines into fully bounded lines and words.

use crate::lrc::RawLine;

/// Floor so a short word never flashes by unreadably.
pub const MIN_WORD_MS: i64 = 90;
/// A line never stays lit longer than this, however long the gap to the next one.
pub const MAX_LINE_MS: i64 = 8000;

#[derive(Debug, Clone, PartialEq)]
pub struct Word {
    pub text: String,
    pub start_ms: i64,
    /// Always greater than `start_ms`.
    pub end_ms: i64,
    /// True when the timing was synthesized rather than sourced.
    pub synthesized: bool,
    /// True for a syllable that runs straight into the next one, with no
    /// space between them. Only sourced timing splits words this finely.
    pub joined: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Line {
    pub start_ms: i64,
    pub end_ms: i64,
    pub words: Vec<Word>,
    /// The words as displayed: see `join_words`.
    pub text: String,
}

impl Line {
    /// Whether the time each word is sung came from a source. A line whose
    /// word times were worked out from its own length has none to show.
    pub fn word_timed(&self) -> bool {
        self.words.iter().any(|word| !word.synthesized)
    }
}

/// The text of a line: a space after every word, except between the
/// syllables of one word.
pub fn join_words(words: &[Word]) -> String {
    let mut text = String::new();
    for (i, word) in words.iter().enumerate() {
        text += &word.text;
        if !word.joined && i + 1 < words.len() {
            text.push(' ');
        }
    }
    text
}

/// Distribute a line's duration across its words by character weight.
///
/// Every word is first given `MIN_WORD_MS`, and only the surplus is weighted.
/// That guarantees the slices sum to exactly the line duration while never
/// dropping a word below the floor. Weighting first and clamping afterwards
/// does not, and overruns the line end on any line mixing very long and very
/// short words.
///
/// When the requested duration is too short to give every word its floor, the
/// span is widened; the caller takes the line's end from the last word.
pub fn synthesize_word_timings(text: &str, line_start_ms: i64, line_end_ms: i64) -> Vec<Word> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.is_empty() {
        return Vec::new();
    }

    let floor_total = MIN_WORD_MS * tokens.len() as i64;
    let duration = (line_end_ms - line_start_ms).max(floor_total);
    let surplus = (duration - floor_total) as f64;

    // +1 approximates the inter-word pause; longer words take longer to sing.
    let weight = |t: &str| (t.chars().count() + 1) as f64;
    let total_weight: f64 = tokens.iter().map(|&t| weight(t)).sum();

    let end_of_line = line_start_ms + duration;
    let mut cursor = line_start_ms;

    tokens
        .iter()
        .enumerate()
        .map(|(i, &token)| {
            let slice = MIN_WORD_MS as f64 + weight(token) / total_weight * surplus;
            let start_ms = cursor;
            // Pin the final end rather than accumulating, so rounding cannot drift past the line.
            let end_ms = if i == tokens.len() - 1 {
                end_of_line
            } else {
                (cursor as f64 + slice).round() as i64
            };
            cursor = end_ms;
            Word {
                text: token.to_owned(),
                start_ms,
                end_ms,
                synthesized: true,
                joined: false,
            }
        })
        .collect()
}

/// Turn enhanced-LRC per-word start times into fully bounded words.
fn bound_sourced_words(
    text: &str,
    starts_ms: &[i64],
    line_start_ms: i64,
    line_end_ms: i64,
) -> Vec<Word> {
    let tokens: Vec<&str> = text.split_whitespace().collect();
    if tokens.is_empty() || tokens.len() != starts_ms.len() {
        return Vec::new();
    }

    tokens
        .iter()
        .enumerate()
        .map(|(i, token)| {
            let start_ms = starts_ms[i].max(line_start_ms);
            let raw_end = starts_ms.get(i + 1).copied().unwrap_or(line_end_ms);
            Word {
                text: (*token).to_owned(),
                start_ms,
                end_ms: raw_end.max(start_ms + MIN_WORD_MS),
                synthesized: false,
                joined: false,
            }
        })
        .collect()
}

/// Derive line ends, attach words, and enforce every invariant the renderer relies on.
///
/// The clamp to `MAX_LINE_MS` is what stops a line before a 40-second
/// instrumental break from staying lit for the whole break.
pub fn normalize_lines(raw: &[RawLine], track_duration_ms: i64) -> Vec<Line> {
    let mut sorted: Vec<&RawLine> = raw.iter().filter(|l| !l.text.trim().is_empty()).collect();
    sorted.sort_by_key(|l| l.start_ms);

    let mut lines = Vec::with_capacity(sorted.len());
    for (i, line) in sorted.iter().enumerate() {
        let hard_cap = line.start_ms + MAX_LINE_MS;
        let natural_end = match sorted.get(i + 1) {
            Some(next) => next.start_ms.min(hard_cap),
            None if track_duration_ms > line.start_ms => track_duration_ms.min(hard_cap),
            None => hard_cap,
        };

        let words = match &line.word_starts_ms {
            Some(starts) if !starts.is_empty() => {
                bound_sourced_words(&line.text, starts, line.start_ms, natural_end)
            }
            _ => synthesize_word_timings(&line.text, line.start_ms, natural_end),
        };
        let (Some(first), Some(last)) = (words.first(), words.last()) else {
            continue;
        };

        lines.push(Line {
            start_ms: first.start_ms,
            end_ms: last.end_ms,
            text: join_words(&words),
            words,
        });
    }
    lines
}

/// Reports the first broken invariant. A normalization bug that reaches the
/// renderer produces subtly wrong animation that is very hard to debug by eye.
pub fn check_invariants(lines: &[Line]) -> Result<(), String> {
    for (i, line) in lines.iter().enumerate() {
        let (Some(first), Some(last)) = (line.words.first(), line.words.last()) else {
            return Err(format!("line {i} has no words"));
        };
        if first.start_ms != line.start_ms {
            return Err(format!("line {i} start != first word start"));
        }
        if last.end_ms != line.end_ms {
            return Err(format!("line {i} end != last word end"));
        }
        if line.end_ms <= line.start_ms {
            return Err(format!("line {i} has non-positive duration"));
        }
        if let Some(w) = line.words.iter().position(|w| w.end_ms <= w.start_ms) {
            return Err(format!("line {i} word {w} has non-positive span"));
        }
        if i > 0 && line.start_ms < lines[i - 1].start_ms {
            return Err(format!("line {i} is out of order"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn raw(start_ms: i64, text: &str) -> RawLine {
        RawLine {
            start_ms,
            text: text.to_owned(),
            word_starts_ms: None,
        }
    }

    fn sourced(start_ms: i64, text: &str, starts: &[i64]) -> RawLine {
        RawLine {
            start_ms,
            text: text.to_owned(),
            word_starts_ms: Some(starts.to_vec()),
        }
    }

    #[test]
    fn covers_the_line_exactly_with_no_gaps() {
        let words = synthesize_word_timings("one two three four", 1000, 5000);
        assert_eq!(words[0].start_ms, 1000);
        assert_eq!(words.last().unwrap().end_ms, 5000);
        for pair in words.windows(2) {
            assert_eq!(pair[1].start_ms, pair[0].end_ms);
        }
    }

    #[test]
    fn weights_longer_words_with_more_time() {
        let words = synthesize_word_timings("a extraordinarily", 0, 4000);
        let span = |w: &Word| w.end_ms - w.start_ms;
        assert!(span(&words[1]) > span(&words[0]));
    }

    #[test]
    fn respects_the_floor_when_the_line_is_too_short() {
        // Ten words in 100ms is impossible at the floor, so the span widens instead.
        let words = synthesize_word_timings("a b c d e f g h i j", 0, 100);
        assert_eq!(words.len(), 10);
        assert!(
            words
                .iter()
                .all(|w| w.end_ms - w.start_ms >= MIN_WORD_MS - 1)
        );
        assert_eq!(words.last().unwrap().end_ms, MIN_WORD_MS * 10);
    }

    #[test]
    fn handles_a_single_word_line() {
        let words = synthesize_word_timings("alone", 500, 1500);
        assert_eq!(words.len(), 1);
        assert_eq!((words[0].start_ms, words[0].end_ms), (500, 1500));
        assert!(words[0].synthesized);
    }

    #[test]
    fn returns_nothing_for_an_empty_string() {
        assert!(synthesize_word_timings("   ", 0, 1000).is_empty());
    }

    #[test]
    fn sorts_lines_and_derives_ends_from_the_following_line() {
        let lines = normalize_lines(
            &[
                raw(2000, "second line"),
                raw(1000, "first line"),
                raw(4000, "third line"),
            ],
            60_000,
        );
        let texts: Vec<&str> = lines.iter().map(|l| l.text.as_str()).collect();
        assert_eq!(texts, ["first line", "second line", "third line"]);
        assert_eq!(lines[0].end_ms, 2000);
        assert_eq!(lines[1].end_ms, 4000);
    }

    #[test]
    fn clamps_a_line_before_a_long_instrumental_break() {
        let lines = normalize_lines(&[raw(0, "lonely"), raw(40_000, "later")], 60_000);
        assert_eq!(lines[0].end_ms, MAX_LINE_MS);
    }

    #[test]
    fn ends_the_last_line_no_later_than_the_track() {
        let lines = normalize_lines(&[raw(1000, "the end")], 3000);
        assert_eq!(lines.last().unwrap().end_ms, 3000);
    }

    #[test]
    fn uses_sourced_word_starts_when_present() {
        let lines = normalize_lines(
            &[sourced(1000, "two words", &[1000, 1600]), raw(3000, "next")],
            60_000,
        );
        assert!(lines[0].words.iter().all(|w| !w.synthesized));
        assert_eq!(lines[0].words[1].start_ms, 1600);
    }

    #[test]
    fn drops_blank_lines() {
        assert!(normalize_lines(&[raw(0, "   ")], 1000).is_empty());
    }

    #[test]
    fn satisfies_every_invariant_the_renderer_relies_on() {
        let fixtures = [
            vec![raw(2000, "second line"), raw(1000, "first line")],
            vec![raw(0, "single")],
            vec![raw(0, "a b c d e f g h i j"), raw(50, "crowded")],
            vec![sourced(0, "with words", &[0, 10]), raw(20, "after")],
        ];
        for fixture in &fixtures {
            assert_eq!(check_invariants(&normalize_lines(fixture, 120_000)), Ok(()));
        }
    }

    #[test]
    fn reports_a_line_end_that_does_not_match_its_last_word() {
        let mut lines = normalize_lines(&[raw(0, "a line"), raw(2000, "another")], 60_000);
        lines[0].end_ms += 500;
        assert!(
            check_invariants(&lines)
                .unwrap_err()
                .contains("end != last word end")
        );
    }
}
