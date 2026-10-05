import type { Source } from 'react-native-iptelly';
import { countsText, movePlaylist, sortPlaylists } from '../src/playlists';

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
