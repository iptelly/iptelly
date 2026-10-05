import type { Epg } from 'react-native-iptelly';
import {
  HALF_HOUR,
  HOUR,
  blockAt,
  blocks,
  floorHalfHour,
  floorHour,
  progress,
  rangeStart,
  scrollTo,
} from '../src/guide';
import { nextFirstVisible } from '../src/components/scroll';
import { demoFiles } from '../src/demo';

// demo.ts writes files and adds playlists, which only demoFiles here avoids.
jest.mock('@dr.pogodin/react-native-fs', () => ({}));
jest.mock('react-native-iptelly', () => ({}));

// A local hour, so floorHour lines up with it in any time zone.
const NOON = floorHour(Date.UTC(2026, 0, 1, 12) / 1000);

function epg(title: string, start: number, end: number): Epg {
  return {
    epgId: title,
    title,
    description: '',
    startTime: '',
    startTimestamp: BigInt(start),
    endTime: '',
    endTimestamp: BigInt(end),
    hasArchive: false,
    nowPlaying: false,
  };
}

describe('blocks', () => {
  test('fills a channel without a guide with hour-long blocks', () => {
    const row = blocks([], NOON, NOON + 3 * HOUR);
    expect(row.map(b => [b.start - NOON, b.end - NOON])).toEqual([
      [0, HOUR],
      [HOUR, 2 * HOUR],
      [2 * HOUR, 3 * HOUR],
    ]);
    expect(row.every(b => b.programme == null)).toBe(true);
  });

  test('starts the first block on the hour', () => {
    const row = blocks([], NOON + HALF_HOUR, NOON + 2 * HOUR);
    expect(row[0].start).toBe(NOON);
  });

  test('fills the gaps between programmes up to the next hour', () => {
    const news = epg('News', NOON, NOON + HALF_HOUR);
    const film = epg('Film', NOON + 2 * HOUR, NOON + 4 * HOUR);
    const row = blocks([film, news], NOON, NOON + 4 * HOUR);
    expect(row.map(b => b.programme?.title ?? '-')).toEqual([
      'News',
      '-',
      '-',
      'Film',
    ]);
    expect(row[1]).toEqual({ start: NOON + HALF_HOUR, end: NOON + HOUR });
    expect(row[2]).toEqual({ start: NOON + HOUR, end: NOON + 2 * HOUR });
  });

  test('keeps a programme that started before the range', () => {
    const row = blocks(
      [epg('Late', NOON - HOUR, NOON + HOUR)],
      NOON,
      NOON + HOUR,
    );
    expect(row).toHaveLength(1);
    expect(row[0].start).toBe(NOON - HOUR);
  });

  test('skips programmes that overlap the one before', () => {
    const row = blocks(
      [epg('A', NOON, NOON + HOUR), epg('B', NOON + HALF_HOUR, NOON + HOUR)],
      NOON,
      NOON + HOUR,
    );
    expect(row.map(b => b.programme?.title)).toEqual(['A']);
  });

  test('skips programmes that start inside the one before but end later', () => {
    // Providers send these, and they'd share a start time with the first.
    const row = blocks(
      [epg('A', NOON, NOON + HOUR), epg('B', NOON, NOON + 2 * HOUR)],
      NOON,
      NOON + 2 * HOUR,
    );
    expect(row.map(b => b.programme?.title ?? '-')).toEqual(['A', '-']);
    expect(new Set(row.map(b => b.start)).size).toBe(row.length);
  });
});

test('blockAt finds the block on at a time, or the nearest', () => {
  const row = blocks([], NOON, NOON + 2 * HOUR);
  expect(blockAt(row, NOON + 10)).toBe(0);
  expect(blockAt(row, NOON + HOUR)).toBe(1);
  expect(blockAt(row, NOON - HOUR)).toBe(0);
  expect(blockAt(row, NOON + 5 * HOUR)).toBe(1);
  expect(blockAt([], NOON)).toBe(-1);
});

describe('scrollTo', () => {
  const now = NOON + 10 * 60;
  const length = 2 * HOUR;

  test('moves the window on so a later block is in view', () => {
    const block = { start: NOON + 2 * HOUR, end: NOON + 3 * HOUR };
    expect(scrollTo(block, NOON, length, now)).toBe(NOON + HOUR);
  });

  test("moves back, but not before now's half hour", () => {
    const block = { start: NOON - HOUR, end: NOON + HOUR };
    expect(scrollTo(block, NOON + HOUR, length, now)).toBe(NOON);
  });

  test("leaves the window where it is when it's already in view", () => {
    const block = { start: NOON + HALF_HOUR, end: NOON + HOUR };
    expect(scrollTo(block, NOON, length, now)).toBe(NOON);
  });
});

test('time helpers', () => {
  expect(floorHalfHour(NOON + 45 * 60)).toBe(NOON + HALF_HOUR);
  expect(rangeStart(NOON + 7 * HOUR) % (6 * HOUR)).toBe(0);
  expect(progress({ start: NOON, end: NOON + HOUR }, NOON + 15 * 60)).toBe(
    0.25,
  );
  expect(progress({ start: NOON, end: NOON + HOUR }, NOON - 1)).toBe(0);
});

test('lists only scroll when the highlight nears an edge', () => {
  // 10 visible rows of 50, keeping 2 rows from the edges.
  expect(nextFirstVisible(0, 7, 50, 10)).toBe(0);
  expect(nextFirstVisible(0, 8, 50, 10)).toBe(1);
  expect(nextFirstVisible(10, 13, 50, 10)).toBe(10);
  expect(nextFirstVisible(10, 11, 50, 10)).toBe(9);
  expect(nextFirstVisible(0, 49, 50, 10)).toBe(40);
  expect(nextFirstVisible(0, 3, 5, 10)).toBe(0);
});

test('the demo playlist has a guide for all but one channel per group', () => {
  const { m3u, xml } = demoFiles(NOON);
  const channels = m3u
    .split('\n')
    .filter(l => l.startsWith('#EXTINF') && !l.includes('Demo Movies'));
  expect(channels).toHaveLength(15);
  expect(channels.filter(l => l.includes('tvg-id'))).toHaveLength(11);
  expect(xml.match(/<channel /g)).toHaveLength(11);
  expect(xml).toContain('<programme channel="demo1.tv"');
});
