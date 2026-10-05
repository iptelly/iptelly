// The Movies and Series screens, laid out like TiviMate's: the categories
// on the left, and a grid of posters on the right under the highlighted
// one's details. A series opens into its seasons and episodes.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import {
  getMediaInfo,
  loadEpisodes,
  setFavorite,
  type Channel,
  type MediaInfo,
  type Source,
} from 'react-native-iptelly';
import { GroupList } from './components/GroupList';
import { Backdrop, MediaDetails } from './components/MediaDetails';
import { PosterGrid, SHAPES, type Shape } from './components/PosterGrid';
import {
  MediaType,
  PAGE_SIZE,
  channelKey,
  errorMessage,
  listKey,
  listMediaType,
  loadChannels,
  loadSeasonEpisodes,
  loadSeasons,
  type ChannelList,
} from './core';
import { gridMove } from './media';
import { usePlaylistGroups, type GroupRow } from './playlistGroups';
import { useRemote, type Key } from './remote';
import { colors, fonts, px } from './theme';
import { RAIL_WIDTH } from './components/Menu';

const LISTS_WIDTH = 450;
const CONTENT_LEFT = RAIL_WIDTH + LISTS_WIDTH + 50;
const HEADING_TOP = 560;
const GRID_TOP = 620;
const GRID_HEIGHT = 1080 - GRID_TOP;
const BACKDROP_HEIGHT = 600;

type Area = 'lists' | 'grid' | 'seasons' | 'episodes';

// Pages through a list from the core, a page at a time as the highlight
// nears its end. Starts again whenever `load` changes.
function usePaged(
  load: ((page: number) => Promise<Channel[]>) | undefined,
  index: number,
  say: (text: string) => void,
) {
  const [items, setItems] = useState<Channel[]>([]);
  const [loading, setLoading] = useState(false);
  const paging = useRef({ page: 1, done: true, loading: false });

  useEffect(() => {
    setItems([]);
    if (!load) {
      return;
    }
    let current = true;
    paging.current = { page: 1, done: false, loading: true };
    setLoading(true);
    load(1)
      .then(first => {
        if (current) {
          paging.current = {
            page: 1,
            done: first.length < PAGE_SIZE,
            loading: false,
          };
          setItems(first);
        }
      })
      .catch(e => say(errorMessage(e)))
      .finally(() => current && setLoading(false));
    return () => {
      current = false;
    };
  }, [load, say]);

  useEffect(() => {
    const page = paging.current;
    if (!load || page.done || page.loading || index < items.length - 15) {
      return;
    }
    page.loading = true;
    load(page.page + 1)
      .then(more => {
        page.page += 1;
        page.done = more.length < PAGE_SIZE;
        setItems(i => [...i, ...more]);
      })
      .catch(e => say(errorMessage(e)))
      .finally(() => {
        page.loading = false;
      });
  }, [load, index, items.length, say]);

  return { items, setItems, loading };
}

export type VodSection = 'movies' | 'series' | 'favourites';

const NO_SOURCES: Source[] = [];

// The lists in each section's left column, before the playlists.
function sectionLists(section: VodSection): ChannelList[] {
  if (section === 'favourites') {
    return [
      { kind: 'favorites', name: 'Channels', mediaType: MediaType.LIVESTREAM },
      { kind: 'favorites', name: 'Movies', mediaType: MediaType.MOVIE },
      { kind: 'favorites', name: 'Series', mediaType: MediaType.SERIE },
    ];
  }
  return [
    { kind: 'favorites', name: 'Favourites' },
    { kind: 'history', name: 'History' },
    { kind: 'all', name: `All ${section}` },
  ];
}

export function Vod({
  section,
  sources,
  sourceIds,
  version,
  active,
  onExit,
  onPlay,
  seriesToOpen,
  say,
}: {
  // Movies, Series, or the Favourites screen, which has favourite
  // channels, movies and series instead of playlists.
  section: VodSection;
  // A series chosen in search, opened straight into its seasons.
  seriesToOpen?: Channel;
  sources: Source[];
  sourceIds: bigint[];
  // Bumped when playlists change, to load their categories again.
  version: number;
  // Whether the screen has the remote. It stays mounted while a movie
  // plays, so it's where it was when the movie ends.
  active: boolean;
  onExit: () => void;
  onPlay: (item: Channel) => void;
  say: (text: string, busy?: boolean) => void;
}) {
  const favourites = section === 'favourites';
  // The screen's media type; the Favourites screen's lists have their own.
  const kind = section === 'series' ? MediaType.SERIE : MediaType.MOVIE;
  const [area, setArea] = useState<Area>('lists');
  const fixedLists = useMemo(() => sectionLists(section), [section]);
  const groups = usePlaylistGroups(
    favourites ? NO_SOURCES : sources,
    fixedLists,
    kind,
    version,
    say,
  );
  const first = favourites ? 0 : fixedLists.length - 1;
  const [listIndex, setListIndex] = useState(first);
  // Kept by value, since collapsing a playlist moves the rows below it.
  const [list, setList] = useState(fixedLists[first]);
  const listType = listMediaType(list, kind);
  const gridShape: Shape =
    listType === MediaType.LIVESTREAM ? 'logo' : 'poster';
  const noun =
    listType === MediaType.LIVESTREAM
      ? 'channels'
      : listType === MediaType.SERIE
      ? 'series'
      : 'movies';
  // Favourites and History change as things are watched and favourited,
  // so they load again each time they're opened.
  const [reloads, setReloads] = useState(0);
  const openRow = groups.rows.findIndex(
    r => r.kind === 'list' && listKey(r.list) === listKey(list),
  );
  const [itemIndex, setItemIndex] = useState(0);
  const [series, setSeries] = useState<Channel>();
  const [seasons, setSeasons] = useState<Channel[]>([]);
  const [seasonIndex, setSeasonIndex] = useState(0);
  const [openSeason, setOpenSeason] = useState(0);
  const [episodeIndex, setEpisodeIndex] = useState(0);
  const [info, setInfo] = useState(() => new Map<string, MediaInfo>());
  const infoPending = useRef(new Set<string>());

  const openList = (next: ChannelList) => {
    if (listKey(next) !== listKey(list)) {
      setList(next);
      setItemIndex(0);
    } else if (next.kind === 'favorites' || next.kind === 'history') {
      setReloads(r => r + 1);
    }
  };

  // Coming back to the screen shows anything favourited or watched since.
  useEffect(() => {
    if (active) {
      setReloads(r => r + 1);
    }
  }, [active]);

  // Moving through the categories shows each one after a moment.
  const highlighted = groups.rows[listIndex];
  useEffect(() => {
    if (
      area !== 'lists' ||
      highlighted?.kind !== 'list' ||
      listKey(highlighted.list) === listKey(list)
    ) {
      return;
    }
    const timer = setTimeout(() => {
      setList(highlighted.list);
      setItemIndex(0);
    }, 300);
    return () => clearTimeout(timer);
  }, [area, highlighted, list]);

  const changing = list.kind === 'favorites' || list.kind === 'history';
  const loadItems = useCallback(
    (page: number) => loadChannels(list, sourceIds, page, kind),
    // `reloads` only matters for the lists that change.
    // eslint-disable-next-line react-hooks/exhaustive-deps
    [list, sourceIds, kind, changing ? reloads : 0],
  );
  const grid = usePaged(loadItems, itemIndex, say);
  const items = grid.items;

  const season = seasons[openSeason];
  const loadEpisodePage = useCallback(
    (page: number) => loadSeasonEpisodes(series!, season, page),
    [series, season],
  );
  const episodes = usePaged(
    series && season ? loadEpisodePage : undefined,
    episodeIndex,
    say,
  );

  // Moving through the seasons shows each one after a moment.
  useEffect(() => {
    if (area !== 'seasons' || seasonIndex === openSeason) {
      return;
    }
    const timer = setTimeout(() => {
      setOpenSeason(seasonIndex);
      setEpisodeIndex(0);
    }, 300);
    return () => clearTimeout(timer);
  }, [area, seasonIndex, openSeason]);

  // The details of whatever's highlighted, fetched once it's been
  // highlighted for a moment.
  const shown = series ?? items[area === 'grid' ? itemIndex : 0];
  const shownKey = shown ? channelKey(shown) : undefined;
  useEffect(() => {
    // Only movies and series have info pages.
    const hasInfo =
      shown?.mediaType === MediaType.MOVIE ||
      shown?.mediaType === MediaType.SERIE;
    if (!shown || !shownKey || !hasInfo || info.has(shownKey)) {
      return;
    }
    if (infoPending.current.has(shownKey)) {
      return;
    }
    const timer = setTimeout(() => {
      infoPending.current.add(shownKey);
      getMediaInfo(shown)
        .then(details =>
          setInfo(previous => new Map(previous).set(shownKey, details)),
        )
        .catch(() => {})
        .finally(() => infoPending.current.delete(shownKey));
    }, 400);
    return () => clearTimeout(timer);
  }, [shown, shownKey, info]);

  const openSeries = async (item: Channel) => {
    say('Loading the episodes…', true);
    try {
      await loadEpisodes(item);
      const found = await loadSeasons(item);
      if (found.length === 0) {
        say('No episodes yet.');
        return;
      }
      setSeries(item);
      setSeasons(found);
      setSeasonIndex(0);
      setOpenSeason(0);
      setEpisodeIndex(0);
      setArea('seasons');
      say(''); // Clears "Loading the episodes…".
    } catch (e) {
      say(errorMessage(e));
    }
  };

  useEffect(() => {
    if (seriesToOpen) {
      openSeries(seriesToOpen);
    }
    // Only when a new series is chosen.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [seriesToOpen]);

  const closeSeries = () => {
    setSeries(undefined);
    setSeasons([]);
    setArea('grid');
  };

  const toggleFavourite = (item: Channel) => {
    if (item.id == null) {
      return;
    }
    const favorite = !item.favorite;
    setFavorite(item.id, favorite)
      .then(() => {
        grid.setItems(all =>
          all.map(i => (i === item ? { ...i, favorite } : i)),
        );
        say(favorite ? 'Added to Favourites.' : 'Removed from Favourites.');
      })
      .catch(e => say(errorMessage(e)));
  };

  const clamp = (value: number, count: number) =>
    Math.max(0, Math.min(value, count - 1));

  const keys: Record<Area, (key: Key) => boolean | void> = {
    lists: key => {
      const current = groups.rows[listIndex];
      if (key === 'up' || key === 'down') {
        setListIndex(i =>
          clamp(i + (key === 'up' ? -1 : 1), groups.rows.length),
        );
      } else if (current?.kind === 'playlist' && key === 'select') {
        groups.toggle(current.sourceId);
      } else if (
        current?.kind === 'list' &&
        (key === 'right' || key === 'select')
      ) {
        openList(current.list);
        setArea('grid');
      } else if (key === 'left' || key === 'back') {
        onExit();
      }
    },
    grid: key => {
      const item = items[itemIndex];
      if (key === 'up' || key === 'down' || key === 'left' || key === 'right') {
        const next = gridMove(
          itemIndex,
          key,
          items.length,
          SHAPES[gridShape].columns,
        );
        if (next !== undefined) {
          setItemIndex(next);
        } else if (key === 'left') {
          setListIndex(i => (openRow >= 0 ? openRow : i));
          setArea('lists');
        }
      } else if (key === 'select' && item) {
        if (item.mediaType === MediaType.SERIE) {
          openSeries(item);
        } else {
          onPlay(item);
        }
      } else if (key === 'longSelect' && item) {
        toggleFavourite(item);
      } else if (key === 'back') {
        setListIndex(i => (openRow >= 0 ? openRow : i));
        setArea('lists');
      }
    },
    seasons: key => {
      if (key === 'up' || key === 'down') {
        setSeasonIndex(i => clamp(i + (key === 'up' ? -1 : 1), seasons.length));
      } else if (key === 'right' || key === 'select') {
        if (seasonIndex !== openSeason) {
          setOpenSeason(seasonIndex);
          setEpisodeIndex(0);
        }
        setArea('episodes');
      } else if (key === 'left' || key === 'back') {
        closeSeries();
      }
    },
    episodes: key => {
      const episode = episodes.items[episodeIndex];
      if (key === 'up' || key === 'down' || key === 'left' || key === 'right') {
        const next = gridMove(
          episodeIndex,
          key,
          episodes.items.length,
          SHAPES.wide.columns,
        );
        if (next !== undefined) {
          setEpisodeIndex(next);
        } else if (key === 'left') {
          setSeasonIndex(openSeason);
          setArea('seasons');
        }
      } else if (key === 'select' && episode) {
        onPlay(episode);
      } else if (key === 'back') {
        setSeasonIndex(openSeason);
        setArea('seasons');
      }
    },
  };

  useRemote(key => keys[area](key), active);

  const shownInfo = shownKey ? info.get(shownKey) : undefined;
  const inSeries = series != null;
  const columnRows: GroupRow[] = inSeries
    ? seasons.map(s => ({
        kind: 'list',
        list: { kind: 'all', name: s.name },
        indented: false,
      }))
    : groups.rows;
  const gridItems = inSeries ? episodes.items : items;
  const gridLoading = inSeries ? episodes.loading : grid.loading;
  const heading = inSeries ? season?.name : list?.name;

  return (
    <View style={[styles.screen, !active && styles.hidden]}>
      <Backdrop
        uri={shownInfo?.backdrop}
        left={LISTS_WIDTH}
        height={BACKDROP_HEIGHT}
      />
      <GroupList
        rows={columnRows}
        index={inSeries ? seasonIndex : listIndex}
        open={inSeries ? openSeason : openRow}
        focused={area === 'lists' || area === 'seasons'}
        width={LISTS_WIDTH}
      />
      <View style={styles.details}>
        <MediaDetails item={shown} info={shownInfo} />
      </View>
      <Text numberOfLines={1} style={styles.heading}>
        {heading}
      </Text>
      <View style={styles.grid}>
        {gridItems.length > 0 ? (
          <PosterGrid
            items={gridItems}
            index={inSeries ? episodeIndex : itemIndex}
            focused={area === 'grid' || area === 'episodes'}
            shape={inSeries ? 'wide' : gridShape}
            height={GRID_HEIGHT}
          />
        ) : (
          <Text style={styles.empty}>
            {gridLoading
              ? 'Loading…'
              : list?.kind === 'favorites'
              ? `No favourite ${noun} yet. Hold OK on one to add it.`
              : list?.kind === 'history'
              ? `Nothing watched yet.`
              : `No ${noun} here.`}
          </Text>
        )}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    position: 'absolute',
    top: 0,
    bottom: 0,
    left: px(RAIL_WIDTH),
    right: 0,
    flexDirection: 'row',
    backgroundColor: colors.background,
  },
  hidden: {
    display: 'none',
  },
  details: {
    position: 'absolute',
    top: px(60),
    left: px(CONTENT_LEFT - RAIL_WIDTH),
    right: px(300),
  },
  heading: {
    position: 'absolute',
    top: px(HEADING_TOP),
    left: px(CONTENT_LEFT - RAIL_WIDTH),
    right: px(40),
    color: colors.text,
    fontSize: fonts.large,
    fontWeight: 'bold',
  },
  grid: {
    position: 'absolute',
    top: px(GRID_TOP),
    left: px(CONTENT_LEFT - RAIL_WIDTH),
    right: 0,
  },
  empty: {
    marginTop: px(40),
    color: colors.textDim,
    fontSize: fonts.normal,
  },
});
