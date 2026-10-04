// The TV guide's time maths. Times are Unix timestamps in seconds.

import type { Epg } from 'react-native-iptelly';

export const HALF_HOUR = 30 * 60;
export const HOUR = 60 * 60;

// How far each guide fetch reaches. The window is quantised so scrolling
// through the guide only fetches again every few hours.
export const RANGE = 12 * HOUR;

// One block in a channel's row: a programme, or an hour of "No information"
// where the guide has nothing.
export type Block = {
  start: number;
  end: number;
  programme?: Epg;
};

export function floorHalfHour(time: number): number {
  return time - (time % HALF_HOUR);
}

// The start of the local hour, so "No information" blocks line up with the
// hours on the time bar even in time zones with half-hour offsets.
export function floorHour(time: number): number {
  const date = new Date(time * 1000);
  date.setMinutes(0, 0, 0);
  return Math.floor(date.getTime() / 1000);
}

export function rangeStart(windowStart: number): number {
  return windowStart - (windowStart % (RANGE / 2));
}

// The channel's row from `from` to `to`: its programmes, with the gaps
// between them filled by hour-long "No information" blocks.
export function blocks(programmes: Epg[], from: number, to: number): Block[] {
  const sorted = programmes
    .map(p => ({
      start: Number(p.startTimestamp),
      end: Number(p.endTimestamp),
      programme: p,
    }))
    .filter(p => p.end > from && p.start < to && p.end > p.start)
    .sort((a, b) => a.start - b.start);
  const result: Block[] = [];
  let cursor = floorHour(from);
  for (const block of sorted) {
    if (block.end <= cursor) {
      continue;
    }
    fill(result, cursor, block.start);
    result.push(block);
    cursor = block.end;
  }
  fill(result, cursor, to);
  return result;
}

function fill(result: Block[], from: number, to: number) {
  let start = from;
  while (start < to) {
    const end = Math.min(floorHour(start) + HOUR, to);
    result.push({ start, end });
    start = end;
  }
}

// The index of the block that's on at `time`, or the nearest one.
export function blockAt(row: Block[], time: number): number {
  if (row.length === 0) {
    return -1;
  }
  const index = row.findIndex(b => b.start <= time && time < b.end);
  if (index >= 0) {
    return index;
  }
  return time < row[0].start ? 0 : row.length - 1;
}

// Where the window has to move so `block` is in view. The window moves in
// half hours and never starts before the half hour that's on now.
export function scrollTo(
  block: Block,
  windowStart: number,
  windowLength: number,
  now: number,
): number {
  const earliest = floorHalfHour(now);
  let start = windowStart;
  while (block.start >= start + windowLength - HALF_HOUR) {
    start += HALF_HOUR;
  }
  while (block.start < start && start > earliest) {
    start -= HALF_HOUR;
  }
  return Math.max(start, earliest);
}

export function progress(block: Block, now: number): number {
  return Math.min(
    1,
    Math.max(0, (now - block.start) / (block.end - block.start)),
  );
}

export function formatTime(time: number): string {
  return new Date(time * 1000).toLocaleTimeString([], {
    hour: '2-digit',
    minute: '2-digit',
  });
}

export function formatDate(time: number): string {
  const date = new Date(time * 1000);
  return `${date.toLocaleDateString([], {
    weekday: 'short',
    month: 'short',
    day: 'numeric',
  })}, ${formatTime(time)}`;
}
