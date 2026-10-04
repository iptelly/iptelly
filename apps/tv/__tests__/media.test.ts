import {
  detailsLine,
  formatDuration,
  formatPosition,
  formatRating,
  gridMove,
} from '../src/media';

test('durations, positions and ratings', () => {
  expect(formatDuration(6300)).toBe('1h 45m');
  expect(formatDuration(2700)).toBe('45m');
  expect(formatDuration(7200)).toBe('2h');
  expect(formatPosition(3723)).toBe('1:02:03');
  expect(formatPosition(245)).toBe('4:05');
  expect(formatRating(6.5)).toBe('6.5');
  expect(formatRating(8)).toBe('8.0');
});

test('the details line leaves out what the provider left out', () => {
  expect(
    detailsLine({ year: '2025', durationSecs: 6300n, genre: 'Thriller' }),
  ).toBe('2025 • 1h 45m • Thriller');
  expect(detailsLine({ genre: 'Drama' })).toBe('Drama');
  expect(detailsLine(undefined)).toBe('');
});

describe('gridMove', () => {
  // 12 posters, 5 to a row:
  //  0  1  2  3  4
  //  5  6  7  8  9
  // 10 11
  test('moves along rows and down columns', () => {
    expect(gridMove(0, 'right', 12, 5)).toBe(1);
    expect(gridMove(1, 'down', 12, 5)).toBe(6);
    expect(gridMove(6, 'up', 12, 5)).toBe(1);
    expect(gridMove(6, 'left', 12, 5)).toBe(5);
  });

  test('leaves the grid off the first column or row', () => {
    expect(gridMove(5, 'left', 12, 5)).toBeUndefined();
    expect(gridMove(3, 'up', 12, 5)).toBeUndefined();
  });

  test('stops at the right-hand and bottom edges', () => {
    expect(gridMove(4, 'right', 12, 5)).toBe(4);
    expect(gridMove(11, 'right', 12, 5)).toBe(11);
    expect(gridMove(10, 'down', 12, 5)).toBe(10);
  });

  test('moves down into a short last row', () => {
    expect(gridMove(8, 'down', 12, 5)).toBe(11);
  });
});
