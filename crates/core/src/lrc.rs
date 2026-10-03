//! LRC and enhanced LRC (ELRC) parsing.

/// A line the parser produced but has not yet been given an end time or words.
#[derive(Debug, Clone, PartialEq)]
pub struct RawLine {
    pub start_ms: i64,
    pub text: String,
    /// Enhanced-LRC word starts, aligned 1:1 with the whitespace tokens of `text`.
    pub word_starts_ms: Option<Vec<i64>>,
}

#[derive(Debug, Default)]
pub struct ParsedLrc {
    pub lines: Vec<RawLine>,
    /// True when at least one line carried inline word markers.
    pub has_word_timing: bool,
}

/// A fractional part is centiseconds at two digits and milliseconds at three:
/// reading `[00:27.93]` as 93ms rather than 930ms puts every line a second early.
fn fraction_to_ms(frac: &str) -> i64 {
    let n: i64 = frac.parse().unwrap_or(0);
    match frac.len() {
        0 => 0,
        1 => n * 100,
        2 => n * 10,
        _ => n,
    }
}

/// Reads `<open>mm:ss[.xx]<close>` at the start of `s`. Returns the time in
/// milliseconds and the number of bytes consumed.
fn read_stamp(s: &str, open: u8, close: u8) -> Option<(i64, usize)> {
    let b = s.as_bytes();
    if b.first() != Some(&open) {
        return None;
    }
    let digits = |from: usize, max: usize| {
        b[from..]
            .iter()
            .take(max)
            .take_while(|c| c.is_ascii_digit())
            .count()
    };

    let mut i = 1;
    let minute_len = digits(i, 3);
    if minute_len == 0 {
        return None;
    }
    let minutes: i64 = s[i..i + minute_len].parse().ok()?;
    i += minute_len;

    if b.get(i) != Some(&b':') || digits(i + 1, 2) != 2 {
        return None;
    }
    let seconds: i64 = s[i + 1..i + 3].parse().ok()?;
    i += 3;

    let mut frac = "";
    if matches!(b.get(i), Some(b'.' | b':')) {
        let frac_len = digits(i + 1, 3);
        if frac_len == 0 {
            return None;
        }
        frac = &s[i + 1..i + 1 + frac_len];
        i += 1 + frac_len;
    }

    if b.get(i) != Some(&close) {
        return None;
    }
    Some((
        minutes * 60_000 + seconds * 1000 + fraction_to_ms(frac),
        i + 1,
    ))
}

/// `[ti:...]`, `[ar:...]` and friends. `[offset:]` is the only one that changes timing.
fn metadata(line: &str) -> Option<(&str, &str)> {
    let inner = line.strip_prefix('[')?.strip_suffix(']')?;
    let (key, value) = inner.split_once(':')?;
    (!key.is_empty() && key.bytes().all(|c| c.is_ascii_alphabetic())).then_some((key, value))
}

/// A bracketed annotation with no timestamp: `[Chorus]`, `[Verse 2]`, `[Instrumental]`.
fn is_section(s: &str) -> bool {
    s.len() >= 2 && s.starts_with('[') && s.ends_with(']') && !s[1..s.len() - 1].contains(']')
}

/// Removes inline `<mm:ss.xx>` word markers, returning the clean text and the
/// marker times in order.
fn strip_word_tags(rest: &str) -> (String, Vec<i64>) {
    let mut text = String::with_capacity(rest.len());
    let mut starts = Vec::new();
    let mut i = 0;
    while i < rest.len() {
        if let Some((ms, len)) = read_stamp(&rest[i..], b'<', b'>') {
            starts.push(ms);
            text.push(' ');
            i += len;
        } else {
            let Some(ch) = rest[i..].chars().next() else {
                break;
            };
            text.push(ch);
            i += ch.len_utf8();
        }
    }
    (
        text.split_whitespace().collect::<Vec<_>>().join(" "),
        starts,
    )
}

/// Parse standard LRC and enhanced LRC into raw lines.
///
/// Malformed lines are skipped rather than failed on: lyric files in the wild
/// are routinely half-broken, and one bad line should not cost the whole track.
///
/// `[offset:]` is applied as `timestamp + offset`. The LRC convention is
/// genuinely ambiguous about the sign, and providers disagree; the manual
/// offset setting is the escape hatch for a file that reads the other way.
pub fn parse_lrc(content: &str) -> ParsedLrc {
    let mut out = ParsedLrc::default();
    let mut offset_ms = 0i64;

    for raw in content.lines() {
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            continue;
        }

        if let Some((key, value)) = metadata(trimmed) {
            if key.eq_ignore_ascii_case("offset")
                && let Ok(parsed) = value.trim().parse::<i64>()
            {
                offset_ms = parsed;
            }
            continue;
        }

        // Consume every leading timestamp; `[00:12.00][01:44.00] refrain` is legal LRC.
        let mut starts = Vec::new();
        let mut rest = trimmed;
        while let Some((ms, len)) = read_stamp(rest, b'[', b']') {
            starts.push(ms);
            rest = &rest[len..];
        }
        if starts.is_empty() {
            continue;
        }

        let rest = rest.trim();
        if rest.is_empty() || is_section(rest) {
            continue;
        }

        let (text, word_starts) = strip_word_tags(rest);
        if text.is_empty() {
            continue;
        }

        // Trust word markers only when they line up 1:1 with the words they bound.
        let usable = word_starts.len() == text.split(' ').count();
        out.has_word_timing |= usable;

        for start in starts {
            out.lines.push(RawLine {
                start_ms: start + offset_ms,
                text: text.clone(),
                word_starts_ms: usable.then(|| word_starts.iter().map(|w| w + offset_ms).collect()),
            });
        }
    }

    out
}

/// Split a plain-text lyric body into lines, for the unsynced fallback.
pub fn parse_plain(content: &str) -> Vec<String> {
    content
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !is_section(line))
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn starts(content: &str) -> Vec<i64> {
        parse_lrc(content)
            .lines
            .iter()
            .map(|l| l.start_ms)
            .collect()
    }

    #[test]
    fn reads_a_two_digit_fraction_as_centiseconds() {
        assert_eq!(starts("[00:27.93] first line"), [27_930]);
    }

    #[test]
    fn reads_a_three_digit_fraction_as_milliseconds() {
        assert_eq!(starts("[00:27.930] first line"), [27_930]);
    }

    #[test]
    fn accepts_a_colon_as_the_fraction_separator() {
        assert_eq!(starts("[01:05:50] a line"), [65_500]);
    }

    #[test]
    fn parses_enhanced_lrc_word_markers() {
        let parsed = parse_lrc("[00:27.93] <00:27.93> first <00:28.21> line <00:28.60> here");
        assert!(parsed.has_word_timing);
        assert_eq!(parsed.lines[0].text, "first line here");
        assert_eq!(
            parsed.lines[0].word_starts_ms,
            Some(vec![27_930, 28_210, 28_600])
        );
    }

    #[test]
    fn ignores_word_markers_that_do_not_align_with_the_words() {
        let parsed = parse_lrc("[00:10.00] <00:10.00> two words here");
        assert!(!parsed.has_word_timing);
        assert_eq!(parsed.lines[0].word_starts_ms, None);
        assert_eq!(parsed.lines[0].text, "two words here");
    }

    #[test]
    fn applies_an_offset_tag_to_every_timestamp() {
        assert_eq!(
            starts("[offset:+500]\n[00:10.00] a\n[00:20.00] b"),
            [10_500, 20_500]
        );
    }

    #[test]
    fn strips_metadata_tags_and_section_markers() {
        let parsed = parse_lrc(
            "[ar:Someone]\n[ti:A Song]\n[by:Someone Else]\n[00:01.00] [Chorus]\n[00:02.00] real words",
        );
        assert_eq!(parsed.lines.len(), 1);
        assert_eq!(parsed.lines[0].text, "real words");
    }

    #[test]
    fn expands_a_line_carrying_several_timestamps() {
        let parsed = parse_lrc("[00:10.00][01:10.00] refrain");
        assert_eq!(starts("[00:10.00][01:10.00] refrain"), [10_000, 70_000]);
        assert!(parsed.lines.iter().all(|l| l.text == "refrain"));
    }

    #[test]
    fn skips_malformed_lines() {
        assert_eq!(
            parse_lrc("not a lyric line\n[bad] also not\n[00:03.00] good")
                .lines
                .len(),
            1
        );
        assert_eq!(parse_lrc("[00:3.00] short seconds").lines.len(), 0);
    }

    #[test]
    fn drops_empty_lyric_bodies() {
        assert!(parse_lrc("[00:05.00]   ").lines.is_empty());
    }

    #[test]
    fn handles_crlf_and_non_ascii_text() {
        let parsed = parse_lrc("[00:01.00] café ☕\r\n[00:02.00] 日本語");
        assert_eq!(parsed.lines[0].text, "café ☕");
        assert_eq!(parsed.lines[1].text, "日本語");
    }

    #[test]
    fn plain_keeps_non_empty_lines_and_drops_section_markers() {
        assert_eq!(
            parse_plain("first\n\n[Chorus]\nsecond  "),
            ["first", "second"]
        );
    }
}
