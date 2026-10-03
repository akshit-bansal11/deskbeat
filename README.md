# Sonic Veil

Desktop widgets for Spotify on Windows 11, in one small native app: a spectrum visualizer, synced lyrics, a clock, and a now-playing player.

It is built as a light alternative to a Rainmeter setup. Widgets redraw only when something changes, and nothing is drawn at all while Spotify is silent or a window covers the desktop.

> **Pre-release.** The app has not yet been run against a live Spotify session. See [What has and has not been tested](#what-has-and-has-not-been-tested).

## What you get

| Widget | What it shows |
| --- | --- |
| **Visualizer** | A spectrum of Spotify's audio only, not your games or calls. Bars, mirrored bars or a wave. |
| **Lyrics** | Time-synced lyrics from [LRCLIB](https://lrclib.net), highlighted by line or by word, cached on disk. |
| **Player** | Album art, title, artist, a progress bar you can click to seek, and previous, play/pause and next. |
| **Clock** | Day, time and date. |

There is no Spotify login and no client ID: track details and controls come from Windows itself, so it works on free accounts.

## Install

Download `sonic-veil.exe` from [Releases](https://github.com/akshit-bansal11/sonic-veil/releases) and run it. There is no installer. The exe is unsigned, so Windows SmartScreen will warn the first time: choose **More info**, then **Run anyway**.

It needs Windows 11 and the Spotify desktop app playing on the same PC. Playback sent to another device through Spotify Connect is not seen.

## Use

Sonic Veil lives in the notification area. Right-click its icon for the menu, or left-click it for settings.

| Hotkey | Does |
| --- | --- |
| `Ctrl+Alt+S` | Open settings |
| `Ctrl+Alt+E` | Edit layout: drag a widget to move it, drag an edge to resize it |
| `Ctrl+Alt+H` | Hide or show every widget |
| `Ctrl+Alt+Up` / `Down` | Show lyrics 100 ms earlier / later |

Hotkeys can be turned off in settings.

## Customise

The settings panel covers the common choices: four one-click looks, fonts, colours, sizes, which widgets show, where they sit, and how the visualizer moves.

Everything is stored in one file, which you can also edit by hand. It reloads when you save:

```
%APPDATA%\sonic-veil\config.toml
```

The file takes a few things the panel does not: any hex colour, any installed font, and a hand-written clock format (`%H:%M`, `%l:%M %p`, `%A, %e %B` and so on).

Widgets can sit on the desktop under your windows (the default), behave like a normal window, or stay on top of everything.

## If a widget stays empty

Sonic Veil has no console, so its diagnostics write to a file. In a terminal:

```
sonic-veil.exe --probe > probe.txt
```

With Spotify playing, this records for eight seconds what Windows reports about the track and how loud Spotify's audio is. Two more:

- `--probe-system` listens to everything the PC plays instead of Spotify alone.
- `--probe-lyrics "Title" "Artist"` looks one track up on LRCLIB.

Errors are logged to `%LOCALAPPDATA%\sonic-veil\sonic-veil.log`. The lyrics cache is in the same folder and is safe to delete.

## What has and has not been tested

Tested on a real Windows 11 desktop:

- All four widgets draw, sit on the desktop layer, and cost no CPU while idle.
- Lyrics lookup, title cleaning and the disk cache, against LRCLIB.
- Audio capture, both for one app and for the whole system.
- Quitting from the tray.

Not yet tested, because Spotify was not running when this was built:

- Reading the track, position and album art from Spotify.
- The transport buttons and seeking.
- Lyrics staying in time with Spotify's reported position.
- Staying visible through Show Desktop (`Win+D`).

Memory use measured about 54 MB, above the 40 MB this was aiming for.

## Build

```
cargo build --release
```

The quality gate is `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`. `sonic-veil.exe --snapshot <dir>` renders every widget to PNG from made-up data, with no GPU or Spotify, which is how CI checks the drawing code.

## How it stays light

- **No webview and no runtime.** One Rust process drawing with Direct2D.
- **Event-driven.** The clock wakes once a minute, lyrics at the next line, the player once a second. The visualizer runs only while there is sound.
- **Nothing hidden is drawn.** A widget under a maximized or fullscreen window stops entirely.
- **Low-power GPU.** On a laptop with two GPUs it uses the integrated one.

## Licence

MIT
