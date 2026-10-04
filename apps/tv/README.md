# IPTelly TV

IPTelly for Android TV, built with React Native ([react-native-tvos](https://github.com/react-native-tvos/react-native-tvos)). It uses the same Rust core as the desktop app, through [`packages/react-native-iptelly`](../../packages/react-native-iptelly).

So far it only starts the core and lists the sources.

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
