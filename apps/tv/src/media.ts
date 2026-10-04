// Helpers for the Movies and Series screens.

import type { MediaInfo } from 'react-native-iptelly';

// "1h 45m", "45m" or "2h".
export function formatDuration(seconds: number): string {
  const minutes = Math.round(seconds / 60);
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  if (hours === 0) {
    return `${rest}m`;
  }
  return rest === 0 ? `${hours}h` : `${hours}h ${rest}m`;
}

// "2025 • 1h 45m • Thriller", leaving out whatever the provider didn't give.
export function detailsLine(info: MediaInfo | undefined): string {
  return [
    info?.year,
    info?.durationSecs ? formatDuration(Number(info.durationSecs)) : undefined,
    info?.genre,
  ]
    .filter(Boolean)
    .join(' • ');
}

// A position in a video: "1:02:03", or "4:05" under an hour.
export function formatPosition(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const secs = String(total % 60).padStart(2, '0');
  return hours > 0
    ? `${hours}:${String(minutes).padStart(2, '0')}:${secs}`
    : `${minutes}:${secs}`;
}

export function formatRating(rating: number): string {
  return rating.toFixed(1);
}

// The poster the remote moves to in a grid `columns` wide, or undefined
// when the move leaves the grid: left off the first column, or up off the
// first row.
export function gridMove(
  index: number,
  key: 'up' | 'down' | 'left' | 'right',
  count: number,
  columns: number,
): number | undefined {
  const column = index % columns;
  switch (key) {
    case 'left':
      return column === 0 ? undefined : index - 1;
    case 'right':
      return column < columns - 1 && index + 1 < count ? index + 1 : index;
    case 'up':
      return index < columns ? undefined : index - columns;
    case 'down': {
      // Into the last row, even if it's short.
      const below = Math.min(index + columns, count - 1);
      return Math.floor(below / columns) > Math.floor(index / columns)
        ? below
        : index;
    }
  }
}
