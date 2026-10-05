import type { Channel } from 'react-native-iptelly';
import { resultRows } from '../src/Search';
import { withSearch } from '../src/searchStore';

jest.mock('@dr.pogodin/react-native-fs', () => ({}));
jest.mock('react-native-iptelly', () => ({}));

test('the history keeps the latest search first, without repeats', () => {
  expect(withSearch(['kung', 'babylon'], 'the wire')).toEqual([
    'the wire',
    'kung',
    'babylon',
  ]);
  expect(withSearch(['kung', 'Babylon'], ' babylon ')).toEqual([
    'babylon',
    'kung',
  ]);
  expect(withSearch(['kung'], '  ')).toEqual(['kung']);
});

test('the history holds the last 20 searches', () => {
  const history = Array.from({ length: 20 }, (_, i) => `search ${i}`);
  const next = withSearch(history, 'new');
  expect(next).toHaveLength(20);
  expect(next[0]).toBe('new');
  expect(next).not.toContain('search 19');
});

function channel(name: string, favorite = false): Channel {
  return { name, favorite, mediaType: 0, isAdult: false } as Channel;
}

test('result rows leave out empty ones and can put favourites first', () => {
  const results = {
    movies: [],
    series: [channel('Lord of Mysteries')],
    channels: [channel('One'), channel('Two', true)],
  };
  const rows = resultRows(results, true);
  expect(rows.map(r => r.title)).toEqual(['Series', 'Channels']);
  expect(rows[1].items.map(c => c.name)).toEqual(['Two', 'One']);
  expect(resultRows(results, false)[1].items.map(c => c.name)).toEqual([
    'One',
    'Two',
  ]);
});
