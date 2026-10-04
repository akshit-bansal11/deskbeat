//! Fetches lyrics on a short-lived thread, with a disk cache so a track is
//! only ever downloaded once. LRCLIB answers first, with line timing; a
//! LyricsPlus server then replaces that with word timing when it has any.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use deskbeat_core::lrc::{parse_lrc, parse_plain};
use deskbeat_core::lrclib::{self, Query, Record};
use deskbeat_core::lyricsplus;
use deskbeat_core::timing::{Line, normalize_lines};
use windows::Foundation::Uri;
use windows::Web::Http::HttpClient;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::core::HSTRING;

use crate::window::WM_APP_LYRICS;
use crate::{Notify, data_dir, log};

const USER_AGENT: &str = concat!(
    "Deskbeat/",
    env!("CARGO_PKG_VERSION"),
    " (https://github.com/akshit-bansal11/deskbeat)"
);
/// A "no lyrics" answer is asked again after this long: LRCLIB gains tracks.
const MISS_TTL: Duration = Duration::from_secs(7 * 24 * 60 * 60);
/// A LyricsPlus server answers two requests in ten seconds and refuses the
/// third, so requests to one server are kept this far apart.
const WORD_REQUEST_GAP: Duration = Duration::from_millis(5200);
const TOO_MANY_REQUESTS: i32 = 429;

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
    let path = cache_path(query, "json");
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

/// Looks a track up and publishes what it finds. `word_servers` is `None`
/// when word timing is not wanted.
pub fn fetch(
    query: Query,
    track_gen: u64,
    shared: Arc<Mutex<LyricsState>>,
    notify: Notify,
    word_servers: Option<Vec<String>>,
) {
    std::thread::spawn(move || {
        let publish = |lyrics: Lyrics| {
            if let Ok(mut state) = shared.lock()
                && state.track_gen == track_gen
            {
                state.lyrics = lyrics;
            }
            notify.post(WM_APP_LYRICS);
        };
        let Some(servers) = word_servers else {
            return publish(lookup(&query));
        };
        let path = cache_path(&query, "words.json");
        let known = cached_words(&path, query.duration_ms);
        if let Some(Some(lines)) = known {
            return publish(Lyrics::Synced(lines));
        }
        // The servers can be slow, so the line-timed answer goes up first.
        publish(lookup(&query));
        // A track that was skipped past is not worth one of the few requests.
        let wanted = || {
            shared
                .lock()
                .is_ok_and(|state| state.track_gen == track_gen)
        };
        if known.is_none()
            && let Some(lines) = download_words(&query, &servers, &path, &wanted)
        {
            publish(Lyrics::Synced(lines));
        }
    });
}

/// Word-timed lyrics for a track, from the cache or the servers.
pub fn words(query: &Query, servers: &[String]) -> Option<Vec<Line>> {
    let path = cache_path(query, "words.json");
    match cached_words(&path, query.duration_ms) {
        Some(known) => known,
        None => download_words(query, servers, &path, &|| true),
    }
}

/// A request to a LyricsPlus server, held back until that server's last
/// one is `WORD_REQUEST_GAP` old. Every fetch thread waits its turn here.
fn paced_get(client: &HttpClient, server: &str, url: &str) -> Option<(i32, String)> {
    static LAST: Mutex<Option<HashMap<String, Instant>>> = Mutex::new(None);
    let mut last = LAST.lock().ok()?;
    let last = last.get_or_insert_with(HashMap::new);
    if let Some(wait) = last
        .get(server)
        .and_then(|at| WORD_REQUEST_GAP.checked_sub(at.elapsed()))
    {
        std::thread::sleep(wait);
    }
    let answer = get(client, url);
    last.insert(server.to_owned(), Instant::now());
    answer
}

/// `Some(None)` is a remembered miss, stored as an empty file.
fn cached_words(path: &Path, duration_ms: i64) -> Option<Option<Vec<Line>>> {
    let body = std::fs::read_to_string(path).ok()?;
    if body.is_empty() {
        let age = std::fs::metadata(path).ok()?.modified().ok()?.elapsed();
        return age.is_ok_and(|age| age <= MISS_TTL).then_some(None);
    }
    // A body that no longer parses is asked for again.
    lyricsplus::parse(&body, duration_ms).map(Some)
}

/// Asks each server in turn, the cleaned title first: it is the one the
/// servers usually know. A miss is only remembered when a server said so;
/// servers that are down or rate-limited say nothing about the track. A
/// server that refuses for asking too often is asked once more.
fn download_words(
    query: &Query,
    servers: &[String],
    path: &Path,
    wanted: &dyn Fn() -> bool,
) -> Option<Vec<Line>> {
    let client = client()?;
    let mut missing = false;
    for attempt in lrclib::attempts(query).iter().rev() {
        'servers: for server in servers {
            let url = lyricsplus::url(server, attempt);
            for _ in 0..2 {
                if !wanted() {
                    return None;
                }
                match paced_get(&client, server, &url) {
                    Some((200, body)) => {
                        if let Some(lines) = lyricsplus::parse(&body, query.duration_ms) {
                            write_text(path, &body);
                            return Some(lines);
                        }
                        // Line-timed only, or another recording of the song.
                        missing = true;
                        break 'servers;
                    }
                    Some((404, _)) => {
                        missing = true;
                        break 'servers;
                    }
                    Some((TOO_MANY_REQUESTS, _)) => {}
                    _ => break,
                }
            }
        }
    }
    if missing {
        write_text(path, "");
    }
    None
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

fn cache_path(q: &Query, extension: &str) -> PathBuf {
    let key = format!(
        "{}\n{}\n{}\n{}",
        q.title,
        q.artist,
        q.album,
        q.duration_ms / 1000
    );
    data_dir()
        .join("lyrics")
        .join(format!("{:016x}.{extension}", fnv1a(&key)))
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
    write_text(path, &serde_json::to_string(record).unwrap_or_default());
}

fn write_text(path: &Path, text: &str) {
    let written = path
        .parent()
        .map_or(Ok(()), std::fs::create_dir_all)
        .and_then(|()| std::fs::write(path, text));
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

fn client() -> Option<HttpClient> {
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
    let client = HttpClient::new().ok()?;
    client
        .DefaultRequestHeaders()
        .ok()?
        .UserAgent()
        .ok()?
        .TryParseAdd(&HSTRING::from(USER_AGENT))
        .ok()?;
    Some(client)
}

/// `Some(None)` means LRCLIB answered and has nothing; `None` means it could
/// not be reached, which must not be cached as a miss.
fn download(query: &Query) -> Option<Option<Record>> {
    let client = client()?;

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
