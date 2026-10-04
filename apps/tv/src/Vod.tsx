// The Movies and Series screens, laid out like TiviMate's: the categories
// on the left, and a grid of posters on the right under the highlighted
// one's details. A series opens into its seasons and episodes.

import { useCallback, useEffect, useRef, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import {
  getMediaInfo,
  loadEpisodes,
  setFavorite,
  type Channel,
  type MediaInfo,
} from 'react-native-iptelly';
import { GroupList } from './components/GroupList';
import { Backdrop, MediaDetails } from './components/MediaDetails';
import { PosterGrid, SHAPES } from './components/PosterGrid';
import {
  MediaType,
  PAGE_SIZE,
  channelKey,
  errorMessage,
  loadChannels,
  loadGroups,
  loadSeasonEpisodes,
  loadSeasons,
  type ChannelList,
} from './core';
import { gridMove } from './media';
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

export function Vod({
  kind,
  sourceIds,
  active,
  onExit,
  onPlay,
  say,
}: {
  kind: typeof MediaType.MOVIE | typeof MediaType.SERIE;
  sourceIds: bigint[];
  // Whether the screen has the remote. It stays mounted while a movie
  // plays, so it's where it was when the movie ends.
  active: boolean;
  onExit: () => void;
  onPlay: (item: Channel) => void;
  say: (text: string, busy?: boolean) => void;
}) {
  const noun = kind === MediaType.MOVIE ? 'movies' : 'series';
  const [area, setArea] = useState<Area>('lists');
  const [lists, setLists] = useState<ChannelList[]>(() => [
    { kind: 'favorites', name: 'Favourites' },
    { kind: 'history', name: 'History' },
    { kind: 'all', name: `All ${noun}` },
  ]);
  const fixedLists = 3;
  const [listIndex, setListIndex] = useState(fixedLists - 1);
  const [openList, setOpenList] = useState(fixedLists - 1);
  const [itemIndex, setItemIndex] = useState(0);
  const [series, setSeries] = useState<Channel>();
  const [seasons, setSeasons] = useState<Channel[]>([]);
  const [seasonIndex, setSeasonIndex] = useState(0);
  const [openSeason, setOpenSeason] = useState(0);
  const [episodeIndex, setEpisodeIndex] = useState(0);
  const [info, setInfo] = useState(() => new Map<string, MediaInfo>());
  const infoPending = useRef(new Set<string>());

  // The categories, a page at a time.
  const groupPaging = useRef({ page: 0, done: false, loading: false });
  useEffect(() => {
    const paging = groupPaging.current;
    if (paging.done || paging.loading || listIndex < lists.length - 6) {
      return;
    }
    paging.loading = true;
    loadGroups(sourceIds, paging.page + 1, kind)
      .then(more => {
        paging.page += 1;
        paging.done = more.length < PAGE_SIZE;
        setLists(l => [...l, ...more]);
      })
      .catch(e => say(errorMessage(e)))
      .finally(() => {
        paging.loading = false;
      });
  }, [listIndex, lists.length, sourceIds, kind, say]);

  // Moving through the categories shows each one after a moment.
  useEffect(() => {
    if (area !== 'lists' || listIndex === openList) {
      return;
    }
    const timer = setTimeout(() => {
      setOpenList(listIndex);
      setItemIndex(0);
    }, 300);
    return () => clearTimeout(timer);
  }, [area, listIndex, openList]);

  const list = lists[openList];
  const loadItems = useCallback(
    (page: number) => loadChannels(list, sourceIds, page, kind),
    [list, sourceIds, kind],
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
    if (!shown || !shownKey || info.has(shownKey)) {
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
      if (key === 'up' || key === 'down') {
        setListIndex(i => clamp(i + (key === 'up' ? -1 : 1), lists.length));
      } else if (key === 'right' || key === 'select') {
        if (listIndex !== openList) {
          setOpenList(listIndex);
          setItemIndex(0);
        }
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
          SHAPES.poster.columns,
        );
        if (next !== undefined) {
          setItemIndex(next);
        } else if (key === 'left') {
          setListIndex(openList);
          setArea('lists');
        }
      } else if (key === 'select' && item) {
        if (kind === MediaType.SERIE) {
          openSeries(item);
        } else {
          onPlay(item);
        }
      } else if (key === 'longSelect' && item) {
        toggleFavourite(item);
      } else if (key === 'back') {
        setListIndex(openList);
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
  const columnItems: ChannelList[] = inSeries
    ? seasons.map(s => ({ kind: 'all', name: s.name }))
    : lists;
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
        groups={columnItems}
        index={inSeries ? seasonIndex : listIndex}
        open={inSeries ? openSeason : openList}
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
            shape={inSeries ? 'wide' : 'poster'}
            height={GRID_HEIGHT}
          />
        ) : (
          <Text style={styles.empty}>
            {gridLoading
              ? 'Loading…'
              : list?.kind === 'favorites'
              ? `No favourites yet. Hold OK on one to add it.`
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
