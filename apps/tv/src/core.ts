// The Rust core, and the helpers the screens use to page through it.

import {
  CachesDirectoryPath,
  DocumentDirectoryPath,
} from '@dr.pogodin/react-native-fs';
import {
  getSources,
  init,
  search,
  type Channel,
  type Filters,
} from 'react-native-iptelly';

// The core's constants, which uniffi can't export (see crates/iptelly-core).
export const MediaType = { LIVESTREAM: 0, MOVIE: 1, SERIE: 2, GROUP: 3 };
export const ViewType = { ALL: 0, FAVORITES: 1, CATEGORIES: 2, HISTORY: 3 };
export const SortType = { PROVIDER: 2 };
export const SourceType = { M3U: 0, M3U_LINK: 1, XTREAM: 2, CUSTOM: 3 };

// How many rows search() returns per page.
export const PAGE_SIZE = 36;

let started: Promise<void> | undefined;

// Starts the core the first time it's called. The core keeps its database
// in the data folder and its logs and downloaded playlists in the cache
// folder. Android gives every app its own.
export function ready(): Promise<void> {
  started ??= init(DocumentDirectoryPath, CachesDirectoryPath);
  return started;
}

export function errorMessage(error: unknown): string {
  const inner = (error as { inner?: { message?: string } })?.inner;
  if (inner?.message) {
    return inner.message;
  }
  return error instanceof Error ? error.message : String(error);
}

export async function enabledSourceIds(): Promise<bigint[]> {
  await ready();
  const sources = await getSources();
  return sources.filter(s => s.enabled && s.id != null).map(s => s.id!);
}

// A list in the groups column: Favourites, History, everything, or one of
// the playlists' groups. The same lists hold channels, movies or series.
export type ChannelList =
  | { kind: 'favorites'; name: string }
  | { kind: 'history'; name: string }
  | { kind: 'all'; name: string }
  | { kind: 'group'; name: string; groupId: bigint };

export const FIXED_LISTS: ChannelList[] = [
  { kind: 'favorites', name: 'Favourites' },
  { kind: 'all', name: 'All channels' },
];

function filters(
  sourceIds: bigint[],
  page: number,
  viewType: number,
  mediaType: number,
  groupId?: bigint,
): Filters {
  return {
    sourceIds,
    mediaTypes: new Uint8Array([mediaType]).buffer,
    viewType,
    page,
    groupId,
    useKeywords: false,
    sort: SortType.PROVIDER,
  };
}

export async function loadGroups(
  sourceIds: bigint[],
  page: number,
  mediaType = MediaType.LIVESTREAM,
): Promise<ChannelList[]> {
  const groups = await search(
    filters(sourceIds, page, ViewType.CATEGORIES, mediaType),
  );
  return groups
    .filter(g => g.id != null)
    .map(g => ({ kind: 'group', name: g.name, groupId: g.id! }));
}

export function loadChannels(
  list: ChannelList,
  sourceIds: bigint[],
  page: number,
  mediaType = MediaType.LIVESTREAM,
): Promise<Channel[]> {
  const viewType = {
    favorites: ViewType.FAVORITES,
    history: ViewType.HISTORY,
    all: ViewType.ALL,
    group: ViewType.CATEGORIES,
  }[list.kind];
  const groupId = list.kind === 'group' ? list.groupId : undefined;
  return search(filters(sourceIds, page, viewType, mediaType, groupId));
}

export async function lastWatched(
  sourceIds: bigint[],
): Promise<Channel | undefined> {
  const history = await search(
    filters(sourceIds, 1, ViewType.HISTORY, MediaType.LIVESTREAM),
  );
  return history[0];
}

// A series' seasons. Call loadEpisodes from react-native-iptelly first.
export function loadSeasons(series: Channel): Promise<Channel[]> {
  return search({
    ...filters([series.sourceId!], 1, ViewType.ALL, MediaType.SERIE),
    seriesId: BigInt(series.url!),
  });
}

export function loadSeasonEpisodes(
  series: Channel,
  season: Channel,
  page: number,
): Promise<Channel[]> {
  return search({
    ...filters([series.sourceId!], page, ViewType.ALL, MediaType.MOVIE),
    seriesId: BigInt(series.url!),
    season: season.id,
  });
}

export function channelKey(channel: Channel): string {
  return String(channel.id);
}
