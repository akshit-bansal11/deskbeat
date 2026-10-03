//! Sonic Veil: desktop widgets for Spotify.
//!
//! `sonic-veil` runs the app. `sonic-veil --snapshot <dir>` renders the
//! widgets to PNG files from made-up data. `sonic-veil --probe` prints what
//! Windows reports about Spotify for a few seconds, for diagnosing a machine
//! where a widget stays empty; redirect its output to a file to read it.

#![cfg_attr(not(test), windows_subsystem = "windows")]

mod app;
mod capture;
mod gfx;
mod lyrics;
mod media;
mod settings;
mod snapshot;
mod tray;
mod widgets;
mod window;

use std::ffi::c_void;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use sonic_veil_core::config::AudioSource;
use windows::Win32::Foundation::HWND;
use windows::core::Result;

const APP_DIR: &str = "sonic-veil";
const LOG_LIMIT_BYTES: u64 = 512 * 1024;

/// Milliseconds since the app started, on a clock that never goes backwards.
/// Every timestamp shared between threads is on this clock.
pub fn now_ms() -> f64 {
    static START: OnceLock<Instant> = OnceLock::new();
    START.get_or_init(Instant::now).elapsed().as_secs_f64() * 1000.0
}

fn env_dir(variable: &str) -> PathBuf {
    std::env::var_os(variable)
        .map_or_else(std::env::temp_dir, PathBuf::from)
        .join(APP_DIR)
}

/// Where the config file lives: it roams with the user.
pub fn config_dir() -> PathBuf {
    env_dir("APPDATA")
}

/// Where the log and the lyrics cache live: machine-local, safe to delete.
pub fn data_dir() -> PathBuf {
    env_dir("LOCALAPPDATA")
}

/// Appends a line to the log file. There is no console and no telemetry, so
/// this file is the only place a failure can be seen.
pub fn log(message: &str) {
    let dir = data_dir();
    let path = dir.join("sonic-veil.log");
    let _ = std::fs::create_dir_all(&dir);
    if std::fs::metadata(&path).is_ok_and(|meta| meta.len() > LOG_LIMIT_BYTES) {
        let _ = std::fs::remove_file(&path);
    }
    let (t, _) = app::local_time();
    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
    {
        let _ = writeln!(
            file,
            "{}-{:02}-{:02} {:02}:{:02}:{:02} {message}",
            t.year, t.month, t.day, t.hour, t.minute, t.second
        );
    }
}

/// Lets a worker thread wake the UI thread by posting to its main window.
/// A window handle is just a number, which is what makes this safe to send.
#[derive(Clone, Copy)]
pub struct Notify(isize);

impl Notify {
    pub fn new(hwnd: HWND) -> Self {
        Self(hwnd.0 as isize)
    }

    /// For runs with no window to notify.
    pub fn none() -> Self {
        Self(0)
    }

    pub fn post(self, msg: u32) {
        if self.0 != 0 {
            window::post(HWND(self.0 as *mut c_void), msg);
        }
    }
}

/// Prints what the media session and the audio capture report, twice a second.
fn probe() -> Result<()> {
    let shared = Arc::new(Mutex::new(media::MediaState::default()));
    let _controls = media::spawn(shared.clone(), Notify::none());
    let audio = capture::spawn(Notify::none())?;
    audio.set_source(Some(AudioSource::Spotify));
    println!("spotify root process: {:?}", capture::spotify_pid());

    let mut samples = vec![0.0f32; 2048];
    for _ in 0..16 {
        std::thread::sleep(Duration::from_millis(500));
        audio.kick();
        let state = shared.lock().map(|s| s.clone()).unwrap_or_default();
        let rate = audio.latest(&mut samples);
        let peak = samples.iter().fold(0.0f32, |p, s| p.max(s.abs()));
        println!(
            "present={} playing={} title={:?} artist={:?} album={:?} duration={:.0}ms \
             position={:.0}ms (reported {:.0}ms, {:.0}ms ago) art={:?} accent={:?} | \
             audio rate={rate} peak={peak:.4}",
            state.present,
            state.playing,
            state.title,
            state.artist,
            state.album,
            state.duration_ms,
            state.position_now(now_ms()),
            state.position_ms,
            now_ms() - state.position_at_ms,
            state.art.as_ref().map(|art| (art.w, art.h)),
            state.art.as_ref().and_then(|art| art.accent),
        );
    }
    Ok(())
}

fn main() {
    std::panic::set_hook(Box::new(|info| log(&format!("panic: {info}"))));
    now_ms();

    let args: Vec<String> = std::env::args().skip(1).collect();
    let result = match args.first().map(String::as_str) {
        Some("--snapshot") => {
            snapshot::run(Path::new(args.get(1).map_or("snapshots", String::as_str)))
        }
        Some("--probe") => probe(),
        _ => app::run(),
    };
    if let Err(error) = result {
        log(&format!("fatal: {error}"));
        eprintln!("{error}");
        std::process::exit(1);
    }
}
