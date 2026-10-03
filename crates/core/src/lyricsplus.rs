//! Word-timed lyrics from a LyricsPlus server: request building and response
//! parsing. The HTTP call itself lives in the app; everything here is pure.
//!
//! LRCLIB knows when each line starts and nothing more, so word timing built
//! from it is a guess. These servers carry the time every syllable is sung.

use serde::Deserialize;

use crate::lrclib::{Query, encode};
use crate::timing::{Line, Word, join_words};

/// Community-run mirrors of the same service, tried in order. They come and
/// go, which is why the list is a setting and not only this constant.
pub const SERVERS: [&str; 2] = [
    "https://lyricsplus.binimum.org",
    "https://lyricsplus.prjktla.workers.dev",
];

/// Lyrics that run this far past the end of the track belong to another
/// recording of the song.
const OVERRUN_MS: i64 = 5000;

pub fn url(server: &str, q: &Query) -> String {
    let mut url = format!(
        "{}/v2/lyrics/get?title={}&artist={}",
        server.trim_end_matches('/'),
        encode(&q.title),
        encode(&q.artist)
    );
    if !q.album.is_empty() {
        url += &format!("&album={}", encode(&q.album));
    }
    if q.duration_ms > 0 {
        url += &format!("&duration={}", (q.duration_ms as f64 / 1000.0).round());
    }
    url
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Response {
    /// `"Word"` when the syllables are timed, `"Line"` when only lines are.
    #[serde(rename = "type")]
    timing: String,
    lyrics: Vec<Entry>,
}

#[derive(Deserialize, Default)]
#[serde(default)]
struct Entry {
    syllabus: Vec<Syllable>,
}

/// Times are milliseconds. The text ends in a space when the syllable ends
/// its word.
#[derive(Deserialize, Default)]
#[serde(default)]
struct Syllable {
    time: f64,
    duration: f64,
    text: String,
    #[serde(rename = "isBackground")]
    background: bool,
}

/// The lines of a response, or `None` when it has no word timing or does not
/// fit a track of `duration_ms`. Line-timed answers are refused: LRCLIB
/// already covers those, with better matching.
pub fn parse(body: &str, duration_ms: i64) -> Option<Vec<Line>> {
    let response: Response = serde_json::from_str(body).ok()?;
    if response.timing != "Word" {
        return None;
    }

    let mut lines: Vec<Line> = response.lyrics.iter().filter_map(line).collect();
    lines.sort_by_key(|l| l.start_ms);
    let last_start = lines.last()?.start_ms;
    (duration_ms <= 0 || last_start < duration_ms + OVERRUN_MS).then_some(lines)
}

fn line(entry: &Entry) -> Option<Line> {
    let mut words: Vec<Word> = Vec::new();
    for syllable in &entry.syllabus {
        let text = syllable.text.trim();
        if text.is_empty() {
            continue;
        }
        // A background vocal is a word of its own, whatever the spacing says.
        if syllable.background
            && let Some(previous) = words.last_mut()
        {
            previous.joined = false;
        }
        let start_ms = syllable.time as i64;
        words.push(Word {
            text: text.to_owned(),
            start_ms,
            end_ms: start_ms + (syllable.duration as i64).max(1),
            synthesized: false,
            joined: !syllable.background && !syllable.text.ends_with(char::is_whitespace),
        });
    }
    words.last_mut()?.joined = false;

    // Background vocals overlap the lead, so the first and last words are not
    // necessarily the earliest and latest.
    Some(Line {
        start_ms: words.iter().map(|w| w.start_ms).min()?,
        end_ms: words.iter().map(|w| w.end_ms).max()?,
        text: join_words(&words),
        words,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    const BODY: &str = r#"{"type":"Word","metadata":{"source":"x"},"lyrics":[
        {"time":5000,"duration":900,"text":"second line","syllabus":[
            {"time":5000,"duration":400,"text":"second "},
            {"time":5400,"duration":500,"text":"line"}]},
        {"time":1000,"duration":2000,"text":"(Yes)my bloody nose","syllabus":[
            {"time":1000,"duration":300,"text":"my "},
            {"time":1300,"duration":200,"text":"bloo"},
            {"time":1500,"duration":250,"text":"dy "},
            {"time":1200,"duration":1800,"text":"(Yes)","isBackground":true},
            {"time":1750,"duration":400,"text":"nose"}]},
        {"time":9000,"duration":10,"text":"","syllabus":[]}]}"#;

    #[test]
    fn syllables_keep_their_own_times_and_join_into_words() {
        let lines = parse(BODY, 60_000).unwrap();
        assert_eq!(lines.len(), 2, "the empty line is dropped");
        let first = &lines[0];
        assert_eq!(first.text, "my bloody (Yes) nose");
        assert_eq!((first.start_ms, first.end_ms), (1000, 3000));
        let spans: Vec<(i64, i64)> = first.words.iter().map(|w| (w.start_ms, w.end_ms)).collect();
        assert_eq!(
            spans,
            [
                (1000, 1300),
                (1300, 1500),
                (1500, 1750),
                (1200, 3000),
                (1750, 2150)
            ]
        );
        assert!(first.words.iter().all(|w| !w.synthesized));
        assert_eq!(lines[1].text, "second line");
    }

    #[test]
    fn refuses_what_is_not_word_timed_or_not_this_track() {
        assert!(parse(&BODY.replace("\"Word\"", "\"Line\""), 60_000).is_none());
        assert!(parse("{\"error\":{\"status\":404}}", 60_000).is_none());
        assert!(parse("not json", 60_000).is_none());
        // An unknown duration is not held against the lyrics. A last line
        // starting 35s after the track ends is.
        assert!(parse(BODY, 0).is_some());
        assert!(parse(&BODY.replace("\"time\":5", "\"time\":95"), 60_000).is_none());
    }

    #[test]
    fn the_url_carries_what_the_server_matches_on() {
        let q = Query {
            title: "bad guy".to_owned(),
            artist: "Billie Eilish".to_owned(),
            album: String::new(),
            duration_ms: 194_088,
        };
        assert_eq!(
            url("https://example.org/", &q),
            "https://example.org/v2/lyrics/get?title=bad%20guy&artist=Billie%20Eilish&duration=194"
        );
    }
}
