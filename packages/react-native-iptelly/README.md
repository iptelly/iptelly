# react-native-iptelly

IPTelly's Rust core ([`crates/iptelly-ffi`](../../crates/iptelly-ffi)) as a React Native turbo module, for the [TV app](../../apps/tv).

[uniffi-bindgen-react-native](https://jhugman.github.io/uniffi-bindgen-react-native/) generates most of this package:

- `src/generated/` holds the TypeScript bindings.
- `cpp/` holds the C++ that connects Hermes to Rust.
- `android/` holds the Android module.
- `src/index.tsx` is the entry point.

Don't edit these by hand. Change the Rust, then regenerate them:

```sh
yarn rust ubrn:android
```

That builds the Rust for each Android ABI (see `ubrn.config.yaml`) and copies the static libraries into `android/src/main/jniLibs`, which isn't committed. It then regenerates the bindings and runs `bob build` for codegen's output.

The API is described in [`crates/iptelly-ffi/README.md`](../../crates/iptelly-ffi/README.md). In TypeScript the names are camelCase, so `play_request` becomes `playRequest`, and 64-bit integers such as ids become `bigint`.
