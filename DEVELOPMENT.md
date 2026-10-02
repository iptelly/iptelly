# Developing IPTelly

This document explains how to check out, build, and run IPTelly locally. It's aimed at
people contributing code, not end users - if you just want to use the app, see the
[README](README.md) for install links instead.

## Overview

IPTelly is a [Tauri v2](https://v2.tauri.app/) desktop app:

- **Backend**: Rust, in `src-tauri/` (SQLite for storage, `mpv`/`vlc` spawned as external
  player processes).
- **Frontend**: Angular 22, in `src/` (renders inside the Tauri webview).

## Prerequisites

1. **Rust** - install via [rustup](https://rustup.rs/). The project targets edition 2024
   and Rust 1.91.1+ (`src-tauri/Cargo.toml`'s `rust-version`); `rustup` will pick up
   whatever `stable` resolves to, which is fine.
2. **Node.js 24** (or 22.22.3+) with npm (bundled with Node) - see [Package manager](#package-manager)
   below.
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
cd iptelly
```

If you're working from a fork, clone your fork instead and add this repo as an
`upstream` remote so you can keep up with changes there.

## Package manager

Use **npm**. CI and the release workflow (`.github/workflows/buildAndUpload.yml`) and
`tauri.conf.json`'s `beforeDevCommand` both run npm, which reads
`package-lock.json`. The commands below run the Tauri CLI through `npx`, which uses the
version pinned in `node_modules` (no global install needed).

```
npm install
```

The npm `@tauri-apps/*` packages must stay on the same major.minor version as the Rust
`tauri`/`tauri-plugin-*` crates in `src-tauri/Cargo.toml`. `tauri build` fails outright
(not just a warning) if they drift apart. If you bump one side, bump the other to match and
re-run `npm install` so `package-lock.json` is updated before committing.

For a clean install that matches CI exactly (e.g. after switching branches, or if
`node_modules` was created by another package manager), use `rm -rf node_modules && npm ci`.

## Running in dev mode

```
npx tauri dev
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
- **Quick frontend-only check**: `npx ng build --configuration development`.

## Building everything locally

A full production build (optimized Rust release binary + Angular production bundle,
packaged for your OS):

```
npx tauri build
```

On Linux, this builds `deb`, `rpm`, and `AppImage` bundles by default (per
`src-tauri/tauri.conf.json`'s `bundle.targets: "all"`). To build only specific bundle
types (faster, useful when iterating on packaging):

```
npx tauri build --bundles rpm       # just the RPM
npx tauri build --bundles deb       # just the .deb
npx tauri build --bundles appimage  # just the AppImage
npx tauri build --bundles deb,rpm   # what CI builds on Linux
```

Output lands in `src-tauri/target/release/bundle/<type>/`, e.g.
`src-tauri/target/release/bundle/rpm/IPTelly-0.0.0-1.x86_64.rpm`. The plain, unbundled
binary itself is at `src-tauri/target/release/iptelly` if you just want to run it directly
without installing a package.

To install the RPM you just built (Fedora; `ffmpeg` needs RPM Fusion enabled, since the
package depends on it):

```
sudo dnf install ./src-tauri/target/release/bundle/rpm/IPTelly-*.rpm
```

Use `dnf reinstall` instead to replace a build of the same version that's already installed.
Local builds are always version `0.0.0` (see [Versioning](#versioning)), so this is the
usual case once you've installed one.

### Versioning

The version in `package.json`, `src-tauri/Cargo.toml` and `src-tauri/tauri.conf.json` is
a `0.0.0` placeholder; don't bump it in a PR. The real version is entered when running the
**Build and Upload** workflow (Actions tab → Build and Upload → Run workflow), which
writes it into those three files on the build runner only. It must be plain
`MAJOR.MINOR.PATCH` (e.g. `2.1.0`), because the Windows MSI bundler rejects pre-release
suffixes. The workflow only runs from `main`. Once every platform builds, the workflow tags the commit it built as `v<version>`
(e.g. `v2.1.0`); a version whose tag already exists is rejected up front.

On Windows/macOS, `npx tauri build` produces the platform's native installer (`.msi` on
Windows via WiX, `.dmg`/`.app` on macOS) the same way.

## Verifying before committing

There's a small backend test suite (`src-tauri/src/**/test_*` modules, plus
`src-tauri/tests/`), and these practical checks:

```
cd src-tauri && cargo check --no-default-features    # backend compiles
cd src-tauri && RUSTFLAGS="-D warnings" cargo test --no-default-features
                                                       # backend tests pass, no warnings
cd .. && npx ng build --configuration development     # frontend compiles
npm run lint                                           # frontend lint + formatting
npx ng test --watch=false                              # frontend tests pass
npm audit --omit=dev --audit-level=high                # no high/critical advisories in shipped deps
```

CI's `audit` job (in `.github/workflows/frontendLint.yml`) runs the same `npm audit` and
fails on any high or critical advisory in the production dependencies. Dev dependencies
(the Angular build tooling) aren't checked; run `npm audit` without `--omit=dev` to see
those too.

The `RUSTFLAGS="-D warnings"` run is what CI (`.github/workflows/rustLint.yml`) enforces
on every PR - it turns plain rustc warnings (unused imports, unused variables, dead
code, ...) into build failures across every target, including `#[cfg(test)]` modules,
which a plain `cargo check`/`cargo build` never compiles at all.

These are quick and catch the vast majority of mistakes before you get to a full
`tauri build`, which is much slower.

### Frontend tests (Vitest)

Frontend specs (`src/**/*.spec.ts`) run on Vitest through Angular's
`@angular/build:unit-test` builder, in Node with jsdom (no browser needed).

```
npm test                     # watch mode
npx ng test --watch=false    # single run, as CI does
```

Two helpers in `src/testing/` keep specs short:

- `test-module.ts` exports `appTestModule`, which declares every app component
  (`APP_DECLARATIONS` from `app.module.ts`) along with the modules their templates use.
  Pass it to `TestBed.configureTestingModule(appTestModule)`, then set any required
  inputs with `fixture.componentRef.setInput(...)` before the first `detectChanges()`.
- `setup.ts` runs before every test. It mocks Tauri IPC with `@tauri-apps/api/mocks`,
  so `invoke()` returns empty results instead of failing, and stubs the browser
  observers jsdom lacks. Override a command in a single spec by calling `mockIPC` again.

CI's `test` job runs the single-run command on every PR.

### Frontend linting and formatting (Biome)

The frontend uses [Biome](https://biomejs.dev/) for both linting and formatting (there's
no ESLint or Prettier). Its config is `biome.json`, which covers the `.ts` and `.css`
files under `src/` and skips the vendored files in `src/assets/`.

```
npm run lint        # check formatting, import order and lint rules
npm run lint:fix    # apply formatting and safe fixes
```

CI (`.github/workflows/frontendLint.yml`) runs `npx biome ci` on every PR and fails
on any error-level finding. Install the Biome editor extension to format on save.

Biome doesn't lint Angular `.html` templates. Template type errors are still caught by
`ng build`, because `strictTemplates` is on in `tsconfig.json`.

Two Angular-specific settings in `biome.json` matter:

- `style/useImportType` is off. Turning it on would rewrite imports that are only used
  as constructor parameter types into `import type`, which TypeScript erases, so
  Angular's dependency injection breaks at runtime even though the build succeeds.
- `unsafeParameterDecoratorsEnabled` is on so Biome can parse constructor-parameter
  decorators such as `@Inject(...)`.

A few `suspicious/*` rules (`noDoubleEquals` and others) are temporarily set to `warn`
because existing code has a backlog of them. Fix those case by case (`==` to `===` can
change behaviour if a value arrives as a string), then switch each rule back to its
default once its backlog is cleared.

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
