// The playlists' order (Settings > Playlists > Playlists sorting and
// Reorder playlists) and their counts.

import type { Source, SourceCounts } from 'react-native-iptelly';

export type PlaylistSort = 'name' | 'added' | 'manual';

export const SORT_LABELS: Record<PlaylistSort, string> = {
  name: 'By name',
  added: 'By date added',
  manual: 'Manual',
};

// `order` holds playlist ids for manual sorting; playlists missing from it
// (added since) go at the end, oldest first.
export function sortPlaylists(
  sources: Source[],
  sort: PlaylistSort,
  order: string[],
): Source[] {
  const byAdded = [...sources].sort((a, b) =>
    Number((a.id ?? 0n) - (b.id ?? 0n)),
  );
  if (sort === 'name') {
    return byAdded.sort((a, b) =>
      a.name.localeCompare(b.name, undefined, { sensitivity: 'base' }),
    );
  }
  if (sort === 'manual') {
    const at = (s: Source) => {
      const i = order.indexOf(String(s.id));
      return i < 0 ? Number.MAX_SAFE_INTEGER : i;
    };
    return byAdded.sort((a, b) => at(a) - at(b));
  }
  return byAdded;
}

// Moves the playlist at `from` up or down one place.
export function movePlaylist(
  sources: Source[],
  from: number,
  step: -1 | 1,
): Source[] {
  const to = from + step;
  if (to < 0 || to >= sources.length) {
    return sources;
  }
  const moved = [...sources];
  [moved[from], moved[to]] = [moved[to], moved[from]];
  return moved;
}

export function countsText(counts: SourceCounts | undefined): string {
  if (!counts) {
    return 'Counting…';
  }
  return `Channels: ${counts.channels}, movies: ${counts.movies}, series: ${counts.series}`;
}
