/**
 * The library builds its own native code (android/CMakeLists.txt, from
 * uniffi-bindgen-react-native), so the app only needs codegen's output.
 *
 * @type {import('@react-native-community/cli-types').UserDependencyConfig}
 */
module.exports = {
  dependency: {
    platforms: {
      android: {
        cmakeListsPath: 'generated/jni/CMakeLists.txt',
      },
    },
  },
};
