# Developing IPTelly

This document explains how to check out, build, and run IPTelly locally. It's aimed at
people contributing code, not end users - if you just want to use the app, see the
[README](README.md) for install links instead.

## Overview

IPTelly is a [Tauri v2](https://v2.tauri.app/) desktop app:

- **Backend**: Rust, in `src-tauri/` (SQLite for storage, `mpv`/`vlc` spawned as external
  player processes).
- **Frontend**: Angular 17, in `src/` (renders inside the Tauri webview).

## Prerequisites

1. **Rust** - install via [rustup](https://rustup.rs/). The project targets edition 2024
   and Rust 1.91.1+ (`src-tauri/Cargo.toml`'s `rust-version`); `rustup` will pick up
   whatever `stable` resolves to, which is fine.
2. **Node.js 20+** and a package manager - see [Package manager: pnpm vs npm](#package-manager-pnpm-vs-npm)
   below before you pick one.
3. **Platform build dependencies** for Tauri itself:
   - **Linux (Debian/Ubuntu)**:
     ```
     sudo apt install libwebkit2gtk-4.1-dev build-essential curl wget file \
       libxdo-dev libssl-dev libayatana-appindicator3-dev librsvg2-dev libgtk-3-dev
     ```
     (Fedora/openSUSE/Arch equivalents exist for these same packages - check your
     distro's package names for `webkit2gtk`, `appindicator`, `librsvg`, etc.)
   - **Windows**: Visual Studio Build Tools (C++ workload) and the
     [WebView2 runtime](https://developer.microsoft.com/microsoft-edge/webview2/) (usually
     already present on Windows 10/11).
   - **macOS**: Xcode Command Line Tools (`xcode-select --install`).
4. **Runtime dependencies** the app itself shells out to: `mpv`, `ffmpeg`, `yt-dlp` (and
   optionally `vlc`, if you want to test the VLC player option). See the README's
   [Prerequisites](README.md#prerequisites) section for install commands per platform -
   you need these to actually play a stream from a dev build, not just to compile it.

## Getting the source

```
git clone https://github.com/iptelly/iptelly.git
cd open-tv
```

If you're working from a fork, clone your fork instead and add this repo as an
`upstream` remote so you can keep up with changes there.

## Package manager: pnpm vs npm

Local development in this repo has been done with **pnpm** (faster installs, and what
`pnpm-lock.yaml` reflects), but **CI and the release build workflow
(`.github/workflows/buildAndUpload.yml`) use plain `npm install` / `npm run tauri build`**,
which reads `package-lock.json` instead. Both lockfiles are committed and need to stay in
sync with each other and with `src-tauri/Cargo.toml`'s `tauri`/`tauri-plugin-*` crate
versions - `tauri build` hard-fails (not just warns) if the npm `@tauri-apps/*` packages
and the Rust `tauri`/plugin crates drift onto different major.minor versions. If you bump
any `@tauri-apps/*` package or any `tauri-plugin-*` crate, bump the other side to match and
regenerate **both** lockfiles (`npm install` and `pnpm install`) before committing.

For day-to-day dev work, either works - install pnpm via `corepack enable` or
`npm install -g pnpm`, then:

```
pnpm install   # or: npm install
```

## Running in dev mode

```
pnpm tauri dev
```

This starts the Angular dev server (`ng serve`, with hot reload) and builds/launches the
Rust backend (`cargo run`) pointed at it, opening the app window. Editing frontend files
reloads the webview live; editing Rust files under `src-tauri/` triggers a recompile and
restart of the backend, which the Tauri CLI handles automatically - just keep an eye on
the terminal for compile errors.

Useful things while developing:

- **The SQLite database** lives at (platform data dir)`/dev.iptelly.iptelly/db.sqlite` -
  e.g. `~/.local/share/dev.iptelly.iptelly/db.sqlite` on Linux. Handy to inspect directly
  with `sqlite3` when debugging data issues.
- **Quick backend-only check** (faster than a full `tauri dev` cycle when you just want to
  know if Rust changes compile): `cd src-tauri && cargo check --no-default-features`.
- **Quick frontend-only check**: `pnpm ng build --configuration development`.

## Building everything locally

A full production build (optimized Rust release binary + Angular production bundle,
packaged for your OS):

```
pnpm tauri build
```

On Linux, this builds `deb`, `rpm`, and `AppImage` bundles by default (per
`src-tauri/tauri.conf.json`'s `bundle.targets: "all"`). To build only specific bundle
types (faster, useful when iterating on packaging):

```
pnpm tauri build --bundles rpm      # just the RPM
pnpm tauri build --bundles deb      # just the .deb
pnpm tauri build --bundles appimage # just the AppImage
```

Output lands in `src-tauri/target/release/bundle/<type>/`. The plain, unbundled binary
itself is at `src-tauri/target/release/iptelly` if you just want to run it directly
without installing a package.

On Windows/macOS, `pnpm tauri build` produces the platform's native installer (`.msi` on
Windows via WiX, `.dmg`/`.app` on macOS) the same way.

## Verifying before committing

There's a small backend test suite (`src-tauri/src/**/test_*` modules, plus
`src-tauri/tests/`), and these practical checks:

```
cd src-tauri && cargo check --no-default-features    # backend compiles
cd src-tauri && RUSTFLAGS="-D warnings" cargo test --no-default-features
                                                       # backend tests pass, no warnings
cd .. && pnpm ng build --configuration development    # frontend compiles
```

The `RUSTFLAGS="-D warnings"` run is what CI (`.github/workflows/rustLint.yml`) enforces
on every push/PR - it turns plain rustc warnings (unused imports, unused variables, dead
code, ...) into build failures across every target, including `#[cfg(test)]` modules,
which a plain `cargo check`/`cargo build` never compiles at all.

These are quick and catch the vast majority of mistakes before you get to a full
`tauri build`, which is much slower.

### `src-tauri/tests/` - parser integration tests

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

### `src-tauri/benches/` - parser benchmarks

`cargo bench --no-default-features` runs `benches/m3u_parse.rs`, which benchmarks
`m3u::get_channel_from_lines` against the same real 2471-channel fixture, plus a synthetic
500,000-channel/20,000-category playlist for seeing how the parser scales well past
anything a real fixture reaches - useful before/after a parsing change to see whether it
actually helped. Needs `get_channel_from_lines` to be `pub` since benches compile as a
separate crate, same as `tests/`.

The synthetic playlist isn't checked in (~90MB) - the benchmark generates and caches it at
`tests/fixtures/synthetic_large_playlist.m3u8` on first run (a few seconds), and reuses
that file on every run after. Delete it to force a fresh one (e.g. after changing the
generator in `benches/m3u_parse.rs`).
