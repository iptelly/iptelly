# IPTelly TV

IPTelly for Android TV, built with React Native ([react-native-tvos](https://github.com/react-native-tvos/react-native-tvos)). It uses the same Rust core as the desktop app, through [`packages/react-native-iptelly`](../../packages/react-native-iptelly).

The home screen is laid out like TiviMate's:

- **Menu:** a menu on the left, which shrinks to icons when you move right.
- **Groups:** the playlist's groups, with Favourites and All channels at the top.
- **Guide:** the guide for the chosen group, with the playing channel in a preview above it.

The remote works like this:

- **Moving:** Right moves into the next column and Left or Back goes back.
- **Guide:** moving right in the guide moves forward in time.
- **Playing:** OK plays a channel in the preview, and OK again makes it full screen. In full screen, Up and Down change channel.
- **Favourites:** holding OK on a channel adds it to Favourites, or removes it.

Movies and Series show the categories on the left and a grid of posters on the right, under the highlighted one's details:

- **Details:** the plot, cast, director, rating and backdrop come from the provider's Xtream account, so M3U playlists only have a title and poster.
- **Movies:** OK plays a movie full screen. OK pauses it, Left and Right move back and forward 10 seconds, and Back returns to the grid.
- **Series:** OK opens a series' seasons and episodes.

Settings, then Playlists, adds an Xtream account or an M3U link. Debug builds also offer a demo playlist, with made-up channels and a guide that play public test streams, and four open films from the Internet Archive.

The screens draw their own highlight instead of using Android's focus, so the guide can scroll in time. `src/remote.ts` turns the remote's keys into moves.

Search and most settings aren't done yet.

## What you need

- Node 20 or later. Yarn comes with the repo (`.yarn/releases`), so a global `yarn` of any version works.
- Rust, with the Android targets and cargo-ndk:
  ```sh
  rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android
  cargo install cargo-ndk
  ```
- The Android SDK with NDK 27.1.12297006, and JDK 17 or 21. Gradle 9.0 doesn't run on newer JDKs.

Set these in the shell you build from:

```sh
export ANDROID_HOME=~/Android/Sdk
export ANDROID_NDK_HOME=$ANDROID_HOME/ndk/27.1.12297006
export JAVA_HOME=/path/to/jdk-21
```

## Building and running

From the repo root:

```sh
yarn install
yarn rust ubrn:android   # builds crates/iptelly-ffi and generates the bindings
yarn tv start            # Metro, in its own terminal
yarn tv android          # builds the app and runs it on the emulator or TV
```

Run `yarn rust ubrn:android` again whenever `crates/iptelly-core` or `crates/iptelly-ffi` changes.

For a quicker build while developing on the emulator, add `--active-arch-only` to `yarn tv android`.

## Tests

```sh
yarn tv test
```
