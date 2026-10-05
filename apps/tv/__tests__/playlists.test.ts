import type { Group, Source } from 'react-native-iptelly';
import { stepRow } from '../src/components/SettingsPanel';
import {
  countsText,
  groupRows,
  movePlaylist,
  playlistRows,
  playlistsDue,
  serverText,
  sortPlaylists,
} from '../src/playlists';

jest.mock('@dr.pogodin/react-native-fs', () => ({}));
jest.mock('react-native-iptelly', () => ({}));
jest.mock('react-native-svg', () => ({}));

function source(id: bigint, name: string): Source {
  return { id, name, enabled: true, sourceType: 2 } as Source;
}

const trex = source(1n, 'trex');
const strong = source(2n, 'strong8k');
const demo = source(3n, 'Demo');
const all = [strong, trex, demo];
const names = (list: Source[]) => list.map(s => s.name);

test('playlists sort by name, by date added, or by hand', () => {
  expect(names(sortPlaylists(all, 'name', []))).toEqual([
    'Demo',
    'strong8k',
    'trex',
  ]);
  expect(names(sortPlaylists(all, 'added', []))).toEqual([
    'trex',
    'strong8k',
    'Demo',
  ]);
  // Playlists missing from the manual order go at the end, oldest first.
  expect(names(sortPlaylists(all, 'manual', ['3']))).toEqual([
    'Demo',
    'trex',
    'strong8k',
  ]);
});

test('reordering moves a playlist one place, within the list', () => {
  expect(names(movePlaylist(all, 0, 1))).toEqual(['trex', 'strong8k', 'Demo']);
  expect(names(movePlaylist(all, 0, -1))).toEqual(names(all));
});

test('counts read like TiviMate', () => {
  expect(
    countsText({ channels: 57020n, movies: 181108n, series: 48836n }),
  ).toBe('Channels: 57020, movies: 181108, series: 48836');
  expect(countsText(undefined)).toBe('Counting…');
});

test("only an Xtream playlist's page has Xtream Codes parameters", () => {
  expect(playlistRows(trex)).toContain('xtream');
  expect(playlistRows({ ...trex, sourceType: 1 })).not.toContain('xtream');
  expect(serverText('http://example.com:8080/player_api.php')).toBe(
    'http://example.com:8080',
  );
});

test('playlists are due an update after their interval, or on app start', () => {
  const now = 1_000_000;
  const updated = (hoursAgo: number) => BigInt(now - hoursAgo * 3600);
  const fresh = { ...trex, lastUpdated: updated(1) };
  const stale = { ...strong, lastUpdated: updated(25) };
  const unused = { ...demo, enabled: false, lastUpdated: updated(100) };
  const sources = [fresh, stale, unused];

  // 24 hours by default.
  expect(names(playlistsDue(sources, {}, now, false))).toEqual(['strong8k']);
  // Never, for strong8k, and on app start for trex.
  const updates = {
    '1': { hours: 24, onStart: true },
    '2': { hours: 0, onStart: false },
  };
  expect(names(playlistsDue(sources, updates, now, true))).toEqual(['trex']);
  expect(playlistsDue(sources, updates, now, false)).toEqual([]);
});

test('Manage groups lists groups under a heading for each kind', () => {
  const group = (name: string, mediaType: number) =>
    ({ name, mediaType } as Group);
  const rows = groupRows([group('Films', 1), group('News', 0)]);
  expect(
    rows.map(r => ('heading' in r ? `# ${r.heading}` : r.group.name)),
  ).toEqual(['# Channels', 'News', '# Movies', 'Films']);
});

test('Up and Down skip headings and stop at the ends', () => {
  const headings = [true, false, true, false];
  expect(stepRow(1, 1, 4, headings)).toBe(3);
  expect(stepRow(3, -1, 4, headings)).toBe(1);
  expect(stepRow(1, -1, 4, headings)).toBe(1);
  expect(stepRow(3, 1, 4, headings)).toBe(3);
});
