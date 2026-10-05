import type { ChannelList } from '../src/core';
import { groupRows } from '../src/playlistGroups';

jest.mock('@dr.pogodin/react-native-fs', () => ({}));
jest.mock('react-native-iptelly', () => ({}));

const fixed: ChannelList[] = [{ kind: 'all', name: 'All channels' }];
const group = (id: bigint, name: string): ChannelList => ({
  kind: 'group',
  name,
  groupId: id,
});
const playlists = [
  { sourceId: 1n, name: 'Home' },
  { sourceId: 2n, name: 'Sport' },
  { sourceId: 3n, name: 'Empty' },
];
const groups = new Map([
  ['1', [group(10n, 'News'), group(11n, 'Kids')]],
  ['2', [group(20n, 'Football')]],
  ['3', []],
]);

function names(rows: ReturnType<typeof groupRows>): string[] {
  return rows.map(r =>
    r.kind === 'playlist'
      ? `${r.expanded ? 'v' : '>'} ${r.name}`
      : `${r.indented ? '  ' : ''}${r.list.name}`,
  );
}

test('each playlist has its groups under its name', () => {
  expect(names(groupRows(fixed, playlists, groups, new Set()))).toEqual([
    'All channels',
    'v Home',
    '  News',
    '  Kids',
    'v Sport',
    '  Football',
  ]);
});

test('a collapsed playlist hides its groups', () => {
  expect(names(groupRows(fixed, playlists, groups, new Set(['1'])))).toEqual([
    'All channels',
    '> Home',
    'v Sport',
    '  Football',
  ]);
});

test('a playlist still loading shows just its name', () => {
  expect(names(groupRows(fixed, playlists, new Map(), new Set()))).toEqual([
    'All channels',
    'v Home',
    'v Sport',
    'v Empty',
  ]);
});
