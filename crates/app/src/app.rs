//! The app: owns every window and all state, and runs the one loop.
//!
//! The loop drains messages, ticks the widgets that are due, draws the ones
//! that changed, then sleeps for exactly as long as the most impatient widget
//! allows. With nothing playing that is forever: no timer, no frame, no CPU.

use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};

use sonic_veil_core::color::{Rgba, parse_hex, with_alpha};
use sonic_veil_core::config::{Align, Config, Layer, Preset};
use sonic_veil_core::lrclib::Query;
use sonic_veil_core::timefmt::LocalTime;
use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, GetLastError, HANDLE, HWND, WAIT_OBJECT_0};
use windows::Win32::Graphics::Dwm::DwmFlush;
use windows::Win32::Storage::FileSystem::{
    FILE_NOTIFY_CHANGE_FILE_NAME, FILE_NOTIFY_CHANGE_LAST_WRITE, FILE_NOTIFY_CHANGE_SIZE,
    FindFirstChangeNotificationW, FindNextChangeNotification,
};
use windows::Win32::System::SystemInformation::GetLocalTime;
use windows::Win32::System::Threading::{CreateMutexW, INFINITE};
use windows::Win32::UI::HiDpi::{
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, SetProcessDpiAwarenessContext,
};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, RegisterHotKey, UnregisterHotKey, VK_DOWN, VK_UP,
};
use windows::Win32::UI::WindowsAndMessaging::*;
use windows::core::{HSTRING, Result, w};

use crate::capture::{self, Audio};
use crate::gfx::{Gfx, Surface, TextStyle, rect};
use crate::lyrics::{self, Lyrics, LyricsState};
use crate::media::{self, Cmd, MediaState};
use crate::settings::Panel;
use crate::tray::{Item, Tray};
use crate::widgets::{Action, Ctx, Kind, Wake, Widget};
use crate::window::{self, Cover, Event, Mouse, Z};
use crate::{Notify, config_dir, log, now_ms};

const HOTKEY_EDIT: i32 = 1;
const HOTKEY_HIDE: i32 = 2;
const HOTKEY_SETTINGS: i32 = 3;
const HOTKEY_OFFSET_EARLIER: i32 = 4;
const HOTKEY_OFFSET_LATER: i32 = 5;
const HOTKEYS: [(i32, u32); 5] = [
    (HOTKEY_EDIT, b'E' as u32),
    (HOTKEY_HIDE, b'H' as u32),
    (HOTKEY_SETTINGS, b'S' as u32),
    (HOTKEY_OFFSET_EARLIER, VK_UP.0 as u32),
    (HOTKEY_OFFSET_LATER, VK_DOWN.0 as u32),
];

const CMD_SETTINGS: u32 = 100;
const CMD_EDIT: u32 = 101;
const CMD_OPEN_CONFIG: u32 = 102;
const CMD_AUTOSTART: u32 = 103;
const CMD_HIDE: u32 = 104;
const CMD_QUIT: u32 = 105;
const CMD_WIDGET: u32 = 200;
const CMD_PRESET: u32 = 300;
const CMD_LAYER: u32 = 400;
const LAYERS: [(Layer, &str); 3] = [
    (Layer::Desktop, "On the desktop"),
    (Layer::Normal, "Like a normal window"),
    (Layer::Top, "Always on top"),
];

/// Spotify green, used when the accent is automatic and there is no album art.
pub const FALLBACK_ACCENT: Rgba = [0.118, 0.843, 0.376, 1.0];
const WHITE: Rgba = [1.0; 4];
const DEFAULT_CARD: Rgba = [0.063, 0.063, 0.078, 1.0];

const OFFSET_STEP_MS: i32 = 100;
/// Editors save in several writes; wait for them to finish before reloading.
const RELOAD_DELAY_MS: f64 = 200.0;
const SAVE_DELAY_MS: f64 = 500.0;
/// Show Desktop reorders windows a moment after the foreground changes.
const DESKTOP_RECHECK_MS: f64 = 250.0;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const RUN_KEY: &str = r"HKCU\Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "SonicVeil";

/// Colours parsed from the config once, not on every frame.
pub struct Palette {
    pub text: Rgba,
    pub card: Rgba,
    /// `None` when the accent follows the album art.
    pub accent: Option<Rgba>,
}

impl Palette {
    pub fn new(cfg: &Config) -> Self {
        Self {
            text: parse_hex(&cfg.theme.text).unwrap_or(WHITE),
            card: parse_hex(&cfg.theme.card_color).unwrap_or(DEFAULT_CARD),
            accent: parse_hex(&cfg.theme.accent),
        }
    }

    pub fn accent(&self, media: &MediaState) -> Rgba {
        self.accent
            .or(media.art.as_ref().and_then(|art| art.accent))
            .unwrap_or(FALLBACK_ACCENT)
    }
}

pub fn local_time() -> (LocalTime, u32) {
    let t = unsafe { GetLocalTime() };
    let time = LocalTime {
        year: u32::from(t.wYear),
        month: u32::from(t.wMonth),
        day: u32::from(t.wDay),
        weekday: u32::from(t.wDayOfWeek),
        hour: u32::from(t.wHour),
        minute: u32::from(t.wMinute),
        second: u32::from(t.wSecond),
    };
    (time, 1000u32.saturating_sub(u32::from(t.wMilliseconds)))
}

/// When a widget should next be ticked.
#[derive(Clone, Copy, PartialEq)]
enum Due {
    Now,
    At(f64),
    Never,
}

struct Host {
    kind: Kind,
    hwnd: HWND,
    surface: Surface,
    widget: Box<dyn Widget>,
    /// Size in physical pixels.
    size: (i32, i32),
    visible: bool,
    /// Hidden behind a window that covers its monitor: not ticked, not drawn.
    paused: bool,
    dirty: bool,
    due: Due,
}

struct App {
    gfx: Gfx,
    main: HWND,
    notify: Notify,
    tray: Tray,
    hosts: Vec<Host>,

    cfg: Config,
    cfg_path: PathBuf,
    /// The config file's text as this app last read or wrote it, so its own
    /// saves are not mistaken for an edit by the user.
    cfg_text: String,
    /// The file on disk does not parse. It is moved aside, not overwritten.
    cfg_broken: bool,
    cfg_watch: Option<HANDLE>,
    reload_at: Option<f64>,
    /// Dragging a slider changes a setting many times a second; the file is
    /// written once the changes stop.
    save_at: Option<f64>,
    hotkeys_on: bool,
    palette: Palette,
    settings: Option<Panel>,

    media: MediaState,
    media_shared: Arc<Mutex<MediaState>>,
    media_tx: Sender<Cmd>,
    lyrics: Lyrics,
    lyrics_gen: u64,
    lyrics_shared: Arc<Mutex<LyricsState>>,
    audio: Arc<Audio>,

    /// Pixels per display-independent pixel on the primary monitor.
    scale: f32,
    edit: bool,
    hidden: bool,
    /// How much of its monitor the window in front covers, and which monitor.
    cover: Cover,
    cover_monitor: isize,
    /// Show Desktop is active and the widgets were lifted above the desktop.
    raised: bool,
    desktop_recheck_at: Option<f64>,
    device_lost: bool,
    quit: bool,
}

fn read_config(path: &Path) -> (Config, String, bool) {
    match std::fs::read_to_string(path) {
        Ok(text) => match Config::from_toml(&text) {
            Ok(cfg) => (cfg, text, false),
            Err(error) => {
                log(&format!(
                    "config.toml does not parse, using defaults: {error}"
                ));
                (Config::default(), text, true)
            }
        },
        Err(_) => (Config::default(), String::new(), false),
    }
}

fn reg(args: &[&str]) -> bool {
    Command::new("reg")
        .args(args)
        .creation_flags(CREATE_NO_WINDOW)
        .output()
        .is_ok_and(|output| output.status.success())
}

fn autostart_enabled() -> bool {
    reg(&["query", RUN_KEY, "/v", RUN_VALUE])
}

fn set_autostart(on: bool) {
    let done = match (on, std::env::current_exe()) {
        (true, Ok(exe)) => {
            let command = format!("\"{}\"", exe.display());
            reg(&[
                "add", RUN_KEY, "/v", RUN_VALUE, "/t", "REG_SZ", "/d", &command, "/f",
            ])
        }
        (true, Err(_)) => false,
        (false, _) => reg(&["delete", RUN_KEY, "/v", RUN_VALUE, "/f"]),
    };
    if !done {
        log("could not change the start-with-Windows setting");
    }
}

pub fn run() -> Result<()> {
    unsafe {
        // A second launch must not exit silently: it asks the first to show itself.
        let _instance = CreateMutexW(None, false, w!("Local\\SonicVeil.Instance"))?;
        if GetLastError() == ERROR_ALREADY_EXISTS {
            if let Ok(existing) = FindWindowW(window::CLASS, window::MAIN_TITLE) {
                window::post(existing, window::WM_APP_SHOW_SETTINGS);
            }
            return Ok(());
        }
        let _ = SetProcessDpiAwarenessContext(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2);
    }

    window::register()?;
    let main = window::create_main()?;
    let notify = Notify::new(main);

    // The folder has to exist before it can be watched.
    let _ = std::fs::create_dir_all(config_dir());
    let cfg_path = config_dir().join("config.toml");
    let (cfg, cfg_text, cfg_broken) = read_config(&cfg_path);
    let cfg_watch = unsafe {
        FindFirstChangeNotificationW(
            &HSTRING::from(config_dir().as_os_str()),
            false,
            FILE_NOTIFY_CHANGE_LAST_WRITE | FILE_NOTIFY_CHANGE_FILE_NAME | FILE_NOTIFY_CHANGE_SIZE,
        )
    }
    .ok();

    let media_shared = Arc::new(Mutex::new(MediaState::default()));
    let mut app = App {
        gfx: Gfx::new()?,
        main,
        notify,
        tray: Tray::new(main)?,
        hosts: Vec::new(),
        palette: Palette::new(&cfg),
        cfg,
        cfg_path,
        cfg_text,
        cfg_broken,
        cfg_watch,
        reload_at: None,
        save_at: None,
        hotkeys_on: false,
        settings: None,
        media: MediaState::default(),
        media_tx: media::spawn(media_shared.clone(), notify),
        media_shared,
        lyrics: Lyrics::None,
        lyrics_gen: 0,
        lyrics_shared: Arc::default(),
        audio: capture::spawn(notify)?,
        scale: 1.0,
        edit: false,
        hidden: false,
        cover: Cover::None,
        cover_monitor: 0,
        raised: false,
        desktop_recheck_at: None,
        device_lost: false,
        quit: false,
    };

    for kind in Kind::ALL {
        let hwnd = window::create_widget((0, 0, 100, 100))?;
        app.hosts.push(Host {
            kind,
            hwnd,
            surface: app.gfx.surface(hwnd, 100, 100)?,
            widget: kind.create(),
            size: (100, 100),
            visible: false,
            paused: false,
            dirty: true,
            due: Due::Now,
        });
    }
    if app.cfg_text.is_empty() {
        // First run: write the defaults out so there is a file to edit.
        app.save_config();
    }
    app.apply_config();
    // Whatever is already in front may be covering the desktop.
    app.foreground_changed(unsafe { GetForegroundWindow() });
    app.run_loop();
    if app.save_at.is_some() {
        app.save_config();
    }
    app.tray.remove();
    Ok(())
}

impl App {
    // ----- the loop --------------------------------------------------------

    fn run_loop(&mut self) {
        let mut msg = MSG::default();
        while !self.quit {
            unsafe {
                while PeekMessageW(&mut msg, None, 0, 0, PM_REMOVE).as_bool() {
                    if msg.message == WM_QUIT {
                        return;
                    }
                    let _ = TranslateMessage(&msg);
                    DispatchMessageW(&msg);
                    self.drain_events();
                }
            }
            self.drain_events();
            if self.quit {
                // Checked here, not only at the top: the wait below can be long.
                break;
            }

            match self.frame() {
                // Blocks until the compositor's next refresh.
                Wake::Frame => {
                    if unsafe { DwmFlush() }.is_err() {
                        self.wait(16);
                    }
                }
                Wake::After(delay) => self.wait(delay.as_millis().clamp(1, 3_600_000) as u32),
                Wake::Idle => self.wait(INFINITE),
            }
        }
    }

    fn drain_events(&mut self) {
        while let Some(event) = window::next_event() {
            self.handle(event);
        }
    }

    /// Sleeps until a message arrives, the config folder changes, or `ms` pass.
    fn wait(&mut self, ms: u32) {
        let handles: &[HANDLE] = self.cfg_watch.as_slice();
        let woke = unsafe { MsgWaitForMultipleObjects(Some(handles), false, ms, QS_ALLINPUT) };
        if let Some(watch) = self.cfg_watch
            && woke == WAIT_OBJECT_0
        {
            self.reload_at = Some(now_ms() + RELOAD_DELAY_MS);
            if unsafe { FindNextChangeNotification(watch) }.is_err() {
                self.cfg_watch = None;
            }
        }
    }

    /// Ticks what is due, draws what changed, and says when to run again.
    fn frame(&mut self) -> Wake {
        let now = now_ms();
        if self.reload_at.is_some_and(|at| now >= at) {
            self.reload_at = None;
            self.reload_config();
        }
        if self.desktop_recheck_at.is_some_and(|at| now >= at) {
            self.desktop_recheck_at = None;
            self.check_show_desktop(unsafe { GetForegroundWindow() });
        }
        if self.save_at.is_some_and(|at| now >= at) {
            self.save_at = None;
            self.save_config();
        }
        if self.device_lost {
            self.rebuild_graphics();
        }
        if self.settings.as_ref().is_some_and(|panel| panel.dirty) {
            self.draw_settings();
        }

        let (time, ms_to_next_second) = local_time();
        let ctx = Ctx {
            cfg: &self.cfg,
            media: &self.media,
            lyrics: &self.lyrics,
            lyrics_gen: self.lyrics_gen,
            audio: &self.audio,
            now_ms: now,
            time,
            ms_to_next_second,
            text: self.palette.text,
            card: self.palette.card,
            accent: self.palette.accent(&self.media),
        };

        let mut wake = Wake::Idle;
        for host in &mut self.hosts {
            if !host.visible || host.paused {
                continue;
            }
            if host.due == Due::Now || matches!(host.due, Due::At(at) if now >= at) {
                let tick = host.widget.tick(&ctx);
                host.dirty |= tick.dirty;
                host.due = match tick.wake {
                    Wake::Frame => Due::Now,
                    Wake::After(delay) => Due::At(now + delay.as_secs_f64() * 1000.0),
                    Wake::Idle => Due::Never,
                };
            }
            if host.dirty {
                host.dirty = false;
                if let Err(error) = draw_host(&mut self.gfx, host, &ctx, self.scale, self.edit) {
                    log(&format!("drawing {} failed: {error}", host.kind.label()));
                    self.device_lost = true;
                }
            }
            wake = wake.sooner(match host.due {
                Due::Now => Wake::Frame,
                Due::At(at) => Wake::after_ms(at - now),
                Due::Never => Wake::Idle,
            });
        }

        if self.settings.as_ref().is_some_and(|panel| panel.dirty) {
            wake = Wake::Frame;
        }
        for deadline in [self.reload_at, self.save_at, self.desktop_recheck_at]
            .into_iter()
            .flatten()
        {
            wake = wake.sooner(Wake::after_ms(deadline - now));
        }
        if self.device_lost {
            wake = wake.sooner(Wake::after_ms(500.0));
        }
        wake
    }

    /// Makes every widget tick on the next pass: something they read changed.
    fn poke(&mut self) {
        for host in &mut self.hosts {
            host.due = Due::Now;
        }
    }

    fn redraw_all(&mut self) {
        for host in &mut self.hosts {
            host.due = Due::Now;
            host.dirty = true;
        }
    }

    /// Draws the settings panel and acts on whatever was clicked in it.
    fn draw_settings(&mut self) {
        let (_, _, aw, ah) = window::work_area();
        let area = (
            (aw as f32 / self.scale) as i32,
            (ah as f32 / self.scale) as i32,
        );
        let accent = self.palette.accent(&self.media);
        let mut cfg = self.cfg.clone();
        let Some(panel) = &mut self.settings else {
            return;
        };
        let drawn = panel.draw(&mut self.gfx, &mut cfg, self.scale, accent, self.edit, area);
        let outcome = match drawn {
            Ok(outcome) => outcome,
            Err(error) => {
                log(&format!("drawing the settings panel failed: {error}"));
                self.device_lost = true;
                return;
            }
        };
        if outcome.changed {
            self.cfg = cfg.sanitized();
            self.save_at = Some(now_ms() + SAVE_DELAY_MS);
            self.apply_config();
        }
        if outcome.toggle_edit {
            self.toggle_edit();
        }
        if outcome.open_config {
            self.open_config_file();
        }
    }

    /// After a GPU reset or driver update every device object is dead.
    fn rebuild_graphics(&mut self) {
        let rebuilt = Gfx::new().and_then(|gfx| {
            for host in &mut self.hosts {
                host.surface = gfx.surface(host.hwnd, host.size.0 as u32, host.size.1 as u32)?;
                host.widget.reset();
            }
            if let Some(panel) = &mut self.settings {
                panel.rebuild(&gfx)?;
            }
            Ok(gfx)
        });
        match rebuilt {
            Ok(gfx) => {
                self.gfx = gfx;
                self.device_lost = false;
                self.redraw_all();
            }
            Err(error) => log(&format!("could not rebuild graphics: {error}")),
        }
    }

    // ----- events ----------------------------------------------------------

    fn handle(&mut self, event: Event) {
        match event {
            Event::Media => self.media_changed(),
            Event::Lyrics => {
                if let Ok(mut shared) = self.lyrics_shared.lock()
                    && shared.track_gen == self.media.track_gen
                {
                    self.lyrics = std::mem::take(&mut shared.lyrics);
                    self.lyrics_gen += 1;
                }
                self.poke();
            }
            Event::Audio => self.poke(),
            Event::Display => {
                self.sync_windows();
                self.redraw_all();
            }
            Event::TaskbarCreated => self.tray.add(),
            Event::ShowSettings => self.open_settings(),
            Event::Tray(WM_RBUTTONUP | WM_CONTEXTMENU) => self.show_menu(),
            Event::Tray(WM_LBUTTONUP) => self.open_settings(),
            Event::Tray(_) => {}
            Event::Command(id) => self.command(id),
            Event::Hotkey(id) => self.hotkey(id),
            Event::Foreground(hwnd) => self.foreground_changed(hwnd),
            Event::Moved(hwnd) => self.widget_moved(hwnd),
            Event::Mouse { hwnd, kind, x, y } => self.mouse(hwnd, kind, x, y),
            Event::Wheel { hwnd, delta } => {
                if let Some(panel) = self.settings.as_mut().filter(|panel| panel.hwnd == hwnd) {
                    panel.wheel(delta);
                }
            }
            Event::Close(hwnd) => {
                if let Some(panel) = self.settings.take_if(|panel| panel.hwnd == hwnd) {
                    panel.close();
                }
            }
        }
    }

    fn media_changed(&mut self) {
        let Ok(new) = self.media_shared.lock().map(|state| state.clone()) else {
            return;
        };
        let was_present = self.media.present;
        let new_track = new.track_gen != self.media.track_gen;
        self.media = new;

        if self.media.present != was_present {
            // Spotify opened or closed: the capture thread has a process to
            // find or lose, and the Spotify-only widgets appear or go.
            self.audio.kick();
            self.sync_windows();
        }
        if new_track || (self.media.present && matches!(self.lyrics, Lyrics::None)) {
            self.audio.kick();
            self.fetch_lyrics();
        }
        self.poke();
    }

    fn fetch_lyrics(&mut self) {
        let wanted = self.cfg.lyrics.enabled && self.media.present && !self.media.title.is_empty();
        self.lyrics = if wanted {
            Lyrics::Loading
        } else {
            Lyrics::None
        };
        self.lyrics_gen += 1;
        if let Ok(mut shared) = self.lyrics_shared.lock() {
            shared.track_gen = self.media.track_gen;
            shared.lyrics = Lyrics::None;
        }
        if wanted {
            let query = Query {
                title: self.media.title.clone(),
                artist: self.media.artist.clone(),
                album: self.media.album.clone(),
                duration_ms: self.media.duration_ms as i64,
            };
            lyrics::fetch(
                query,
                self.media.track_gen,
                self.lyrics_shared.clone(),
                self.notify,
            );
        }
    }

    fn host_index(&self, hwnd: HWND) -> Option<usize> {
        self.hosts.iter().position(|host| host.hwnd == hwnd)
    }

    fn mouse(&mut self, hwnd: HWND, kind: Mouse, x: i32, y: i32) {
        if let Some(panel) = self.settings.as_mut().filter(|panel| panel.hwnd == hwnd) {
            panel.mouse(kind, x as f32 / self.scale, y as f32 / self.scale);
            return;
        }
        let Some(index) = self.host_index(hwnd) else {
            return;
        };
        if self.edit {
            return;
        }
        let host = &mut self.hosts[index];
        let at = (x as f32 / self.scale, y as f32 / self.scale);
        let action = match kind {
            Mouse::Move => {
                host.dirty |= host.widget.hover(Some(at));
                None
            }
            Mouse::Leave => {
                host.dirty |= host.widget.hover(None);
                None
            }
            Mouse::Up => host.widget.click(at.0, at.1),
            Mouse::Down => None,
        };
        // A send fails only if the media thread is gone; there is nothing to control then.
        let _ = match action {
            Some(Action::PlayPause) => self.media_tx.send(Cmd::PlayPause),
            Some(Action::Next) => self.media_tx.send(Cmd::Next),
            Some(Action::Previous) => self.media_tx.send(Cmd::Previous),
            Some(Action::Seek(fraction)) => self
                .media_tx
                .send(Cmd::SeekMs(fraction * self.media.duration_ms)),
            None => Ok(()),
        };
    }

    fn widget_moved(&mut self, hwnd: HWND) {
        let Some(index) = self.host_index(hwnd) else {
            return;
        };
        let kind = self.hosts[index].kind;
        let (x, y, w, h) = window::bounds(hwnd);
        let (ax, ay, aw, ah) = window::work_area();
        let dip = |px: i32| (px as f32 / self.scale).round() as i32;

        let frame = kind.frame_mut(&mut self.cfg);
        frame.w = dip(w).max(1) as u32;
        frame.h = dip(h).max(1) as u32;
        frame.set_origin(dip(x - ax), dip(y - ay), dip(aw), dip(ah));
        self.cfg = self.cfg.clone().sanitized();
        self.save_config();
        self.sync_windows();
    }

    fn foreground_changed(&mut self, hwnd: HWND) {
        let own_panel = self
            .settings
            .as_ref()
            .is_some_and(|panel| panel.hwnd == hwnd);
        if hwnd == self.main || own_panel || self.host_index(hwnd).is_some() {
            return;
        }
        let front = (window::cover(hwnd), window::monitor(hwnd));
        if front != (self.cover, self.cover_monitor) {
            (self.cover, self.cover_monitor) = front;
            self.sync_windows();
            self.redraw_all();
        }
        self.check_show_desktop(hwnd);
        self.desktop_recheck_at = Some(now_ms() + DESKTOP_RECHECK_MS);
    }

    /// Keeps desktop-layer widgets visible through Show Desktop (Win+D), which
    /// raises the desktop over every window, bottom-pinned ones included.
    fn check_show_desktop(&mut self, foreground: HWND) {
        if self.cfg.general.layer != Layer::Desktop || self.edit {
            return;
        }
        let Some(probe) = self.hosts.iter().find(|host| host.visible) else {
            return;
        };
        let covered = window::desktop_covers(probe.hwnd);
        if covered && !self.raised {
            self.raised = true;
            window::set_pin_bottom(false);
            for host in &self.hosts {
                // Topmost then not: lands at the top of the ordinary windows,
                // which is just above the raised desktop.
                window::set_z(host.hwnd, Z::Top);
                window::set_z(host.hwnd, Z::Normal);
            }
        } else if self.raised && !covered && !window::is_shell(foreground) {
            // A real window is in front again: back under everything.
            self.raised = false;
            self.sync_windows();
        }
    }

    // ----- commands --------------------------------------------------------

    fn hotkey(&mut self, id: i32) {
        match id {
            HOTKEY_EDIT => self.toggle_edit(),
            HOTKEY_HIDE => self.toggle_hidden(),
            HOTKEY_SETTINGS => self.open_settings(),
            HOTKEY_OFFSET_EARLIER => self.nudge_offset(OFFSET_STEP_MS),
            HOTKEY_OFFSET_LATER => self.nudge_offset(-OFFSET_STEP_MS),
            _ => {}
        }
    }

    fn command(&mut self, id: u32) {
        match id {
            CMD_SETTINGS => self.open_settings(),
            CMD_EDIT => self.toggle_edit(),
            CMD_OPEN_CONFIG => self.open_config_file(),
            CMD_AUTOSTART => set_autostart(!autostart_enabled()),
            CMD_HIDE => self.toggle_hidden(),
            CMD_QUIT => self.quit = true,
            _ => {
                if let Some(&kind) = Kind::ALL.get(id.wrapping_sub(CMD_WIDGET) as usize) {
                    let enabled = kind.enabled_mut(&mut self.cfg);
                    *enabled = !*enabled;
                } else if let Some(&preset) = Preset::ALL.get(id.wrapping_sub(CMD_PRESET) as usize)
                {
                    self.cfg.apply_preset(preset);
                } else if let Some(&(layer, _)) = LAYERS.get(id.wrapping_sub(CMD_LAYER) as usize) {
                    self.cfg.general.layer = layer;
                } else {
                    return;
                }
                self.save_config();
                self.apply_config();
            }
        }
    }

    fn show_menu(&mut self) {
        let mut items = vec![
            Item::new(CMD_SETTINGS, "Settings\tCtrl+Alt+S"),
            Item::check(CMD_EDIT, "Edit layout\tCtrl+Alt+E", self.edit),
            Item::check(CMD_HIDE, "Hide widgets\tCtrl+Alt+H", self.hidden),
            Item::SEPARATOR,
        ];
        for (i, kind) in Kind::ALL.into_iter().enumerate() {
            items.push(Item::check(
                CMD_WIDGET + i as u32,
                kind.label(),
                kind.enabled(&self.cfg),
            ));
        }
        items.push(Item::SEPARATOR);
        for (i, (layer, label)) in LAYERS.into_iter().enumerate() {
            let current = self.cfg.general.layer == layer;
            items.push(Item::check(CMD_LAYER + i as u32, label, current));
        }
        items.push(Item::SEPARATOR);
        let presets: Vec<String> = Preset::ALL
            .iter()
            .map(|preset| format!("Look: {}", preset.name()))
            .collect();
        for (i, label) in presets.iter().enumerate() {
            items.push(Item::new(CMD_PRESET + i as u32, label));
        }
        items.push(Item::SEPARATOR);
        items.push(Item::new(CMD_OPEN_CONFIG, "Open config file"));
        items.push(Item::check(
            CMD_AUTOSTART,
            "Start with Windows",
            autostart_enabled(),
        ));
        items.push(Item::SEPARATOR);
        items.push(Item::new(CMD_QUIT, "Quit"));

        if let Err(error) = self.tray.menu(&items) {
            log(&format!("could not show the tray menu: {error}"));
        }
    }

    fn toggle_edit(&mut self) {
        self.edit = !self.edit;
        self.raised = false;
        if self.edit {
            self.hidden = false;
        }
        if let Some(panel) = &mut self.settings {
            panel.dirty = true;
        }
        self.sync_windows();
        self.redraw_all();
    }

    fn toggle_hidden(&mut self) {
        self.hidden = !self.hidden;
        self.sync_windows();
    }

    fn nudge_offset(&mut self, by_ms: i32) {
        self.cfg.lyrics.offset_ms = (self.cfg.lyrics.offset_ms + by_ms).clamp(-10_000, 10_000);
        self.save_config();
        self.poke();
    }

    fn open_settings(&mut self) {
        if let Some(panel) = &self.settings {
            panel.focus();
            return;
        }
        match Panel::open(&self.gfx, self.scale) {
            Ok(panel) => self.settings = Some(panel),
            Err(error) => {
                // The panel is a convenience; the file is always editable.
                log(&format!("could not open the settings panel: {error}"));
                self.open_config_file();
            }
        }
    }

    fn open_config_file(&mut self) {
        if !self.cfg_path.exists() {
            self.save_config();
        }
        if let Err(error) = Command::new("notepad").arg(&self.cfg_path).spawn() {
            log(&format!("could not open the config file: {error}"));
        }
    }

    // ----- config ----------------------------------------------------------

    fn save_config(&mut self) {
        self.save_at = None;
        let text = self.cfg.to_toml();
        let written = std::fs::create_dir_all(config_dir()).and_then(|()| {
            if self.cfg_broken {
                // Never overwrite a file the user wrote and this app could not read.
                std::fs::rename(&self.cfg_path, self.cfg_path.with_extension("broken.toml"))?;
            }
            std::fs::write(&self.cfg_path, &text)
        });
        match written {
            Ok(()) => {
                self.cfg_text = text;
                self.cfg_broken = false;
            }
            Err(error) => log(&format!("could not save config.toml: {error}")),
        }
    }

    /// The config folder changed on disk: pick up an edit made by the user.
    fn reload_config(&mut self) {
        let Ok(text) = std::fs::read_to_string(&self.cfg_path) else {
            return;
        };
        if text == self.cfg_text {
            return;
        }
        match Config::from_toml(&text) {
            Ok(cfg) => {
                self.cfg = cfg;
                self.cfg_broken = false;
                self.apply_config();
            }
            Err(error) => {
                // Keep running on the last good config while the file is mid-edit.
                log(&format!(
                    "config.toml does not parse, keeping the old settings: {error}"
                ));
                self.cfg_broken = true;
            }
        }
        self.cfg_text = text;
    }

    /// Pushes `self.cfg` out to everything that depends on it.
    fn apply_config(&mut self) {
        self.palette = Palette::new(&self.cfg);
        self.audio.set_source(
            self.cfg
                .visualizer
                .enabled
                .then_some(self.cfg.visualizer.source),
        );

        if self.cfg.general.hotkeys != self.hotkeys_on {
            self.hotkeys_on = self.cfg.general.hotkeys;
            let modifiers = MOD_CONTROL | MOD_ALT | MOD_NOREPEAT;
            for (id, key) in HOTKEYS {
                let changed = unsafe {
                    if self.hotkeys_on {
                        RegisterHotKey(Some(self.main), id, modifiers, key)
                    } else {
                        UnregisterHotKey(Some(self.main), id)
                    }
                };
                if changed.is_err() && self.hotkeys_on {
                    log(&format!("hotkey {id} is taken by another app"));
                }
            }
        }
        if let Some(panel) = &mut self.settings {
            panel.dirty = true;
        }

        if self.cfg.lyrics.enabled == matches!(self.lyrics, Lyrics::None) {
            // Lyrics were just switched on with a track playing, or off.
            self.fetch_lyrics();
        }
        for host in &mut self.hosts {
            host.widget.reset();
        }
        self.sync_windows();
        self.redraw_all();
    }

    /// Positions, sizes, shows, hides and orders every widget window.
    fn sync_windows(&mut self) {
        let (ax, ay, aw, ah) = window::work_area();
        self.scale = window::dpi(self.main) as f32 / 96.0;
        let scale = self.scale;
        let px = |dip: f32| (dip * scale).round() as i32;
        let layer = self.cfg.general.layer;
        let pausing =
            self.cfg.general.pause_on_fullscreen && !self.edit && self.cover != Cover::None;

        // Unpinned while reordering, or the pin would undo it.
        window::set_pin_bottom(false);
        window::set_edit_mode(self.edit);

        for host in &mut self.hosts {
            let frame = host.kind.frame(&self.cfg);
            let (left, top) = frame.origin((aw as f32 / scale) as i32, (ah as f32 / scale) as i32);
            let bounds = (
                ax + px(left as f32),
                ay + px(top as f32),
                px(frame.w as f32).max(1),
                px(frame.h as f32).max(1),
            );
            if window::bounds(host.hwnd) != bounds {
                window::set_bounds(host.hwnd, bounds);
            }
            if host.size != (bounds.2, bounds.3) {
                host.size = (bounds.2, bounds.3);
                if let Err(error) =
                    self.gfx
                        .resize(&mut host.surface, bounds.2 as u32, bounds.3 as u32)
                {
                    log(&format!("resizing {} failed: {error}", host.kind.label()));
                    self.device_lost = true;
                }
                host.dirty = true;
            }

            // A widget under a window that covers its monitor cannot be seen,
            // so it stops running. An always-on-top widget is above a maximized
            // window and keeps going; under true fullscreen it is hidden.
            host.paused = pausing
                && window::monitor(host.hwnd) == self.cover_monitor
                && (layer != Layer::Top || self.cover == Cover::Fullscreen);

            let has_content = !host.kind.needs_spotify()
                || self.media.present
                || !self.cfg.general.hide_without_spotify;
            let visible = host.kind.enabled(&self.cfg)
                && !self.hidden
                && !(host.paused && layer == Layer::Top)
                && (self.edit || has_content);
            if visible != host.visible {
                host.visible = visible;
                window::show(host.hwnd, visible);
                host.dirty = true;
                host.due = Due::Now;
            }

            window::set_click_through(host.hwnd, !self.edit && !host.kind.interactive());
            if self.edit || layer == Layer::Top {
                window::set_z(host.hwnd, Z::Top);
            } else if layer == Layer::Normal {
                window::set_z(host.hwnd, Z::Normal);
            } else if !self.raised {
                window::set_z(host.hwnd, Z::Normal);
                window::set_z(host.hwnd, Z::Bottom);
            }
        }

        window::set_pin_bottom(layer == Layer::Desktop && !self.edit && !self.raised);
    }
}

/// Draws one widget into its window, with the edit-mode frame on top.
fn draw_host(gfx: &mut Gfx, host: &mut Host, ctx: &Ctx, scale: f32, edit: bool) -> Result<()> {
    let (w, h) = (host.size.0 as f32 / scale, host.size.1 as f32 / scale);
    gfx.begin(&host.surface);
    gfx.set_transform(scale, 0.0, 0.0);
    let drawn = host.widget.draw(gfx, w, h, ctx);
    let framed = if edit {
        draw_edit_frame(gfx, host.kind, w, h, ctx)
    } else {
        Ok(())
    };
    gfx.end(&host.surface).and(drawn).and(framed)
}

/// The outline and label shown while the layout is being edited.
pub fn draw_edit_frame(gfx: &mut Gfx, kind: Kind, w: f32, h: f32, ctx: &Ctx) -> Result<()> {
    let radius = ctx.cfg.theme.card_radius.min(w.min(h) / 2.0);
    gfx.fill_round(rect(0.0, 0.0, w, h), radius, with_alpha(ctx.accent, 0.10));
    gfx.stroke_round(rect(1.0, 1.0, w - 2.0, h - 2.0), radius, ctx.accent, 2.0);

    let label = format!("{}  {} × {}", kind.label(), w.round(), h.round());
    let style = TextStyle {
        font: &ctx.cfg.theme.font,
        size: 12.0,
        weight: 600,
        align: Align::Left,
        wrap: false,
    };
    let layout = gfx.layout(&label, &style, w - 20.0, 20.0)?;
    let (text_w, text_h) = Gfx::measure(&layout);
    gfx.fill_round(
        rect(8.0, 8.0, text_w + 16.0, text_h + 6.0),
        (text_h + 6.0) / 2.0,
        ctx.accent,
    );
    gfx.draw_text(&layout, 16.0, 11.0, [0.0, 0.0, 0.0, 0.92], false);
    Ok(())
}
