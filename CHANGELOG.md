# Changelog

All notable changes to this project are recorded here. The format follows
[Keep a Changelog 1.1.0](https://keepachangelog.com/en/1.1.0/) and the project
uses [Semantic Versioning 2.0.0](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

### Added

- Every text of the clock and the player has its own font, size, weight,
  colour, opacity, letter spacing and capitals setting. The lyrics take
  their own font.
- The progress bar has a thickness, colour and track opacity; the buttons a
  size and colour; the album art a corner rounding.
- "Hide all widgets", "Start with Windows" and "Quit" in the settings panel.
- The visualizer can span the whole width of the screen, which is now the
  default, with up to 512 bars.
- The visualizer can be flipped left to right and upside down.
- Every element of the clock and the player has its own position: the day,
  time and date, and the player's art, title, artist, three buttons,
  progress bar, elapsed time and total time. In edit mode each is dragged
  separately; empty space still moves the whole widget.
- The clock and the player have no box of their own: their window wraps
  around wherever their elements are, so an element can be dragged anywhere
  without being clipped. In edit mode they show only element outlines.
- The clock's day, time and date can each be hidden.
- "Art on the left" and "Centred stack" arrange the player in one click, as
  a starting point. Album art size and progress bar length are settings.
- Lyrics: separate colours for the current line, the current word and the
  other lines; an outline with its own colour and width; a shadow with its
  own colour and strength.
- Separate fonts for the clock's day row and its time and date rows.
- Fonts placed in `%APPDATA%\sonic-veil\fonts` can be used without
  installing them in Windows.

### Removed

- The tray icon's right-click menu. A double-click opens the settings
  panel, which now holds everything the menu did.
- Frames and labels in edit mode. Only the element under the pointer is
  outlined.
- `clock.day_font`, `day_size`, `time_font`, `time_size`, `text_size` and
  `time_weight`, and `player.title_size` and `artist_size`, replaced by keys
  of each element such as `clock.day.size`. Files that use the old keys
  load and keep their values.
- `clock.align` from the config. A file that still has it loads, and the
  key is ignored.

### Changed

- In word mode the rest of the current line is drawn in the current-line
  colour, and the word being sung in its own colour, which defaults to the
  accent.
- The clock's day and time default to Anurati and Quicksand, the pairing the
  Mond Rainmeter skin uses. Without those fonts the theme font is used.
- Widget width and height can be set up to the size of the screen.

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
