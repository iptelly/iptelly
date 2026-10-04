const path = require('path');
const { getDefaultConfig } = require('@react-native/metro-config');
const { withMetroConfig } = require('react-native-monorepo-config');

const root = path.resolve(__dirname, '../..');

/**
 * Metro configuration
 * https://reactnative.dev/docs/metro
 *
 * Watches the whole repo so packages/react-native-iptelly is picked up from
 * source. The Rust folders are left out: target/ alone is far too big to
 * watch, and none of it is JavaScript.
 *
 * @type {import('@react-native/metro-config').MetroConfig}
 */
const config = withMetroConfig(getDefaultConfig(__dirname), {
  root,
  dirname: __dirname,
  conditions: ['react-native-iptelly-source'],
});

const rustFolders = ['target', 'crates', 'flatpak', '.git'].map(
  (folder) => new RegExp(`^${path.join(root, folder).replace(/[/\\.]/g, '\\$&')}[/\\\\]`)
);
config.resolver.blockList = [...config.resolver.blockList, ...rustFolders];

module.exports = config;
