# Developing Fred TV

This document explains how to check out, build, and run Fred TV locally. It's aimed at
people contributing code, not end users - if you just want to use the app, see the
[README](README.md) for install links instead.

## Overview

Fred TV is a [Tauri v2](https://v2.tauri.app/) desktop app:

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
git clone https://github.com/Fredolx/open-tv.git
cd open-tv
```

If you're working from a fork, clone your fork instead and add `Fredolx/open-tv` as an
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

- **The SQLite database** lives at (platform data dir)`/dev.fredol.open-tv/db.sqlite` -
  e.g. `~/.local/share/dev.fredol.open-tv/db.sqlite` on Linux. Handy to inspect directly
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
itself is at `src-tauri/target/release/open_tv` if you just want to run it directly
without installing a package.

On Windows/macOS, `pnpm tauri build` produces the platform's native installer (`.msi` on
Windows via WiX, `.dmg`/`.app` on macOS) the same way.

## Verifying before committing

There's no dedicated test suite to run, so the practical checks are:

```
cd src-tauri && cargo check --no-default-features   # backend compiles
cd .. && pnpm ng build --configuration development   # frontend compiles
```

Both are quick and catch the vast majority of mistakes before you get to a full
`tauri build`, which is much slower.
