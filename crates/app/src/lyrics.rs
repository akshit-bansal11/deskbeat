//! Fetches lyrics from LRCLIB on a short-lived thread, with a disk cache so a
//! track is only ever downloaded once.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use sonic_veil_core::lrc::{parse_lrc, parse_plain};
use sonic_veil_core::lrclib::{self, Query, Record};
use sonic_veil_core::timing::{Line, normalize_lines};
use windows::Foundation::Uri;
use windows::Web::Http::HttpClient;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::HSTRING;

use crate::window::WM_APP_LYRICS;
use crate::{Notify, data_dir, log};

const USER_AGENT: &str = concat!(
    "SonicVeil/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/akshit-bansal11/sonic-veil)"
);
/// A "no lyrics" answer is asked again after this long: LRCLIB gains tracks.
const MISS_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);

#[derive(Default)]
pub enum Lyrics {
    /// Nothing is playing.
    #[default]
    None,
    Loading,
    Synced(Vec<Line>),
    /// Text without timestamps.
    Plain(Vec<String>),
    Instrumental,
    Missing,
}

#[derive(Default)]
pub struct LyricsState {
    /// The track these lyrics belong to. A fetch that finishes after the
    /// track changed must not overwrite the newer track's state.
    pub track_gen: u64,
    pub lyrics: Lyrics,
}

/// Finds lyrics for a track: the disk cache first, LRCLIB otherwise. Blocks
/// on the network, so it belongs on a worker thread.
pub fn lookup(query: &Query) -> Lyrics {
    let path = cache_path(query);
    let record = match read_cache(&path) {
        Some(known) => known,
        None => match download(query) {
            Some(answer) => {
                write_cache(&path, &answer);
                answer
            }
            // Offline or LRCLIB is down: say missing now, and ask again next time.
            None => None,
        },
    };
    to_lyrics(record, query.duration_ms)
}

pub fn fetch(query: Query, track_gen: u64, shared: Arc<Mutex<LyricsState>>, notify: Notify) {
    std::thread::spawn(move || {
        let lyrics = lookup(&query);
        if let Ok(mut state) = shared.lock()
            && state.track_gen == track_gen
        {
            state.lyrics = lyrics;
        }
        notify.post(WM_APP_LYRICS);
    });
}

fn to_lyrics(record: Option<Record>, duration_ms: i64) -> Lyrics {
    let Some(record) = record else {
        return Lyrics::Missing;
    };
    if let Some(synced) = &record.synced_lyrics {
        let lines = normalize_lines(&parse_lrc(synced).lines, duration_ms);
        if !lines.is_empty() {
            return Lyrics::Synced(lines);
        }
    }
    if record.instrumental {
        return Lyrics::Instrumental;
    }
    match record.plain_lyrics.as_deref().map(parse_plain) {
        Some(lines) if !lines.is_empty() => Lyrics::Plain(lines),
        _ => Lyrics::Missing,
    }
}

/// FNV-1a. The standard hasher is not guaranteed stable across Rust
/// releases, and these names are file names that must survive an update.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3)
    })
}

fn cache_path(q: &Query) -> PathBuf {
    let key = format!(
        "{}\n{}\n{}\n{}",
        q.title,
        q.artist,
        q.album,
        q.duration_ms / 1000
    );
    data_dir()
        .join("lyrics")
        .join(format!("{:016x}.json", fnv1a(&key)))
}

/// `Some(None)` is a remembered miss.
fn read_cache(path: &Path) -> Option<Option<Record>> {
    let text = std::fs::read_to_string(path).ok()?;
    let cached: Option<Record> = serde_json::from_str(&text).ok()?;
    if cached.is_none() {
        let age = std::fs::metadata(path).ok()?.modified().ok()?.elapsed();
        if age.is_ok_and(|age| age > MISS_TTL) {
            return None;
        }
    }
    Some(cached)
}

fn write_cache(path: &Path, record: &Option<Record>) {
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, serde_json::to_string(record).unwrap_or_default()));
    if let Err(error) = written {
        log(&format!("could not cache lyrics: {error}"));
    }
}

/// Status code and body, or `None` when the request never completed.
fn get(client: &HttpClient, url: &str) -> Option<(i32, String)> {
    let uri = Uri::CreateUri(&HSTRING::from(url)).ok()?;
    let response = client.GetAsync(&uri).ok()?.join().ok()?;
    let status = response.StatusCode().ok()?.0;
    let body = response
        .Content()
        .ok()?
        .ReadAsStringAsync()
        .ok()?
        .join()
        .ok()?;
    Some((status, body.to_string()))
}

/// `Some(None)` means LRCLIB answered and has nothing; `None` means it could
/// not be reached, which must not be cached as a miss.
fn download(query: &Query) -> Option<Option<Record>> {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let client = HttpClient::new().ok()?;
    client
        .DefaultRequestHeaders()
        .ok()?
        .UserAgent()
        .ok()?
        .TryParseAdd(&HSTRING::from(USER_AGENT))
        .ok()?;

    let attempts = lrclib::attempts(query);
    for attempt in &attempts {
        match get(&client, &lrclib::get_url(attempt))? {
            (200, body) => {
                if let Ok(record) = serde_json::from_str::<Record>(&body) {
                    return Some(Some(record));
                }
            }
            // 404 is LRCLIB's ordinary "no match", not a failure.
            (404, _) => {}
            _ => return None,
        }
    }

    match get(&client, &lrclib::search_url(attempts.last()?))? {
        (200, body) => {
            let records = serde_json::from_str::<Vec<Record>>(&body).ok()?;
            Some(lrclib::pick_best(records, query.duration_ms))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn record(synced: Option<&str>, plain: Option<&str>, instrumental: bool) -> Option<Record> {
        Some(Record {
            duration: Some(100.0),
            instrumental,
            plain_lyrics: plain.map(str::to_owned),
            synced_lyrics: synced.map(str::to_owned),
        })
    }

    #[test]
    fn prefers_synced_lyrics_then_plain_then_missing() {
        let synced = to_lyrics(record(Some("[00:01.00] hi"), Some("hi"), false), 100_000);
        assert!(matches!(synced, Lyrics::Synced(lines) if lines.len() == 1));

        let plain = to_lyrics(
            record(Some("no timestamps"), Some("hi\nthere"), false),
            100_000,
        );
        assert!(matches!(plain, Lyrics::Plain(lines) if lines.len() == 2));

        assert!(matches!(
            to_lyrics(record(None, None, true), 100_000),
            Lyrics::Instrumental
        ));
        assert!(matches!(
            to_lyrics(record(None, Some("  "), false), 100_000),
            Lyrics::Missing
        ));
        assert!(matches!(to_lyrics(None, 100_000), Lyrics::Missing));
    }

    #[test]
    fn the_cache_name_is_stable() {
        // FNV-1a test vector; these are file names and must never change.
        assert_eq!(fnv1a("a"), 0xaf63_dc4c_8601_ec8c);
        assert_eq!(fnv1a(""), 0xcbf2_9ce4_8422_2325);
    }
}
