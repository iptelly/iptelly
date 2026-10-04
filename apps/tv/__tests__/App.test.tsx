/**
 * @format
 */

import ReactTestRenderer from 'react-test-renderer';
import App from '../App';

// Jest has no native modules, so the Rust core and the file system are
// stand-ins here.
jest.mock('react-native-iptelly', () => ({
  init: jest.fn(() => Promise.resolve()),
  getSources: jest.fn(() => Promise.resolve([{ id: 1n, name: 'My channels' }])),
}));
jest.mock('@dr.pogodin/react-native-fs', () => ({
  DocumentDirectoryPath: '/data',
  CachesDirectoryPath: '/cache',
}));

test('starts the core and lists the sources', async () => {
  const { init } = jest.requireMock('react-native-iptelly');
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    renderer = ReactTestRenderer.create(<App />);
  });
  expect(init).toHaveBeenCalledWith('/data', '/cache');
  expect(JSON.stringify(renderer.toJSON())).toContain('My channels');
});
