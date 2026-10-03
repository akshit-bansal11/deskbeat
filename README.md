# Sonic Veil

Desktop widgets for Spotify on Windows 11, in one small native app: a spectrum visualizer, synced lyrics, a clock, and a now-playing player.

It is built as a light alternative to a Rainmeter setup. Widgets redraw only when something changes, and nothing is drawn at all while Spotify is silent or a fullscreen app is in front.

> Work in progress. There is no release yet.

## How it works

- **Track and controls** come from the Windows media session, so there is no Spotify login, no client ID, and it works on free accounts.
- **The visualizer** listens to Spotify's audio only, not to games or calls.
- **Lyrics** come from [LRCLIB](https://lrclib.net) and are cached on disk.
- **Drawing** is Direct2D on a transparent, click-through window per widget.

## Requirements

Windows 11 and the Spotify desktop app, playing on this PC.

## Building

```
cargo build --release
```

The quality gate is `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -- -D warnings` and `cargo test --workspace`.

## Licence

MIT
