// The playlists' order (Settings > Playlists > Playlists sorting and
// Reorder playlists), their counts, a playlist's own page, and when
// playlists update by themselves.

import type { Group, Source, SourceCounts } from 'react-native-iptelly';
import { MediaType, SourceType } from './core';

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

// A playlist's page, in TiviMate's order. 'updateOptions' is a heading.
export type PlaylistRow =
  | 'use'
  | 'catchup'
  | 'userAgent'
  | 'xtream'
  | 'groups'
  | 'updateOptions'
  | 'interval'
  | 'onStart'
  | 'update'
  | 'guide'
  | 'delete';

export const PLAYLIST_ROW_LABELS: Record<PlaylistRow, string> = {
  use: 'Use this playlist',
  catchup: 'Catch-up',
  userAgent: 'User-Agent',
  xtream: 'Xtream Codes parameters',
  groups: 'Manage groups',
  updateOptions: 'Update options',
  interval: 'Update interval, hours',
  onStart: 'Update on app start',
  update: 'Update playlist',
  guide: 'Update guide',
  delete: 'Delete playlist',
};

export function playlistRows(source: Source): PlaylistRow[] {
  const xtream = source.sourceType === SourceType.XTREAM;
  return [
    'use',
    'catchup',
    'userAgent',
    ...(xtream ? (['xtream'] as const) : []),
    'groups',
    'updateOptions',
    'interval',
    'onStart',
    'update',
    'guide',
    'delete',
  ];
}

// An Xtream playlist's server as it's typed, without the API path that
// update_source adds back.
export function serverText(url: string | undefined): string {
  return (url ?? '').replace(/\/player_api\.php$/, '');
}

// The catch-up the app can use for a playlist: the Xtream Codes archive.
export function catchupText(source: Source): string {
  return source.sourceType === SourceType.XTREAM
    ? 'Xtream Codes'
    : 'Not supported';
}

// How often a playlist updates by itself, and whether it does when the app
// starts. 0 hours is never.
export type PlaylistUpdates = { hours: number; onStart: boolean };
export const DEFAULT_UPDATES: PlaylistUpdates = { hours: 24, onStart: false };
export const UPDATE_HOURS = [0, 6, 12, 24, 48, 72];

export function hoursText(hours: number): string {
  return hours === 0 ? 'Never' : String(hours);
}

// The playlists in use that are due an update: those set to update on app
// start, when it's starting, and those last updated longer ago than their
// interval. `now` is in seconds.
export function playlistsDue(
  sources: Source[],
  updates: Record<string, PlaylistUpdates>,
  now: number,
  starting: boolean,
): Source[] {
  return sources.filter(source => {
    if (!source.enabled || source.sourceType === SourceType.CUSTOM) {
      return false;
    }
    const { hours, onStart } = updates[String(source.id)] ?? DEFAULT_UPDATES;
    if (starting && onStart) {
      return true;
    }
    if (hours === 0) {
      return false;
    }
    const last = Number(source.lastUpdated ?? 0n);
    return now - last >= hours * 3600;
  });
}

// Manage groups: a page for each kind of group, as in TiviMate. Groups
// without a kind, such as custom ones, go under Other when there are any.
export type GroupKind = { label: string; groups: Group[] };

const GROUP_KINDS: [number, string][] = [
  [MediaType.LIVESTREAM, 'TV'],
  [MediaType.MOVIE, 'Movies'],
  [MediaType.SERIE, 'Shows'],
];

export function groupKinds(groups: Group[]): GroupKind[] {
  const kinds = GROUP_KINDS.map(([mediaType, label]) => ({
    label,
    groups: groups.filter(g => g.mediaType === mediaType),
  }));
  const other = groups.filter(
    g => !GROUP_KINDS.some(([mediaType]) => g.mediaType === mediaType),
  );
  return other.length > 0
    ? [...kinds, { label: 'Other', groups: other }]
    : kinds;
}

// "12 of 40 shown", under each kind.
export function shownText(groups: Group[]): string {
  if (groups.length === 0) {
    return 'No groups';
  }
  return `${groups.filter(g => !g.hidden).length} of ${groups.length} shown`;
}
