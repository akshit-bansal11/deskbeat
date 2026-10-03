This project inherits every rule in ~/.claude/CLAUDE.md. Rules below add to or override it. Nothing here restates it.

- Reference ID: `yfu0idauhvj2nm9nkp0lcgkd7-yw5rqx3wn4`. Every commit carries it as a `Ref-ID:` trailer.
- Non-code facts live on the Notion project page, not in this repo. Do not create `STATE.md`, `DECISIONS.md`, `DRIFT.md`, `TECH-STACK.md` or `DIRECTORY-STRUCTURE.md` here.
- There is no `package.json`. The quality gate is cargo, and it runs on GitHub Actions because this PC has no Rust toolchain:
  - `check` (writes): `cargo fmt --all && cargo clippy --workspace --all-targets -- -D warnings && cargo test --workspace`
  - `check:ci` (non-mutating, what `ci.yml` runs): `cargo fmt --all --check && cargo clippy --workspace --all-targets --locked -- -D warnings && cargo test --workspace --locked`
- Locally only formatting can run, with a standalone `rustfmt.exe` unpacked into the session scratchpad from static.rust-lang.org (the `rustfmt` and `rustc` components; rustfmt needs rustc's DLLs beside it). Run it as `rustfmt --edition 2024 <files>` before every commit.
- After any `Cargo.toml` change, dispatch `lockfile.yml` on the branch and commit the `Cargo.lock` it uploads. CI uses `--locked`.
- `crates/core` must stay free of Windows types so every test runs anywhere. Anything that needs a window, a device or Spotify goes in `crates/app`.
- Widgets draw only when something changed. A widget that returns `Wake::Frame` while nothing on it is moving is a bug: it is the whole reason this app exists instead of Rainmeter.
- The `windows` crate is pinned to 0.62. Its API moves between minor versions; do not bump it as a drive-by.
