This project inherits every rule in `~/.claude/CLAUDE.md`. Rules below add to or override it. Nothing here restates it.

Deskbeat was the mini project Sonic Veil until 2026-10-03 and inherited `F:/projects/mini-projects/CLAUDE.md`. It no longer does. This file carries forward the handful of rules from that one that Deskbeat still depends on, and nothing else.

## Notion is the source of truth for everything that is not code

The project page is `https://app.notion.com/p/3eec6445b19f8199bb21f82bdd6b4175`, a row in the **Projects** database under **Claude Second Brain**. It holds intent, architecture, decisions, traps already paid for, current state, open threads, next actions and the chat log. **Read it before planning or answering anything about this project.** Entries from before the rename call the project Sonic Veil. The repo stores code and nothing else.

Where the two disagree, the repo wins on code and the Notion page wins on everything else. If a non-code fact is not on that page, you do not know it — ask rather than guess.

**This overrides the global router's §9.** Do not create `STATE.md`, `DECISIONS.md`, `DRIFT.md`, `OPEN_ITEMS.md`, `TECH-STACK.md` or `DIRECTORY-STRUCTURE.md` here. A claude.ai web or app chat has no filesystem, so pointing such a session at `STATE.md` gives it nothing. Files a mechanism actually reads from disk stay on disk: `README.md`, `LICENSE`, `.gitignore`, `.github/*`, `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`, `docs/*`, and this file.

At the end of a working session, append a dated entry to the page's "Chat log" and refresh "Current state", "Open threads" and "Next actions".

Never write secrets, API keys or credential values into Notion. Variable names only.

## Commits

Conventional Commits, with this trailer:

```
Co-Authored-By: <the model writing the commit> <noreply@anthropic.com>
```

**No `Ref-ID:` trailer.** The mini-project reference ID `yfu0idauhvj2nm9nkp0lcgkd7-yw5rqx3wn4` was retired on promotion and no id replaced it. Every commit through `2bfd415` still carries it, because history cannot be rewritten; `git log --grep=yw5rqx3wn4` finds them. Commits after it carry no id.

## `main` is protected

A ruleset on `main` requires a pull request and the `gate` and `build` checks, and blocks force pushes and deletion. Nobody bypasses it. So every change is a branch and a pull request; a direct `git push origin main` is refused. A second ruleset stops `v*` tags being moved or deleted.

## Nothing heavy runs locally

There is **no Rust toolchain on this machine** (no cargo, rustc or MSVC), by choice. The gate and every build run on GitHub Actions. The owner may be gaming, so ask before starting anything long-running or memory-hungry locally.

- The gate is `scripts/check.ps1`: it writes locally, and CI runs it with `-Ci`, which does not.
- Locally only formatting can run, with a standalone `rustfmt.exe` unpacked into the session scratchpad from static.rust-lang.org (the `rustfmt` and `rustc` components; rustfmt needs rustc's DLLs beside it). Run it as `rustfmt --edition 2024 crates/core/src/lib.rs crates/app/src/main.rs` before every commit.
- After a `Cargo.toml` change that alters third-party packages, dispatch `lockfile.yml` on the branch and commit the `Cargo.lock` it uploads. CI uses `--locked`. A version bump of the two workspace packages is two lines in `Cargo.lock` and can be edited by hand.
- To look at a visual change: download the `deskbeat-windows-x64` artifact of the CI run and view its `snapshots/*.png`, or run its exe with `--snapshot <dir>`.

## Code rules that are easy to break

- `crates/core` must stay free of Windows types so every test runs anywhere. Anything that needs a window, a device or Spotify goes in `crates/app`.
- Widgets draw only when something changed. A widget that returns `Wake::Frame` while nothing on it is moving is a bug: it is the whole reason this app exists instead of Rainmeter.
- The `windows` crate is pinned to 0.62. Its API moves between minor versions; do not bump it as a drive-by. Dependabot is told to leave it alone.
- The owner's real config is `%APPDATA%\deskbeat\config.toml`. Never edit it. Test against a copy, with `APPDATA` and `LOCALAPPDATA` pointed at a scratch folder.
- Only one instance runs. A second launch hands over to the first and exits, so a test run does nothing while the owner's copy is in the tray.

## What is the owner's to do, not a session's

Hand over a command; never route around a refusal.

- **Merging a pull request.** `gh pr merge` is refused to a session as `[Merge Without Review]`, on Edgepad on 2026-09-19 even with the owner's explicit instruction. Open the pull request, wait for its checks, and give the owner the merge command.
- **Deleting a release or a tag.** Refused even with authorisation, and the tag ruleset blocks it besides. This covers a draft release too: a session can create one by hand-running `release.yml` and cannot remove it afterwards.
- **Deleting files or folders with `rm`.** Denied by the harness. Give the owner the command.
- **Uploading the social preview image.** GitHub has no API for it: Settings, General, Social preview, upload `assets/social-preview.png`.
- **Submitting to winget.** `packaging/winget/submit.sh` opens a pull request from the owner's account.

## Releases

Pushing a version tag triggers `.github/workflows/release.yml`, which runs the gate, builds the exe, checks the tag against the crate version, and attaches it named for the version, `Deskbeat-1.0.1.exe`, with `SHA256SUMS.txt` and a provenance attestation. Notes come from that version's section of `CHANGELOG.md`, so the section must exist before the tag. Running the workflow by hand makes a private draft instead.

A release is: a pull request that bumps `version` in `Cargo.toml` and the two workspace packages in `Cargo.lock`, moves Unreleased into a dated section of `CHANGELOG.md`, and adds a `packaging/winget/<version>` folder; the owner merges it; then the tag is pushed on the merge commit. The winget folder's `InstallerSha256` is only known after the release, so it lands in a follow-up pull request.

Word timing depends on community-run LyricsPlus mirrors that come and go. If word-by-word lyrics stop, check the servers in `crates/core/src/lyricsplus.rs` before the code.
