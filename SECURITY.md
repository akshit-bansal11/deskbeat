# Security

Deskbeat draws on your desktop, listens to what Spotify plays and asks the internet for lyrics. This is what it touches, what leaves the PC, and how to report a hole.

## The model

- **No account, no login, no telemetry.** There is no Spotify client ID and no token. Track details and the play, pause and skip buttons go through the Windows media session API, the same one the volume flyout uses.
- **What leaves the PC is the track you are playing, to two lyric services.** For each new track, its title, artist, album and length are sent over HTTPS to [LRCLIB](https://lrclib.net) and, when word timing is on, to the LyricsPlus servers listed under `word_servers` in the config. Nothing else is sent: no identifier, no listening history, no audio. Turning the lyrics widget off stops both; `word_sync = false` stops the second.
- **The LyricsPlus servers are community-run mirrors, not a service with a contract.** Whoever runs one sees the requests above, and your IP address as any web server does. The list is yours to edit or empty.
- **Audio never leaves the process.** The visualizer captures Spotify's output through WASAPI loopback, turns it into bar heights and discards it. Nothing is recorded or written to disk.
- **Lyric text is treated as text.** A response is parsed as JSON and drawn with DirectWrite. Nothing in it is executed, and nothing from it reaches a shell, a path or a URL.
- **It runs as you, with no elevation.** It writes only its own folders, `%APPDATA%\deskbeat` and `%LOCALAPPDATA%\deskbeat`, and one value under `HKCU\...\Run` when Start with Windows is on.
- **No secrets in the repository.** The release workflow has none to hold: its only credential is the job's own token.

## Known limits

- The exe is not code-signed, so SmartScreen warns on first run. Verify a download against the release's `SHA256SUMS.txt`, or with `gh attestation verify Deskbeat-<version>.exe --repo akshit-bansal11/deskbeat`, which proves the file was built by this repository's release workflow at that tag.
- Anyone who can write `%APPDATA%\deskbeat\config.toml` can point `word_servers` at a server of their choosing and learn what you play. That person can already run programs as you.
- Fonts in `%APPDATA%\deskbeat\fonts` are loaded by DirectWrite. A malicious font file is a risk for any program that renders it; only put fonts there that you trust.

## Reporting

Open a [security advisory](https://github.com/akshit-bansal11/deskbeat/security/advisories/new) on GitHub; it reaches the maintainer privately. Please do not open a public issue for something exploitable. Expect an acknowledgement within a week.
