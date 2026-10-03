//! LRCLIB request building and response selection. The HTTP call itself lives
//! in the app; everything here is pure.

use serde::{Deserialize, Serialize};

const BASE: &str = "https://lrclib.net/api";

/// The fields of an LRCLIB record this app uses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(default, rename_all = "camelCase")]
pub struct Record {
    /// Seconds.
    pub duration: Option<f64>,
    pub instrumental: bool,
    pub plain_lyrics: Option<String>,
    pub synced_lyrics: Option<String>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Query {
    pub title: String,
    pub artist: String,
    pub album: String,
    pub duration_ms: i64,
}

/// Percent-encodes everything except RFC 3986 unreserved characters.
fn encode(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for byte in s.bytes() {
        if byte.is_ascii_alphanumeric() || b"-._~".contains(&byte) {
            out.push(byte as char);
        } else {
            out += &format!("%{byte:02X}");
        }
    }
    out
}

/// Exact lookup. LRCLIB matches on title, artist, album and duration
/// together, so passing the duration is what keeps a remaster from resolving
/// to the original's timings: same words, several seconds out.
pub fn get_url(q: &Query) -> String {
    let mut url = format!(
        "{BASE}/get?track_name={}&artist_name={}",
        encode(&q.title),
        encode(&q.artist)
    );
    if !q.album.is_empty() {
        url += &format!("&album_name={}", encode(&q.album));
    }
    if q.duration_ms > 0 {
        url += &format!("&duration={}", (q.duration_ms as f64 / 1000.0).round());
    }
    url
}

/// Looser search, used only when the exact lookup misses.
pub fn search_url(q: &Query) -> String {
    format!(
        "{BASE}/search?track_name={}&artist_name={}",
        encode(&q.title),
        encode(&q.artist)
    )
}

const BRACKET_WORDS: [&str; 9] = [
    "remaster", "remix", "version", "edit", "live", "feat", "with", "mono", "stereo",
];
const SUFFIX_WORDS: [&str; 9] = [
    "remaster",
    "remastered",
    "remix",
    "version",
    "edit",
    "live",
    "mono",
    "stereo",
    "mix",
];

/// Drops `(Remastered 2011)`, `[Live]`, `(feat. X)`: any bracketed group
/// naming a version rather than being part of the title.
fn strip_version_brackets(title: &str) -> String {
    let mut out = String::with_capacity(title.len());
    let mut rest = title;
    while let Some(open) = rest.find(['(', '[']) {
        let Some(len) = rest[open..].find([')', ']']) else {
            break;
        };
        let close = open + len;
        let inner = rest[open + 1..close].to_lowercase();
        if BRACKET_WORDS.iter().any(|word| inner.contains(word)) {
            out += rest[..open].trim_end();
        } else {
            out += &rest[..=close];
        }
        rest = &rest[close + 1..];
    }
    out + rest
}

/// Cuts `- Radio Edit`, `- 2019 Remaster`, `- Live at Wembley`. The keyword
/// is often not adjacent to the dash, so the whole segment is searched.
fn strip_version_suffix(title: &str) -> &str {
    for (dash, _) in title.match_indices('-') {
        let segment = title[dash + 1..].split('-').next().unwrap_or_default();
        let is_version = segment
            .split(|c: char| !c.is_alphanumeric())
            .any(|word| SUFFIX_WORDS.iter().any(|k| word.eq_ignore_ascii_case(k)));
        if is_version {
            return title[..dash].trim_end();
        }
    }
    title
}

/// Strip the decorations Spotify puts in titles that lyric databases do not
/// carry. This materially improves the hit rate.
pub fn clean_title(title: &str) -> String {
    let without_brackets = strip_version_brackets(title);
    strip_version_suffix(&without_brackets)
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// First artist only: LRCLIB indexes the primary credit, not the full billing.
pub fn primary_artist(artist: &str) -> &str {
    // ASCII lowercasing keeps byte offsets valid for slicing the original.
    let lower = artist.to_ascii_lowercase();
    let is_word_char = |c: Option<char>| c.is_some_and(char::is_alphanumeric);

    let mut cut = lower.find([',', '&']).unwrap_or(artist.len());
    for word in ["feat", "with"] {
        for (at, _) in lower.match_indices(word) {
            let before = lower[..at].chars().next_back();
            let after = lower[at + word.len()..].chars().next();
            if !is_word_char(before) && !is_word_char(after) {
                cut = cut.min(at);
            }
        }
    }

    match artist[..cut].trim() {
        "" => artist.trim(),
        head => head,
    }
}

/// The queries to try against `/get`, most specific first: the title as
/// Spotify reports it, then the cleaned title with the primary artist.
pub fn attempts(q: &Query) -> Vec<Query> {
    let cleaned = Query {
        title: clean_title(&q.title),
        artist: primary_artist(&q.artist).to_owned(),
        ..q.clone()
    };
    if cleaned == *q {
        vec![cleaned]
    } else {
        vec![q.clone(), cleaned]
    }
}

/// Chooses from search results: synced lyrics beat plain ones, then the
/// closest duration wins. A five-minute live cut is not the three-minute single.
pub fn pick_best(records: Vec<Record>, wanted_ms: i64) -> Option<Record> {
    let wanted = wanted_ms as f64 / 1000.0;
    records.into_iter().min_by(|a, b| {
        let key = |r: &Record| {
            (
                r.synced_lyrics.is_none(),
                (r.duration.unwrap_or(0.0) - wanted).abs(),
            )
        };
        let (a, b) = (key(a), key(b));
        a.0.cmp(&b.0).then(a.1.total_cmp(&b.1))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cleans_version_decorations_from_titles() {
        for (input, expected) in [
            ("Song (Remastered 2011)", "Song"),
            ("Song - Radio Edit", "Song"),
            ("Song (feat. Someone)", "Song"),
            ("Song (Live)", "Song"),
            ("Song [Mono]", "Song"),
            ("Song - 2019 Remaster", "Song"),
            ("Song - Live at Wembley", "Song"),
            ("Song", "Song"),
        ] {
            assert_eq!(clean_title(input), expected, "{input}");
        }
    }

    #[test]
    fn leaves_real_parts_of_a_title_alone() {
        for title in [
            "Sign o the Times (Prince)",
            "Jack-in-the-Box",
            "Song - Part Two",
            "Unclosed (bracket",
        ] {
            assert_eq!(clean_title(title), title);
        }
    }

    #[test]
    fn reduces_a_billing_to_its_primary_artist() {
        for (input, expected) in [
            ("A, B", "A"),
            ("A & B", "A"),
            ("A feat. B", "A"),
            ("A with B", "A"),
            ("Solo", "Solo"),
            ("Withered Hand", "Withered Hand"),
            ("Beyoncé, JAY-Z", "Beyoncé"),
        ] {
            assert_eq!(primary_artist(input), expected, "{input}");
        }
    }

    fn query(title: &str, artist: &str) -> Query {
        Query {
            title: title.to_owned(),
            artist: artist.to_owned(),
            album: "An Album".to_owned(),
            duration_ms: 215_400,
        }
    }

    #[test]
    fn builds_encoded_urls() {
        let q = query("Señorita & Co?", "A/B");
        assert_eq!(
            get_url(&q),
            "https://lrclib.net/api/get?track_name=Se%C3%B1orita%20%26%20Co%3F&artist_name=A%2FB&album_name=An%20Album&duration=215"
        );
        assert_eq!(
            search_url(&q),
            "https://lrclib.net/api/search?track_name=Se%C3%B1orita%20%26%20Co%3F&artist_name=A%2FB"
        );
    }

    #[test]
    fn omits_an_unknown_album_and_duration() {
        let q = Query {
            album: String::new(),
            duration_ms: 0,
            ..query("T", "A")
        };
        assert_eq!(
            get_url(&q),
            "https://lrclib.net/api/get?track_name=T&artist_name=A"
        );
    }

    #[test]
    fn tries_the_raw_title_before_the_cleaned_one() {
        let tries = attempts(&query("Song - 2019 Remaster", "A, B"));
        assert_eq!(tries.len(), 2);
        assert_eq!(tries[0].title, "Song - 2019 Remaster");
        assert_eq!(
            (tries[1].title.as_str(), tries[1].artist.as_str()),
            ("Song", "A")
        );
        assert_eq!(attempts(&query("Song", "A")).len(), 1);
    }

    #[test]
    fn prefers_synced_lyrics_then_the_closest_duration() {
        let record = |duration: f64, synced: bool| Record {
            duration: Some(duration),
            synced_lyrics: synced.then(|| "[00:01.00] x".to_owned()),
            plain_lyrics: Some("x".to_owned()),
            instrumental: false,
        };
        let best = pick_best(
            vec![
                record(215.0, false),
                record(300.0, true),
                record(218.0, true),
            ],
            215_400,
        );
        assert_eq!(best.unwrap().duration, Some(218.0));
        assert_eq!(pick_best(Vec::new(), 1000), None);
    }

    #[test]
    fn parses_a_record_and_ignores_unknown_fields() {
        let record: Record = serde_json::from_str(
            r#"{"id":1,"trackName":"T","duration":215.0,"instrumental":false,
                "plainLyrics":null,"syncedLyrics":"[00:01.00] hi"}"#,
        )
        .unwrap();
        assert_eq!(record.synced_lyrics.as_deref(), Some("[00:01.00] hi"));
        assert_eq!(record.plain_lyrics, None);
    }
}
