# Developing IPTelly

This document explains how to check out, build, and run IPTelly locally. It's aimed at
people contributing code, not end users - if you just want to use the app, see the
[README](README.md) for install links instead.

## Overview

IPTelly is a Rust desktop app with a [Slint](https://slint.dev/) user interface, in a Cargo
workspace at the repo root:

- `crates/iptelly-core/` - everything except the UI (SQLite for storage, source and EPG
  parsing, downloads, restreaming, `mpv`/`vlc` spawned as external player processes).
- `crates/iptelly/` - the app: the Slint UI (`ui/*.slint`) and the Rust that drives it
  (`src/`), which calls into `iptelly-core`. Long-running core calls run on a tokio runtime,
  never on the UI thread.

## Prerequisites

1. **Rust** - install via [rustup](https://rustup.rs/). The project targets edition 2024
   and Rust 1.91.1+ (the crates' `rust-version`); `rustup` will pick up whatever `stable`
   resolves to, which is fine.
2. **Platform build dependencies**:
   - **Linux (Debian/Ubuntu)**: `sudo apt install build-essential pkg-config libssl-dev libfontconfig1-dev`
     (Fedora: `sudo dnf install gcc pkgconf openssl-devel fontconfig-devel`).
   - **Windows**: Visual Studio Build Tools (C++ workload).
   - **macOS**: Xcode Command Line Tools (`xcode-select --install`).
3. **Runtime dependencies** the app itself shells out to: `mpv`, `ffmpeg`, `yt-dlp` (and
   optionally `vlc`, if you want to test the VLC player option). See the README's
   [Prerequisites](README.md#prerequisites) section for install commands per platform -
   you need these to actually play a stream from a dev build, not just to compile it.

## Getting the source

```
git clone https://github.com/iptelly/iptelly.git
cd iptelly
```

If you're working from a fork, clone your fork instead and add this repo as an
`upstream` remote so you can keep up with changes there.

## Running

```
cargo run -p iptelly
```

Editing `.slint` files needs a rebuild too: `build.rs` compiles them into Rust. The
[Slint extension](https://slint.dev/get-started) for VS Code (or another editor) gives a
live preview of a `.slint` file while you edit it.

Useful things while developing:

- **The SQLite database** lives at (platform data dir)`/dev.iptelly.iptelly/db.sqlite` -
  e.g. `~/.local/share/dev.iptelly.iptelly/db.sqlite` on Linux. Handy to inspect directly
  with `sqlite3` when debugging data issues.
- **Logs** go to the app's log folder, shown in the error dialog (click an error toast).
- **Rendering problems**: `SLINT_BACKEND=winit-software cargo run -p iptelly` draws the
  window without the GPU.

## Building the packages locally

The release workflow (`.github/workflows/buildAndUpload.yml`) builds a `.deb` and an
`.rpm` on Ubuntu, and an `.msi` on Windows. Their configuration is in
`crates/iptelly/Cargo.toml` (`[package.metadata.deb]` and
`[package.metadata.generate-rpm]`) and `crates/iptelly/wix/` for the MSI.

```
cargo install cargo-deb cargo-generate-rpm   # once
cargo build --release -p iptelly
cargo deb -p iptelly --no-build              # target/debian/iptelly_0.0.0-1_amd64.deb
cargo generate-rpm -p crates/iptelly         # target/generate-rpm/iptelly-0.0.0-1.x86_64.rpm
```

The plain binary is at `target/release/iptelly` if you just want to run it without
installing a package.

To install the RPM you just built (Fedora; `ffmpeg` needs RPM Fusion enabled, since the
package depends on it):

```
sudo dnf install ./target/generate-rpm/iptelly-*.rpm
```

Use `dnf reinstall` instead to replace a build of the same version that's already installed.
Local builds are always version `0.0.0` (see [Versioning](#versioning)), so this is the
usual case once you've installed one.

On Windows, the MSI needs the [WiX 3 toolset](https://github.com/wixtoolset/wix3/releases)
and `cargo install cargo-wix`. It bundles `mpv.exe` and `vulkan-1.dll` from
`C:\iptelly-deps` (see `crates/iptelly/wix/deps.wxs`), which the release workflow
downloads first. ffmpeg and yt-dlp aren't bundled, to keep the installer small; the app
tells the user to install ffmpeg if they try to re-stream without it. Then:

```
cargo build --release -p iptelly
cargo wix -p iptelly --no-build              # target/wix/iptelly-0.0.0-x86_64.msi
```

### Versioning

The version in `crates/iptelly/Cargo.toml` is a `0.0.0` placeholder; don't bump it in a
PR. The real version is entered when running the **Build and Upload** workflow (Actions
tab → Build and Upload → Run workflow), which writes it into that file on the build runner
only. It must be plain `MAJOR.MINOR.PATCH` (e.g. `2.1.0`), because the MSI rejects
pre-release suffixes. The workflow only runs from `main`. Once every platform builds, the
workflow tags the commit it built as `v<version>` (e.g. `v2.1.0`); a version whose tag
already exists is rejected up front.

Tick **dry run** to build the packages without tagging, from any branch - for example to
check a packaging change before merging it.

## Verifying before committing

Run these from the repo root:

```
cargo fmt --all --check
RUSTFLAGS="-D warnings" cargo test --workspace --no-default-features
```

That second one is what CI (`.github/workflows/rustLint.yml`) enforces on every PR - it
turns plain rustc warnings (unused imports, unused variables, dead code, ...) into build
failures across every target, including `#[cfg(test)]` modules, which a plain
`cargo check`/`cargo build` never compiles at all.

### App tests

`crates/iptelly/src/` has `#[cfg(test)]` modules: unit tests of helpers (file names, byte
sizes, relative dates, ...) and headless UI tests. The UI tests run the real Slint UI on
Slint's testing backend (`i-slint-backend-testing`), with no window on screen and a mock
clock that `testing::wait` advances. They find elements by their `id` in the `.slint`
files (e.g. `"ChannelsView::search"`), which needs the debug info `build.rs` adds to debug
builds. Anything they do that reaches the database uses a throwaway one in the temp
folder (`testing::window`), never yours.

`i-slint-backend-testing` must be the exact same version as `slint`, so bump them
together.

### `crates/iptelly-core/tests/` - parser integration tests

`tests/m3u_parser_test.rs` runs the real m3u parser (`m3u::read_m3u8`) end-to-end against a
real, messy, third-party-generated playlist (`tests/fixtures/samsung_tvplus_playlist.m3u8`,
2471 channels with no `tvg-name` attributes at all and several `group-title`s containing a
literal comma - a good stress test for the name-parsing fallback) and asserts every channel
the file contains ends up in the database by URL. It points the database at a throwaway
temp file instead of your real one via the `OPEN_TV_DB_PATH` env var (`sql.rs` checks this
before falling back to the normal app data directory) - each file under `tests/` is its own
process, so this can't affect a real running app's database.

Regenerate the fixtures with `samsung_tvplus_fetch.py` (ask Claude, or see its own `-h`) if
you need a fresher/larger one.

### `crates/iptelly-core/benches/` - parser benchmarks

`cargo bench -p iptelly-core` runs `benches/m3u_parse.rs`, which benchmarks
`m3u::get_channel_from_lines` against the same real 2471-channel fixture, plus a synthetic
500,000-channel/20,000-category playlist for seeing how the parser scales well past
anything a real fixture reaches - useful before/after a parsing change to see whether it
actually helped. Needs `get_channel_from_lines` to be `pub` since benches compile as a
separate crate, same as `tests/`.

The synthetic playlist isn't checked in (~90MB) - the benchmark generates and caches it at
`tests/fixtures/synthetic_large_playlist.m3u8` on first run (a few seconds), and reuses
that file on every run after. Delete it to force a fresh one (e.g. after changing the
generator in `benches/m3u_parse.rs`).
