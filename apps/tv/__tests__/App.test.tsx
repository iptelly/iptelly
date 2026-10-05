/**
 * @format
 */

import { TextInput } from 'react-native';
import ReactTestRenderer from 'react-test-renderer';
import App from '../App';
import type { Key } from '../src/remote';

// Jest has no native modules, so the Rust core, the file system, the video
// player and SVG are stand-ins here, and the test presses the remote's keys
// itself.
const mockRemote: { press?: (key: Key) => boolean | void } = {};

jest.mock('../src/remote', () => ({
  useRemote: (onKey: (key: Key) => boolean | void, enabled = true) => {
    if (enabled) {
      mockRemote.press = onKey;
    }
  },
}));

jest.mock('react-native-iptelly', () => {
  const channel = (id: bigint, name: string) => ({
    id,
    name,
    url: `http://example.com/${id}.m3u8`,
    mediaType: 0,
    sourceId: 1n,
    favorite: false,
    isAdult: false,
  });
  const channels = [channel(1n, 'Channel One'), channel(2n, 'Channel Two')];
  const movies = [
    { ...channel(21n, 'Film One'), mediaType: 1, rating: 6.5 },
    { ...channel(22n, 'Film Two'), mediaType: 1 },
  ];
  return {
    init: jest.fn(() => Promise.resolve()),
    getSources: jest.fn(() =>
      Promise.resolve([
        {
          id: 1n,
          name: 'My playlist',
          enabled: true,
          sourceType: 2,
          // Just updated, so it isn't due an update.
          lastUpdated: BigInt(Math.floor(Date.now() / 1000)),
        },
      ]),
    ),
    search: jest.fn(
      (filters: {
        viewType: number;
        groupId?: bigint;
        page: number;
        mediaTypes: ArrayBuffer;
      }) => {
        if (filters.page > 1 || filters.viewType === 3) {
          return Promise.resolve([]);
        }
        if (filters.viewType === 2 && filters.groupId == null) {
          return Promise.resolve([{ id: 10n, name: 'News', mediaType: 3 }]);
        }
        const mediaType = new Uint8Array(filters.mediaTypes)[0];
        return Promise.resolve(mediaType === 1 ? movies : channels);
      },
    ),
    getSourceCounts: jest.fn(() =>
      Promise.resolve({ channels: 2n, movies: 1n, series: 0n }),
    ),
    setSourceEnabled: jest.fn(() => Promise.resolve()),
    refreshSource: jest.fn(() => Promise.resolve()),
    getSourceGroups: jest.fn(() =>
      Promise.resolve([
        { id: 10n, name: 'News', mediaType: 0, hidden: false },
        { id: 11n, name: 'Films', mediaType: 1, hidden: true },
      ]),
    ),
    setGroupHidden: jest.fn(() => Promise.resolve()),
    updateSource: jest.fn(() => Promise.resolve()),
    getXtreamAccount: jest.fn(() =>
      Promise.resolve({ expires: undefined, maxConnections: 2 }),
    ),
    getMediaInfo: jest.fn(() =>
      Promise.resolve({ year: '2025', genre: 'Thriller', plot: 'A plot.' }),
    ),
    getGuide: jest.fn((list: unknown[]) => Promise.resolve(list.map(() => []))),
    playRequest: jest.fn((c: { name: string; url: string }) =>
      Promise.resolve({ title: c.name, urls: [c.url], volume: 50 }),
    ),
    addToHistory: jest.fn(() => Promise.resolve()),
  };
});

jest.mock('@dr.pogodin/react-native-fs', () => ({
  DocumentDirectoryPath: '/data',
  CachesDirectoryPath: '/cache',
  // Nothing saved yet.
  readFile: jest.fn(() => Promise.reject(new Error('no file'))),
  writeFile: jest.fn(() => Promise.resolve()),
}));

jest.mock('react-native-video', () => {
  const { View } = require('react-native');
  return (props: object) => <View testID="video" {...props} />;
});

jest.mock('react-native-svg', () => {
  const { View } = require('react-native');
  const Stub = () => <View />;
  return {
    __esModule: true,
    default: Stub,
    Path: Stub,
    Defs: Stub,
    LinearGradient: Stub,
    Rect: Stub,
    Stop: Stub,
  };
});

async function settle() {
  for (let i = 0; i < 5; i++) {
    await ReactTestRenderer.act(async () => {
      await Promise.resolve();
    });
  }
}

async function press(key: Key) {
  await ReactTestRenderer.act(async () => {
    mockRemote.press!(key);
  });
  await settle();
}

function text(renderer: ReactTestRenderer.ReactTestRenderer): string {
  return JSON.stringify(renderer.toJSON());
}

test('browses the groups and guide and plays a channel', async () => {
  const core = jest.requireMock('react-native-iptelly');
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    renderer = ReactTestRenderer.create(<App />);
  });
  await settle();

  expect(core.init).toHaveBeenCalledWith('/data', '/cache');
  expect(text(renderer)).toContain('All channels');
  expect(text(renderer)).toContain('News');
  expect(text(renderer)).toContain('Channel One');

  // Into the guide, where the channels have no programmes.
  await press('right');
  expect(text(renderer)).toContain('No information');
  expect(core.getGuide).toHaveBeenCalled();

  // OK plays the highlighted channel in the preview.
  await press('down');
  await press('select');
  expect(core.playRequest).toHaveBeenCalledWith(
    expect.objectContaining({ name: 'Channel Two' }),
  );
  const video = renderer.root.findByProps({ testID: 'video' });
  expect(video.props.source.uri).toBe('http://example.com/2.m3u8');
  expect(video.props.volume).toBe(0.5);
  expect(core.addToHistory).toHaveBeenCalledWith(2n);

  // OK again goes full screen, and up changes channel.
  await press('select');
  expect(text(renderer)).not.toContain('All channels');
  await press('up');
  expect(renderer.root.findByProps({ testID: 'video' }).props.source.uri).toBe(
    'http://example.com/1.m3u8',
  );

  // Back returns to the guide, then the groups, then the menu.
  await press('back');
  expect(text(renderer)).toContain('Channel Two');
  await press('back');
  await press('back');
  expect(text(renderer)).toContain('Settings');
  expect(text(renderer)).toContain('Favourites');

  await ReactTestRenderer.act(async () => renderer.unmount());
});

test("OK on a playlist's name collapses its groups", async () => {
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    renderer = ReactTestRenderer.create(<App />);
  });
  await settle();
  expect(text(renderer)).toContain('▾  ","My playlist');
  expect(text(renderer)).toContain('News');

  // From All channels down to the playlist's name.
  await press('down');
  await press('select');
  expect(text(renderer)).toContain('▸  ","My playlist');
  expect(text(renderer)).not.toContain('News');

  await press('select');
  expect(text(renderer)).toContain('News');

  await ReactTestRenderer.act(async () => renderer.unmount());
});

test('the Favourites screen has favourite channels, movies and series', async () => {
  const core = jest.requireMock('react-native-iptelly');
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    renderer = ReactTestRenderer.create(<App />);
  });
  await settle();

  // From the groups to the menu, then down to Favourites.
  await press('back');
  for (const key of ['down', 'down', 'down', 'select'] as const) {
    await press(key);
  }
  expect(text(renderer)).toContain('"Channels"');
  expect(text(renderer)).toContain('"Series"');
  expect(text(renderer)).toContain('Channel One');

  // Down to Movies, which shows after a moment.
  await press('down');
  await ReactTestRenderer.act(
    () => new Promise(resolve => setTimeout(resolve, 350)),
  );
  await settle();
  expect(text(renderer)).toContain('Film One');
  expect(core.search).toHaveBeenCalledWith(
    expect.objectContaining({ viewType: 1 }),
  );

  // A favourite movie plays full screen, and Back comes back here.
  await press('right');
  await press('select');
  expect(core.playRequest).toHaveBeenCalledWith(
    expect.objectContaining({ name: 'Film One' }),
  );
  await press('back');
  expect(text(renderer)).not.toContain('"display":"none"');
  expect(text(renderer)).toContain('Film One');

  await ReactTestRenderer.act(async () => renderer.unmount());
});

test('searches movies and channels and plays a result', async () => {
  const core = jest.requireMock('react-native-iptelly');
  const fs = jest.requireMock('@dr.pogodin/react-native-fs');
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    renderer = ReactTestRenderer.create(<App />);
  });
  await settle();

  // From the groups to the menu, then up to Search.
  await press('back');
  await press('up');
  await press('select');
  expect(text(renderer)).toContain('"Search"');

  // OK opens the keyboard; typing searches after a pause.
  await press('select');
  const box = renderer.root.findByType(TextInput);
  await ReactTestRenderer.act(async () => box.props.onChangeText('film'));
  await ReactTestRenderer.act(
    () => new Promise(resolve => setTimeout(resolve, 450)),
  );
  await settle();
  expect(core.search).toHaveBeenCalledWith(
    expect.objectContaining({ query: 'film' }),
  );
  expect(text(renderer)).toContain('"Movies"');
  expect(text(renderer)).toContain('Film One');
  expect(text(renderer)).toContain('"Channels"');

  // Closing the keyboard saves the search to the history.
  await ReactTestRenderer.act(async () => box.props.onBlur());
  expect(fs.writeFile).toHaveBeenCalledWith(
    '/data/search.json',
    expect.stringContaining('"film"'),
    'utf8',
  );

  // Down into the results and play the first movie; Back comes back.
  await press('down');
  await press('select');
  expect(core.playRequest).toHaveBeenCalledWith(
    expect.objectContaining({ name: 'Film One' }),
  );
  await press('back');
  expect(text(renderer)).toContain('Film One');
  expect(renderer.root.findAllByProps({ testID: 'video' })).toHaveLength(0);

  await ReactTestRenderer.act(async () => renderer.unmount());
});

test('Settings > General saves its switches, and can confirm exits', async () => {
  const fs = jest.requireMock('@dr.pogodin/react-native-fs');
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    renderer = ReactTestRenderer.create(<App />);
  });
  await settle();

  // From the groups to the menu, down to Settings, then General.
  await press('back');
  for (const key of ['down', 'down', 'down', 'down', 'select'] as const) {
    await press(key);
  }
  await press('select');
  expect(text(renderer)).toContain('Auto start app on boot');
  expect(text(renderer)).toContain('Not set');

  // Down to "Confirm exit by second press Back" and turn it on.
  for (const key of ['down', 'down', 'down', 'down', 'select'] as const) {
    await press(key);
  }
  expect(fs.writeFile).toHaveBeenCalledWith(
    '/data/settings.json',
    expect.stringContaining('"confirmExit":true'),
    'utf8',
  );

  // Back to the menu. With nothing playing, the first Back there asks for
  // a second, which leaves the app (returns false to Android).
  await press('back');
  await press('back');
  let handled: boolean | void = false;
  await ReactTestRenderer.act(async () => {
    handled = mockRemote.press!('back');
  });
  expect(handled).not.toBe(false);
  expect(text(renderer)).toContain('Press Back again to exit.');
  await ReactTestRenderer.act(async () => {
    handled = mockRemote.press!('back');
  });
  expect(handled).toBe(false);

  await ReactTestRenderer.act(async () => renderer.unmount());
});

test('Settings > Playlists shows each playlist with its counts', async () => {
  const core = jest.requireMock('react-native-iptelly');
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    renderer = ReactTestRenderer.create(<App />);
  });
  await settle();

  // From the groups to the menu, down to Settings, then Playlists.
  await press('back');
  for (const key of ['down', 'down', 'down', 'down', 'select'] as const) {
    await press(key);
  }
  await press('down');
  await press('select');
  expect(text(renderer)).toContain('My playlist');
  expect(text(renderer)).toContain('Channels: 2, movies: 1, series: 0');
  expect(text(renderer)).toContain('Update all playlists');
  expect(text(renderer)).toContain('By name');

  // OK on the playlist opens its page, where the first switch takes it
  // out of use.
  await press('select');
  expect(text(renderer)).toContain('Use this playlist');
  await press('select');
  expect(core.setSourceEnabled).toHaveBeenCalledWith(1n, false);

  await ReactTestRenderer.act(async () => renderer.unmount());
});

test("a playlist's page manages its groups and update interval", async () => {
  const core = jest.requireMock('react-native-iptelly');
  const fs = jest.requireMock('@dr.pogodin/react-native-fs');
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    renderer = ReactTestRenderer.create(<App />);
  });
  await settle();
  // Not due an update when the app started.
  expect(core.refreshSource).not.toHaveBeenCalled();

  // Settings, Playlists, then the playlist.
  await press('back');
  for (const key of ['down', 'down', 'down', 'down', 'select'] as const) {
    await press(key);
  }
  await press('down');
  await press('select');
  await press('select');
  expect(text(renderer)).toContain('Xtream Codes parameters');
  expect(text(renderer)).toContain('Update options');
  expect(text(renderer)).toContain('Update interval, hours');

  // Down past Catch-up, User-Agent and Xtream Codes parameters to Manage
  // groups, which has a page for each kind.
  for (const key of ['down', 'down', 'down', 'down', 'select'] as const) {
    await press(key);
  }
  expect(core.getSourceGroups).toHaveBeenCalledWith(1n);
  expect(text(renderer)).toContain('TV');
  expect(text(renderer)).toContain('1 of 1 shown');
  expect(text(renderer)).toContain('0 of 1 shown');

  // TV's page, where OK hides the highlighted group.
  await press('select');
  expect(text(renderer)).not.toContain('Films');
  await press('select');
  expect(core.setGroupHidden).toHaveBeenCalledWith(10n, true);

  // Back to Manage groups and the page, then down past the Update options
  // heading to the interval, and from 24 hours to 48.
  await press('back');
  expect(text(renderer)).toContain('0 of 1 shown');
  await press('back');
  await press('down');
  await press('select');
  await press('down');
  await press('select');
  expect(fs.writeFile).toHaveBeenCalledWith(
    '/data/settings.json',
    expect.stringContaining('"playlistUpdates":{"1":{"hours":48'),
    'utf8',
  );

  await ReactTestRenderer.act(async () => renderer.unmount());
});

test('browses the movies and plays one', async () => {
  const core = jest.requireMock('react-native-iptelly');
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    renderer = ReactTestRenderer.create(<App />);
  });
  await settle();

  // From the groups to the menu, then down to Movies.
  await press('back');
  await press('down');
  await press('select');
  expect(text(renderer)).toContain('All movies');
  expect(text(renderer)).toContain('Film One');
  expect(text(renderer)).toContain('6.5');

  // The highlighted movie's details come once it's been highlighted a
  // moment.
  await ReactTestRenderer.act(
    () => new Promise(resolve => setTimeout(resolve, 450)),
  );
  await settle();
  expect(core.getMediaInfo).toHaveBeenCalledWith(
    expect.objectContaining({ name: 'Film One' }),
  );
  expect(text(renderer)).toContain('2025 • Thriller');

  // Into the grid, across to the second film, and play it full screen.
  await press('right');
  await press('right');
  await press('select');
  expect(core.playRequest).toHaveBeenCalledWith(
    expect.objectContaining({ name: 'Film Two' }),
  );
  // The movies screen stays, hidden, so Back returns to the same place.
  expect(text(renderer)).toContain('"display":"none"');
  expect(text(renderer)).toContain('0:00');

  // OK pauses it; Back stops it and goes back to the movies.
  await press('select');
  expect(renderer.root.findByProps({ testID: 'video' }).props.paused).toBe(
    true,
  );
  await press('back');
  expect(text(renderer)).not.toContain('"display":"none"');
  expect(renderer.root.findAllByProps({ testID: 'video' })).toHaveLength(0);

  await ReactTestRenderer.act(async () => renderer.unmount());
});

test('Xtream Codes parameters wait for Apply changes', async () => {
  const core = jest.requireMock('react-native-iptelly');
  let renderer!: ReactTestRenderer.ReactTestRenderer;
  await ReactTestRenderer.act(async () => {
    renderer = ReactTestRenderer.create(<App />);
  });
  await settle();

  // Settings, Playlists, the playlist, then down past Use this playlist,
  // Catch-up and User-Agent to Xtream Codes parameters.
  await press('back');
  for (const key of ['down', 'down', 'down', 'down', 'select'] as const) {
    await press(key);
  }
  await press('down');
  await press('select');
  await press('select');
  for (const key of ['down', 'down', 'down', 'select'] as const) {
    await press(key);
  }
  expect(core.getXtreamAccount).toHaveBeenCalledWith(1n);
  expect(text(renderer)).toContain('MPEG-TS');
  expect(text(renderer)).toContain('Unlimited');
  expect(text(renderer)).toContain('Max connections');

  // Down to Output format and choose HLS.
  for (const key of ['down', 'down', 'down', 'select'] as const) {
    await press(key);
  }
  await press('down');
  await press('select');
  expect(text(renderer)).toContain('HLS');

  // Turn off Include VOD; nothing is saved until Apply changes.
  await press('down');
  await press('down');
  await press('select');
  expect(core.updateSource).not.toHaveBeenCalled();
  await press('down');
  await press('select');
  expect(core.updateSource).toHaveBeenCalledWith(
    expect.objectContaining({ outputFormat: 'm3u8', includeVod: false }),
  );
  expect(core.refreshSource).toHaveBeenCalledWith(1n);

  await ReactTestRenderer.act(async () => renderer.unmount());
});
