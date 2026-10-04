# iptelly-ffi

`iptelly-core` for apps that aren't written in Rust, starting with the React Native TV app. [uniffi](https://mozilla.github.io/uniffi-rs/) generates the bindings from `src/lib.rs`, and [uniffi-bindgen-react-native](https://jhugman.github.io/uniffi-bindgen-react-native/) turns them into a React Native module.

uniffi-bindgen-react-native only works with the uniffi version it was built for. Update `uniffi` in `Cargo.toml` and the npm package together. Both are on 0.31.

## What the app calls

Every function is async and fails with `IptellyError`.

| Area | Functions |
|---|---|
| Startup | `init(data_dir, cache_dir)`. Call it first, with the app's data and cache folders. |
| Sources | `get_sources`, `source_name_exists`, `add_source`, `refresh_source`, `refresh_all`, `delete_source`, `set_source_enabled` |
| Browsing | `search(filters)`, `load_episodes(series)` (Xtream, before opening a series), `get_media_info(channel)` (a movie's or series' plot, cast, rating and backdrop, from Xtream), `set_favorite`, `add_to_history`, `remove_from_history` |
| Playback | `play_request(channel)` gives the URLs, headers and settings for the app's player |
| EPG | `get_guide(channels, start, end)`, `get_epg(channel, start, end)`, `get_epg_schedule(channel)`, `refresh_epg(source_id)` |
| Settings | `get_settings`, `update_settings`, `has_adult_pin`, `verify_adult_pin`, `lock_adult_content` |

uniffi can't export constants, so the app needs its own copy of the numeric codes in `iptelly-core`: `media_type`, `source_type`, `view_type` and `sort_type`.

Recording, Re-stream, downloads and the external players aren't exported. They start other programs, which a TV can't do.

## Building for Android

With the Android NDK installed, `cargo-ndk` builds the library for each Android ABI:

```sh
cargo install cargo-ndk
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 build --release -p iptelly-ffi
```

uniffi-bindgen-react-native normally runs this build for you when it generates the module.

Don't set `-C target-cpu=native` in `RUSTFLAGS` for these builds. When cross-compiling, "native" means the build machine's CPU, and rustc crashes.

## Tests

`cargo test -p iptelly-ffi` calls the API the way an app does, against a database in a temporary folder.
