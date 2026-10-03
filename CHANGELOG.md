# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) and the project
uses [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2026-10-03

First build. A pre-release: see "Not yet verified" below before relying on it.

### Added

- Four desktop widgets for Spotify on Windows 11, in one native app: a
  spectrum visualizer, synced lyrics, a clock and a now-playing player.
- Track, position and transport controls from the Windows media session, so
  there is no Spotify login and no client ID.
- Visualizer audio captured from Spotify's process alone, with the whole
  system as an option. Bars, mirrored bars and wave styles.
- Lyrics from LRCLIB with a disk cache, highlighted line by line or word by
  word, scrolling between lines.
- A player card with album art, a blurred-art background, a progress bar that
  seeks on click, and previous, play/pause and next buttons.
- A settings panel, a tray menu, four one-click looks, and an edit mode that
  moves and resizes widgets by dragging.
- A hand-editable `config.toml` that reloads when saved.
- Global hotkeys: Ctrl+Alt+E edit layout, Ctrl+Alt+H hide, Ctrl+Alt+S
  settings, Ctrl+Alt+Up and Down to nudge lyric timing.
- Widgets stop drawing while a window covers their monitor.
- `--snapshot`, `--probe`, `--probe-system` and `--probe-lyrics` diagnostics.

### Not yet verified

- Nothing has been run against a live Spotify session: track details, the
  position Spotify reports, transport buttons and Spotify-only audio capture
  are untested on a real machine.
- Staying visible through Show Desktop (Win+D) is untested.
- The exe is unsigned, so Windows SmartScreen warns on first launch.

[Unreleased]: https://github.com/akshit-bansal11/sonic-veil/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/akshit-bansal11/sonic-veil/releases/tag/v0.1.0
