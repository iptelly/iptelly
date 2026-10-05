// The home screen, laid out like TiviMate's: the menu, the groups and the
// guide side by side, with the playing channel in a preview above the
// guide. Focus moves left to right and each column makes room for the next.

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { Pressable, StyleSheet, Text, View } from 'react-native';
import Video, { type VideoRef } from 'react-native-video';
import { DownloadDirectoryPath } from '@dr.pogodin/react-native-fs';
import {
  addToHistory,
  deleteSource,
  exportAppData,
  getGuide,
  getSourceCounts,
  importAppData,
  playRequest,
  refreshAll,
  refreshEpg,
  refreshSource,
  setFavorite,
  setSourceEnabled,
  type Channel,
  type Epg,
  type PlayRequest,
  getSources,
  type Source,
  type SourceCounts,
} from 'react-native-iptelly';
import { canDrawOverlays, openOverlaySettings } from './appControl';
import {
  SORT_LABELS,
  countsText,
  movePlaylist,
  sortPlaylists,
  type PlaylistSort,
} from './playlists';
import {
  DEFAULT_SETTINGS,
  GENERAL_ITEMS,
  generalList,
  loadSettings,
  saveSettings,
  streamUrl,
  type GeneralItem,
  type GeneralSettings,
} from './appSettings';
import { AddPlaylist } from './components/AddPlaylist';
import { TextSetting } from './components/TextSetting';
import { Details } from './components/Details';
import {
  GUIDE_TOP,
  Guide,
  VISIBLE_ROWS,
  windowLength,
  type GuideRow,
} from './components/Guide';
import { GroupList } from './components/GroupList';
import { InfoBar } from './components/InfoBar';
import { MENU_ITEMS, MENU_WIDTH, Menu, RAIL_WIDTH } from './components/Menu';
import { PlayerBar } from './components/PlayerBar';
import { useFirstVisible } from './components/scroll';
import { SettingsList, SettingsPanel } from './components/SettingsPanel';
import {
  FIXED_LISTS,
  MediaType,
  PAGE_SIZE,
  channelKey,
  enabledSourceIds,
  errorMessage,
  lastWatched,
  listKey,
  loadChannels,
  ready,
  type ChannelList,
} from './core';
import { addDemoPlaylist } from './demo';
import { usePlaylistGroups } from './playlistGroups';
import {
  RANGE,
  blockAt,
  blocks,
  floorHalfHour,
  rangeStart,
  scrollTo,
  type Block,
} from './guide';
import { useRemote, type Key } from './remote';
import { colors, fonts, px } from './theme';
import { Search } from './Search';
import { Vod, type VodSection } from './Vod';

// 'vod' is the Movies, Series or Favourites screen and 'search' the search
// screen, which handle the remote themselves.
type Area =
  | 'menu'
  | 'groups'
  | 'guide'
  | 'settings'
  | 'form'
  | 'fullscreen'
  | 'vod'
  | 'search';

// How far Left and Right move through a movie.
const SEEK_SECONDS = 10;

// Where each column starts, in design pixels, for the column that has focus.
const LAYOUTS = {
  menu: { groups: MENU_WIDTH, groupsWidth: 490, right: MENU_WIDTH + 490 },
  groups: { groups: RAIL_WIDTH, groupsWidth: 450, right: RAIL_WIDTH + 450 },
  guide: { groups: 0, groupsWidth: 0, right: 0 },
};

const PREVIEW = { top: 40, left: 40, width: 640, height: 360 };

const CHANNELS = MENU_ITEMS.findIndex(i => i.key === 'channels');
const MOVIES = MENU_ITEMS.findIndex(i => i.key === 'movies');
const SEARCH = MENU_ITEMS.findIndex(i => i.key === 'search');
const VOD_MENU: Record<VodSection, number> = {
  movies: MOVIES,
  series: MENU_ITEMS.findIndex(i => i.key === 'series'),
  favourites: MENU_ITEMS.findIndex(i => i.key === 'favourites'),
};
const SETTINGS = MENU_ITEMS.findIndex(i => i.key === 'settings');

const SETTINGS_ITEMS = [
  'General',
  'Playlists',
  'EPG',
  'Appearance',
  'Playback',
  'Remote control',
  'Parental controls',
  'Other',
  'About',
];

// Where Settings > General > Back up data saves, and Restore data reads.
const BACKUP_PATH = `${DownloadDirectoryPath}/iptelly-backup.gz`;

// How long a first Back press waits for a second to exit, with "Confirm
// exit by second press Back" on.
const EXIT_CONFIRM_MS = 2000;

// The settings panel's page, and for a playlist's page, the playlist.
type SettingsPage = {
  page: 'root' | 'general' | 'playlists' | 'playlist' | 'sorting' | 'reorder';
  index: number;
  source?: Source;
  confirmDelete?: boolean;
  // Reorder playlists: the highlighted playlist is picked up to move.
  moving?: boolean;
};

// Settings > Playlists, under the playlists themselves.
const PLAYLIST_ACTIONS = [
  'Add playlist',
  'Update all playlists',
  'Playlists sorting',
  'Reorder playlists',
  ...(__DEV__ ? ['Add the demo playlist'] : []),
];
const SORTS: PlaylistSort[] = ['name', 'added', 'manual'];

type Playing = { channel: Channel; request: PlayRequest; number?: number };

function seconds(): number {
  return Math.floor(Date.now() / 1000);
}

// The current time, updated every few seconds.
function useNow(): number {
  const [now, setNow] = useState(seconds);
  useEffect(() => {
    const timer = setInterval(() => setNow(seconds()), 5000);
    return () => clearInterval(timer);
  }, []);
  return now;
}

// Pages through a list as the highlight nears its end.
type Paging = { page: number; done: boolean; loading: boolean };
const firstPage = (): Paging => ({ page: 1, done: false, loading: false });

export function Home() {
  const now = useNow();
  const [area, setArea] = useState<Area>('groups');
  const [menuIndex, setMenuIndex] = useState(CHANNELS);
  const [sources, setSources] = useState<Source[]>([]);
  const [sourceIds, setSourceIds] = useState<bigint[]>();
  // Bumped when playlists change, to load their groups again.
  const [groupsVersion, setGroupsVersion] = useState(0);
  const [groupIndex, setGroupIndex] = useState(1);
  // The list in the guide. It's kept by value, since collapsing a playlist
  // moves the rows below it.
  const [list, setList] = useState<ChannelList>(FIXED_LISTS[1]);
  const [channels, setChannels] = useState<Channel[]>([]);
  const [channelsLoading, setChannelsLoading] = useState(true);
  const channelPaging = useRef(firstPage());
  const [row, setRow] = useState(0);
  const [windowStart, setWindowStart] = useState(() =>
    floorHalfHour(seconds()),
  );
  const [focusTime, setFocusTime] = useState(seconds);
  const [guide, setGuide] = useState(() => new Map<string, Epg[]>());
  const guidePending = useRef(new Set<string>());
  const [playing, setPlaying] = useState<Playing>();
  const [info, setInfo] = useState(false);
  // Movies and episodes can be paused and moved through.
  const video = useRef<VideoRef>(null);
  const [paused, setPaused] = useState(false);
  const [position, setPosition] = useState(0);
  const [duration, setDuration] = useState(0);
  // Where Left and Right have moved to, until the player gets there.
  const [seekTo, setSeekTo] = useState<number>();
  const [vodSection, setVodSection] = useState<VodSection>();
  // A series chosen in search, for the Series screen to open.
  const [seriesToOpen, setSeriesToOpen] = useState<Channel>();
  // The search screen, kept once opened so its results stay.
  const [searchOpened, setSearchOpened] = useState(false);
  // The screen Back from full screen returns to, when what's playing was
  // started from Movies, Series, Favourites or search.
  const [returnTo, setReturnTo] = useState<'vod' | 'search'>();
  const [settings, setSettings] = useState<SettingsPage>({
    page: 'root',
    index: 0,
  });
  // Settings > General.
  const [general, setGeneral] = useState<GeneralSettings>(DEFAULT_SETTINGS);
  // The General setting being typed in the form, or none for Add playlist.
  const [textSetting, setTextSetting] =
    useState<Extract<GeneralItem, { kind: 'text' }>>();
  // When Back was first pressed to exit, with exits confirmed.
  const exitPressed = useRef(0);
  // Settings > Playlists: each playlist's channel, movie and series counts.
  const [counts, setCounts] = useState(() => new Map<string, SourceCounts>());
  const [toast, setToast] = useState<{ text: string; busy?: boolean }>();
  const busy = toast?.busy === true;

  // Shows a message at the bottom of the screen; '' takes it away.
  const say = useCallback((text: string, isBusy = false) => {
    setToast(text ? { text, busy: isBusy } : undefined);
  }, []);

  useEffect(() => {
    if (!toast || toast.busy) {
      return;
    }
    const timer = setTimeout(() => setToast(undefined), 4000);
    return () => clearTimeout(timer);
  }, [toast]);

  useEffect(() => {
    if (!info) {
      return;
    }
    const timer = setTimeout(() => setInfo(false), 5000);
    return () => clearTimeout(timer);
  }, [info, playing]);

  // Sources and groups

  const reload = useCallback(async () => {
    await ready();
    const all = await getSources();
    const ids = await enabledSourceIds();
    setSources(all);
    setSourceIds(ids);
    setGroupsVersion(v => v + 1);
    setGuide(new Map());
    return ids;
  }, []);

  // The playlists in Settings > Playlists' order, which the groups and
  // categories columns follow too.
  const ordered = useMemo(
    () => sortPlaylists(sources, general.playlistSort, general.playlistOrder),
    [sources, general.playlistSort, general.playlistOrder],
  );

  const groups = usePlaylistGroups(
    ordered,
    FIXED_LISTS,
    MediaType.LIVESTREAM,
    groupsVersion,
    say,
  );
  const openRow = groups.rows.findIndex(
    r => r.kind === 'list' && listKey(r.list) === listKey(list),
  );

  const play = useCallback(
    async (channel: Channel, number?: number) => {
      try {
        const request = await playRequest(channel);
        setPlaying({ channel, request, number });
        setPaused(false);
        setPosition(0);
        setDuration(0);
        setSeekTo(undefined);
        setInfo(true);
        if (channel.id != null) {
          addToHistory(channel.id).catch(() => {});
        }
      } catch (e) {
        say(errorMessage(e));
      }
    },
    [say],
  );

  // Holding Left or Right moves the target along; the player only seeks
  // once it stops moving.
  useEffect(() => {
    if (seekTo === undefined) {
      return;
    }
    const timer = setTimeout(() => {
      video.current?.seek(seekTo);
      setPosition(seekTo);
      setSeekTo(undefined);
    }, 500);
    return () => clearTimeout(timer);
  }, [seekTo]);

  const playVod = useCallback(
    async (item: Channel) => {
      await play(item);
      setReturnTo('vod');
      setArea('fullscreen');
    },
    [play],
  );

  const closeVod = useCallback(() => {
    setMenuIndex(vodSection ? VOD_MENU[vodSection] : MOVIES);
    setArea('menu');
  }, [vodSection]);

  const closeSearch = useCallback(() => {
    setMenuIndex(SEARCH);
    setArea('menu');
  }, []);

  // A search result: a series opens in Series, and movies and channels
  // play full screen. Back from a channel goes to the guide, unless the
  // "Stay on search screen" setting is on.
  const openFromSearch = useCallback(
    async (item: Channel, stayOnSearch: boolean) => {
      if (item.mediaType === MediaType.SERIE) {
        setVodSection('series');
        setSeriesToOpen(item);
        setArea('vod');
        return;
      }
      await play(item);
      const live = item.mediaType === MediaType.LIVESTREAM;
      setReturnTo(live && !stayOnSearch ? undefined : 'search');
      setArea('fullscreen');
    },
    [play],
  );

  // Back from full screen to the screen it was started from.
  const backFromPlayer = () => {
    setPlaying(undefined);
    setArea(returnTo ?? 'guide');
    setReturnTo(undefined);
  };

  useEffect(() => {
    (async () => {
      try {
        const saved = await loadSettings();
        setGeneral(saved);
        const ids = await reload();
        if (ids.length === 0) {
          setArea('menu');
          setMenuIndex(SETTINGS);
          say('Add a playlist in Settings, then Playlists.');
          return;
        }
        const last = saved.lastChannelOnStart
          ? await lastWatched(ids)
          : undefined;
        if (last) {
          await play(last);
          setArea('fullscreen');
        }
      } catch (e) {
        say(errorMessage(e));
      }
    })();
  }, [reload, play, say]);

  // Moving through the groups shows each one in the guide after a moment.
  const highlighted = groups.rows[groupIndex];
  useEffect(() => {
    if (
      area !== 'groups' ||
      highlighted?.kind !== 'list' ||
      listKey(highlighted.list) === listKey(list)
    ) {
      return;
    }
    const timer = setTimeout(() => setList(highlighted.list), 300);
    return () => clearTimeout(timer);
  }, [area, highlighted, list]);

  // Channels

  useEffect(() => {
    if (!sourceIds || !list) {
      return;
    }
    let current = true;
    channelPaging.current = { page: 1, done: false, loading: true };
    setChannelsLoading(true);
    loadChannels(list, sourceIds, 1)
      .then(first => {
        if (current) {
          channelPaging.current = {
            page: 1,
            done: first.length < PAGE_SIZE,
            loading: false,
          };
          setChannels(first);
          setRow(0);
        }
      })
      .catch(e => say(errorMessage(e)))
      .finally(() => current && setChannelsLoading(false));
    return () => {
      current = false;
    };
  }, [list, sourceIds, say]);

  useEffect(() => {
    const paging = channelPaging.current;
    if (
      !sourceIds ||
      !list ||
      paging.done ||
      paging.loading ||
      row < channels.length - 10
    ) {
      return;
    }
    paging.loading = true;
    loadChannels(list, sourceIds, paging.page + 1)
      .then(more => {
        paging.page += 1;
        paging.done = more.length < PAGE_SIZE;
        setChannels(c => [...c, ...more]);
      })
      .catch(e => say(errorMessage(e)))
      .finally(() => {
        paging.loading = false;
      });
  }, [row, channels.length, list, sourceIds, say]);

  // The guide

  const layout =
    LAYOUTS[
      area === 'settings' || area === 'form'
        ? 'menu'
        : area === 'fullscreen' || area === 'vod' || area === 'search'
        ? 'guide'
        : area
    ];
  const guideX = layout.right;
  const length = windowLength(guideX);
  const range = rangeStart(windowStart);
  const top = useFirstVisible(row, channels.length, VISIBLE_ROWS - 1);
  const visible = useMemo(
    () => channels.slice(top, top + VISIBLE_ROWS),
    [channels, top],
  );

  const guideKey = useCallback(
    (channel: Channel) => `${range}:${channelKey(channel)}`,
    [range],
  );

  useEffect(() => {
    const wanted = playing ? [...visible, playing.channel] : visible;
    const missing = wanted.filter(c => {
      const key = guideKey(c);
      return !guide.has(key) && !guidePending.current.has(key);
    });
    if (missing.length === 0) {
      return;
    }
    missing.forEach(c => guidePending.current.add(guideKey(c)));
    getGuide(missing, BigInt(range), BigInt(range + RANGE))
      .then(results =>
        setGuide(previous => {
          const next = new Map(previous);
          missing.forEach((c, i) => next.set(guideKey(c), results[i] ?? []));
          return next;
        }),
      )
      .catch(e => say(errorMessage(e)))
      .finally(() =>
        missing.forEach(c => guidePending.current.delete(guideKey(c))),
      );
  }, [visible, playing, guide, guideKey, range, say]);

  const blocksFor = useCallback(
    (channel: Channel): Block[] =>
      blocks(guide.get(guideKey(channel)) ?? [], range, range + RANGE),
    [guide, guideKey, range],
  );

  const rows: GuideRow[] = useMemo(
    () =>
      visible.map((channel, i) => ({
        channel,
        number: top + i + 1,
        blocks: blocksFor(channel),
      })),
    [visible, top, blocksFor],
  );

  const focusedChannel = channels[row];
  const focusedBlocks = focusedChannel ? blocksFor(focusedChannel) : [];
  const focusedBlock = focusedBlocks[blockAt(focusedBlocks, focusTime)];
  const playingKey = playing ? channelKey(playing.channel) : undefined;
  const playingVod =
    playing != null && playing.channel.mediaType !== MediaType.LIVESTREAM;
  const playingBlocks = playing ? blocksFor(playing.channel) : [];
  const playingNow = blockAt(playingBlocks, now);
  // Where the playing channel is in the open list, or -1.
  const playingRow = channels.findIndex(c => channelKey(c) === playingKey);

  const openGuide = () => {
    if (playingRow >= 0) {
      setRow(playingRow);
    }
    setFocusTime(now);
    setWindowStart(floorHalfHour(now));
    setArea('guide');
  };

  // Opens the guide on a list.
  const openList = (next: ChannelList) => {
    if (listKey(next) === listKey(list)) {
      openGuide();
      return;
    }
    setList(next);
    setRow(0);
    setFocusTime(now);
    setWindowStart(floorHalfHour(now));
    setArea('guide');
  };

  // Settings

  // The playlist a playlist's page is for, as it is now.
  const settingsSource = settings.source
    ? sources.find(s => s.id === settings.source!.id) ?? settings.source
    : undefined;

  // The settings page's rows, and their switches, second lines and checks.
  const settingsList = (): {
    items: string[];
    toggles?: (boolean | undefined)[];
    details?: (string | undefined)[];
    checks?: (boolean | undefined)[];
  } => {
    switch (settings.page) {
      case 'root':
        return { items: SETTINGS_ITEMS };
      case 'general':
        return {
          items: GENERAL_ITEMS.map(item => item.label),
          ...generalList(general),
        };
      case 'playlists':
        return {
          items: [...ordered.map(s => s.name), ...PLAYLIST_ACTIONS],
          checks: ordered.map(s => s.enabled),
          details: [
            ...ordered.map(s => countsText(counts.get(String(s.id)))),
            ...PLAYLIST_ACTIONS.map(action =>
              action === 'Playlists sorting'
                ? SORT_LABELS[general.playlistSort]
                : undefined,
            ),
          ],
        };
      case 'playlist':
        return {
          items: [
            'Use this playlist',
            'Update playlist',
            'Update guide',
            settings.confirmDelete
              ? 'Press OK again to delete'
              : 'Delete playlist',
          ],
          toggles: [settingsSource?.enabled ?? false],
        };
      case 'sorting':
        return {
          items: SORTS.map(sort => SORT_LABELS[sort]),
          checks: SORTS.map(sort => sort === general.playlistSort),
        };
      case 'reorder':
        return {
          items: ordered.map(s => s.name),
          details: ordered.map((_, i) =>
            settings.moving && i === settings.index
              ? 'Up and Down move it, OK puts it down'
              : undefined,
          ),
        };
    }
  };

  // Each playlist's counts, whenever the Playlists page is shown.
  const showingPlaylists = area === 'settings' && settings.page === 'playlists';
  useEffect(() => {
    if (!showingPlaylists) {
      return;
    }
    for (const source of sources) {
      if (source.id == null) {
        continue;
      }
      getSourceCounts(source.id)
        .then(found =>
          setCounts(previous =>
            new Map(previous).set(String(source.id), found),
          ),
        )
        .catch(() => {});
    }
  }, [showingPlaylists, sources]);

  const run = async (
    busyText: string,
    work: () => Promise<unknown>,
    doneText: string,
  ) => {
    say(busyText, true);
    try {
      await work();
      say(doneText);
    } catch (e) {
      say(errorMessage(e));
    }
  };

  const chooseSetting = () => {
    if (busy) {
      return;
    }
    const { items } = settingsList();
    const item = items[settings.index];
    if (settings.page === 'root') {
      if (item === 'General') {
        setSettings({ page: 'general', index: 0 });
      } else if (item === 'Playlists') {
        setSettings({ page: 'playlists', index: 0 });
      } else if (item === 'About') {
        say('IPTelly for TVs, using the IPTelly core.');
      } else {
        say(`${item} isn't in the TV app yet.`);
      }
    } else if (settings.page === 'general') {
      chooseGeneral(GENERAL_ITEMS[settings.index]);
    } else if (settings.page === 'playlists') {
      const source = ordered[settings.index];
      if (source) {
        setSettings({ page: 'playlist', index: 0, source });
      } else if (item === 'Add playlist') {
        setArea('form');
      } else if (item === 'Update all playlists') {
        run(
          'Updating all playlists…',
          async () => {
            await refreshAll();
            await reload();
            setCounts(new Map());
          },
          'Updated all playlists.',
        );
      } else if (item === 'Playlists sorting') {
        setSettings({
          page: 'sorting',
          index: SORTS.indexOf(general.playlistSort),
        });
      } else if (item === 'Reorder playlists') {
        setSettings({ page: 'reorder', index: 0 });
      } else {
        run(
          'Adding the demo playlist…',
          async () => {
            await addDemoPlaylist();
            await reload();
          },
          'Added the demo playlist.',
        );
      }
    } else if (settings.page === 'sorting') {
      const sort = SORTS[settings.index];
      updateGeneral({
        ...general,
        playlistSort: sort,
        // Manual starts from the order they're in now.
        playlistOrder:
          sort === 'manual' && general.playlistOrder.length === 0
            ? ordered.map(s => String(s.id))
            : general.playlistOrder,
      });
    } else if (settings.page === 'reorder') {
      setSettings({ ...settings, moving: !settings.moving });
    } else {
      const source = settingsSource!;
      const id = source.id!;
      if (settings.index === 0) {
        run(
          source.enabled ? `Hiding ${source.name}…` : `Showing ${source.name}…`,
          async () => {
            await setSourceEnabled(id, !source.enabled);
            await reload();
          },
          source.enabled
            ? `${source.name} isn't in use.`
            : `${source.name} is in use.`,
        );
      } else if (settings.index === 1) {
        run(
          `Updating ${source.name}…`,
          async () => {
            await refreshSource(id);
            await reload();
          },
          `Updated ${source.name}.`,
        );
      } else if (settings.index === 2) {
        run(
          `Loading the guide for ${source.name}…`,
          async () => {
            await refreshEpg(id);
            setGuide(new Map());
          },
          `Loaded the guide for ${source.name}.`,
        );
      } else if (!settings.confirmDelete) {
        setSettings({ ...settings, confirmDelete: true });
      } else {
        run(
          `Deleting ${source.name}…`,
          async () => {
            await deleteSource(id);
            await reload();
            setSettings({ page: 'playlists', index: 0 });
          },
          `Deleted ${source.name}.`,
        );
      }
    }
  };

  const updateGeneral = (next: GeneralSettings) => {
    setGeneral(next);
    saveSettings(next).catch(e => say(errorMessage(e)));
  };

  const chooseGeneral = async (item: GeneralItem) => {
    if (item.kind === 'text') {
      setTextSetting(item);
      setArea('form');
    } else if (item.kind === 'toggle') {
      const on = !general[item.key];
      updateGeneral({ ...general, [item.key]: on });
      // Starting by itself needs "display over other apps".
      const autoStart =
        item.key === 'autoStartOnBoot' || item.key === 'autoStartOnWake';
      if (on && autoStart && !(await canDrawOverlays())) {
        say(
          'Allow IPTelly to display over other apps, so it can start itself.',
        );
        if (!(await openOverlaySettings())) {
          say("This TV doesn't let apps start by themselves.");
        }
      }
    } else if (item.kind === 'backup') {
      run(
        'Backing up…',
        () => exportAppData(BACKUP_PATH),
        `Backed up to ${BACKUP_PATH}. It includes your playlists' logins.`,
      );
    } else {
      run(
        'Restoring…',
        async () => {
          await importAppData(BACKUP_PATH);
          // Loads the restored playlists, which brings back their
          // favourites and history.
          await refreshAll();
          await reload();
        },
        'Restored your playlists, favourites and settings.',
      );
    }
  };

  const textSettingDone = (value?: string) => {
    if (textSetting && value !== undefined) {
      updateGeneral({ ...general, [textSetting.key]: value });
    }
    setTextSetting(undefined);
    setArea('settings');
  };

  const playlistAdded = (source?: Source) => {
    setArea('settings');
    if (!source?.id) {
      return;
    }
    const id = source.id;
    run(
      `Loading ${source.name}…`,
      async () => {
        await reload();
        await refreshEpg(id);
        setGuide(new Map());
      },
      `Added ${source.name}.`,
    );
  };

  // The remote

  const clamp = (value: number, count: number) =>
    Math.max(0, Math.min(value, count - 1));

  const keys: Record<
    Exclude<Area, 'form' | 'vod' | 'search'>,
    (key: Key) => boolean | void
  > = {
    menu: key => {
      if (key === 'up' || key === 'down') {
        setMenuIndex(i =>
          clamp(i + (key === 'up' ? -1 : 1), MENU_ITEMS.length),
        );
      } else if (key === 'select' || key === 'right') {
        const item = MENU_ITEMS[menuIndex];
        if (item.key === 'channels') {
          setArea('groups');
        } else if (item.key === 'search') {
          setSearchOpened(true);
          setArea('search');
        } else if (
          item.key === 'movies' ||
          item.key === 'series' ||
          item.key === 'favourites'
        ) {
          setSeriesToOpen(undefined);
          setVodSection(item.key);
          setArea('vod');
        } else if (item.key === 'settings') {
          setSettings({ page: 'root', index: 0 });
          setArea('settings');
        } else {
          say(`${item.label} isn't in the TV app yet.`);
        }
      } else if (key === 'back') {
        if (playing) {
          setArea('fullscreen');
          return;
        }
        // Back here leaves the app, after a second press if that's set.
        if (!general.confirmExit) {
          return false;
        }
        const pressed = Date.now();
        if (pressed - exitPressed.current < EXIT_CONFIRM_MS) {
          return false;
        }
        exitPressed.current = pressed;
        say('Press Back again to exit.');
      }
    },
    groups: key => {
      const current = groups.rows[groupIndex];
      if (key === 'up' || key === 'down') {
        setGroupIndex(i =>
          clamp(i + (key === 'up' ? -1 : 1), groups.rows.length),
        );
      } else if (current?.kind === 'playlist' && key === 'select') {
        groups.toggle(current.sourceId);
      } else if (
        current?.kind === 'list' &&
        (key === 'right' || key === 'select')
      ) {
        openList(current.list);
      } else if (key === 'left' || key === 'back') {
        setMenuIndex(CHANNELS);
        setArea('menu');
      }
    },
    guide: key => {
      if (key === 'up' || key === 'down') {
        setRow(r => clamp(r + (key === 'up' ? -1 : 1), channels.length));
      } else if (key === 'right' && focusedBlock) {
        const next = focusedBlocks[focusedBlocks.indexOf(focusedBlock) + 1];
        if (next) {
          setFocusTime(next.start);
          setWindowStart(scrollTo(next, windowStart, length, now));
        }
      } else if (key === 'left') {
        const previous =
          focusedBlock &&
          focusedBlocks[focusedBlocks.indexOf(focusedBlock) - 1];
        if (!focusedBlock || focusedBlock.start <= now || !previous) {
          setGroupIndex(i => (openRow >= 0 ? openRow : i));
          setArea('groups');
        } else {
          setFocusTime(Math.max(previous.start, now));
          setWindowStart(scrollTo(previous, windowStart, length, now));
        }
      } else if (key === 'select' && focusedChannel) {
        if (channelKey(focusedChannel) === playingKey) {
          setArea('fullscreen');
          setInfo(true);
        } else {
          play(focusedChannel, row + 1);
        }
      } else if (key === 'longSelect' && focusedChannel?.id != null) {
        const favorite = !focusedChannel.favorite;
        setFavorite(focusedChannel.id, favorite)
          .then(() => {
            setChannels(c =>
              c.map(ch => (ch === focusedChannel ? { ...ch, favorite } : ch)),
            );
            say(favorite ? 'Added to Favourites.' : 'Removed from Favourites.');
          })
          .catch(e => say(errorMessage(e)));
      } else if (key === 'back') {
        setGroupIndex(i => (openRow >= 0 ? openRow : i));
        setArea('groups');
      }
    },
    settings: key => {
      const count = settingsList().items.length;
      const step = key === 'up' ? -1 : 1;
      if ((key === 'up' || key === 'down') && settings.moving) {
        // Reorder playlists: moves the picked-up playlist.
        const next = movePlaylist(ordered, settings.index, step);
        updateGeneral({
          ...general,
          playlistSort: 'manual',
          playlistOrder: next.map(s => String(s.id)),
        });
        setSettings(s => ({ ...s, index: clamp(s.index + step, count) }));
      } else if (key === 'up' || key === 'down') {
        setSettings(s => ({
          ...s,
          index: clamp(s.index + step, count),
          confirmDelete: false,
        }));
      } else if (key === 'select' || key === 'right') {
        chooseSetting();
      } else if (key === 'back' && settings.moving) {
        setSettings({ ...settings, moving: false });
      } else if (key === 'back' || key === 'left') {
        const playlistsAt = (action: string) =>
          ordered.length + PLAYLIST_ACTIONS.indexOf(action);
        if (settings.page === 'playlist') {
          setSettings({
            page: 'playlists',
            index: Math.max(0, ordered.indexOf(settingsSource!)),
          });
        } else if (settings.page === 'sorting') {
          setSettings({
            page: 'playlists',
            index: playlistsAt('Playlists sorting'),
          });
        } else if (settings.page === 'reorder') {
          setSettings({
            page: 'playlists',
            index: playlistsAt('Reorder playlists'),
          });
        } else if (settings.page === 'playlists') {
          setSettings({
            page: 'root',
            index: SETTINGS_ITEMS.indexOf('Playlists'),
          });
        } else if (settings.page === 'general') {
          setSettings({
            page: 'root',
            index: SETTINGS_ITEMS.indexOf('General'),
          });
        } else {
          setArea('menu');
        }
      }
    },
    fullscreen: key => {
      if (playingVod) {
        return vodPlayerKey(key);
      }
      // A channel from Favourites or search isn't in the guide's list.
      if (returnTo) {
        if (key === 'back' || key === 'left') {
          backFromPlayer();
        } else {
          setInfo(i => key !== 'info' || !i);
        }
        return;
      }
      const at = playingRow;
      if (
        key === 'up' ||
        key === 'down' ||
        key === 'channelUp' ||
        key === 'channelDown'
      ) {
        const step = key === 'up' || key === 'channelUp' ? -1 : 1;
        const next = channels[at + step];
        if (at >= 0 && next) {
          setRow(at + step);
          play(next, at + step + 1);
        } else {
          setInfo(true);
        }
      } else if (key === 'select') {
        if (info) {
          openGuide();
        } else {
          setInfo(true);
        }
      } else if (key === 'info') {
        setInfo(i => !i);
      } else if (key === 'back' || key === 'left') {
        openGuide();
      }
    },
  };

  // A full-screen movie or episode: OK pauses, Left and Right move back
  // and forward, and Back returns to the Movies or Series screen.
  const vodPlayerKey = (key: Key) => {
    if (key === 'select' || key === 'playPause') {
      setPaused(p => !p);
      setInfo(true);
    } else if (key === 'left' || key === 'right') {
      const from = seekTo ?? position;
      const step = key === 'left' ? -SEEK_SECONDS : SEEK_SECONDS;
      const end = duration > 0 ? duration : from + step;
      setSeekTo(Math.max(0, Math.min(from + step, end)));
      setInfo(true);
    } else if (key === 'back') {
      backFromPlayer();
    } else {
      setInfo(true);
    }
  };

  useRemote(
    key =>
      area === 'form' || area === 'vod' || area === 'search'
        ? true
        : keys[area](key),
    area !== 'form' && area !== 'vod' && area !== 'search',
  );

  // Drawing

  const fullscreen = area === 'fullscreen';
  const inVod = area === 'vod';
  const inSearch = area === 'search';
  // Search fills the screen, like TiviMate's.
  const menuShown = area !== 'guide' && !fullscreen && !inSearch;
  const previewStyle = [
    styles.video,
    fullscreen
      ? StyleSheet.absoluteFill
      : {
          position: 'absolute' as const,
          left: px(guideX + PREVIEW.left),
          top: px(PREVIEW.top),
          width: px(PREVIEW.width),
          height: px(PREVIEW.height),
        },
  ];
  const headers = playing
    ? Object.fromEntries(
        [
          // Settings > General's User-Agent, if the playlist has none.
          ['User-Agent', playing.request.userAgent || general.userAgent],
          ['Referer', playing.request.referrer],
          ['Origin', playing.request.origin],
        ].filter(([, value]) => value),
      )
    : {};

  return (
    <View style={styles.screen}>
      {/* The highlight is drawn by the app, but Android only sends the
          remote's keys while something has focus. */}
      {area !== 'form' && (
        <Pressable focusable hasTVPreferredFocus style={styles.focusAnchor} />
      )}
      {!fullscreen && !inVod && !inSearch && (
        <>
          <View
            style={[
              styles.previewFrame,
              {
                left: px(guideX + PREVIEW.left),
                top: px(PREVIEW.top),
                width: px(PREVIEW.width),
                height: px(PREVIEW.height),
              },
            ]}
          >
            {!playing && (
              <Text style={styles.hint}>Press OK on a channel to watch it</Text>
            )}
          </View>
          <Details
            x={guideX + PREVIEW.left + PREVIEW.width + 40}
            block={area === 'guide' ? focusedBlock : playingBlocks[playingNow]}
            channel={area === 'guide' ? focusedChannel : playing?.channel}
            group={area === 'guide' ? list?.name : undefined}
            now={now}
          />
          {channels.length > 0 ? (
            <Guide
              x={guideX}
              rows={rows}
              focusedRow={row}
              focusedBlock={focusedBlock}
              focused={area === 'guide'}
              playing={playingKey}
              windowStart={windowStart}
              now={now}
            />
          ) : (
            <Text style={[styles.empty, { left: px(guideX + 80) }]}>
              {channelsLoading || !sourceIds
                ? 'Loading…'
                : sourceIds.length === 0
                ? 'No playlists yet.'
                : list?.kind === 'favorites'
                ? 'No favourites yet. Hold OK on a channel to add it.'
                : 'No channels here.'}
            </Text>
          )}
        </>
      )}
      {sourceIds && vodSection && (
        <Vod
          key={vodSection}
          section={vodSection}
          sources={ordered}
          sourceIds={sourceIds}
          version={groupsVersion}
          active={inVod}
          onExit={closeVod}
          onPlay={playVod}
          seriesToOpen={seriesToOpen}
          say={say}
        />
      )}
      {sourceIds && searchOpened && (
        <Search
          sourceIds={sourceIds}
          active={inSearch}
          onExit={closeSearch}
          onOpen={openFromSearch}
          say={say}
        />
      )}
      {playing && !inVod && !inSearch && (
        <Video
          ref={video}
          key={channelKey(playing.channel)}
          source={{
            uri: streamUrl(playing.request.urls[0], general.udpProxy),
            headers,
          }}
          style={previewStyle}
          resizeMode="contain"
          paused={paused}
          // The Home button shrinks it into a corner, if that's set; the
          // whole screen goes there, so it's shown full screen.
          enterPictureInPictureOnLeave={general.pipOnHome}
          onPictureInPictureStatusChanged={({ isActive }) => {
            if (isActive) {
              setArea('fullscreen');
            }
          }}
          volume={(playing.request.volume ?? 100) / 100}
          onLoad={data => setDuration(data.duration)}
          onProgress={data => setPosition(data.currentTime)}
          onError={() => say(`Couldn't play ${playing.channel.name}.`)}
        />
      )}
      {fullscreen && playingVod && (info || paused || seekTo != null) && (
        <PlayerBar
          title={playing.channel.name}
          position={seekTo ?? position}
          duration={duration}
          paused={paused}
        />
      )}
      {fullscreen && info && playing && !playingVod && (
        <InfoBar
          channel={playing.channel}
          number={
            playing.number ?? (playingRow >= 0 ? playingRow + 1 : undefined)
          }
          current={playingBlocks[playingNow]}
          next={playingBlocks[playingNow + 1]}
          now={now}
        />
      )}
      {menuShown && (
        <View style={styles.columns}>
          <Menu
            expanded={area !== 'groups' && !inVod}
            focused={area === 'menu'}
            index={menuIndex}
            section={
              area === 'settings' || area === 'form'
                ? SETTINGS
                : inVod && vodSection
                ? VOD_MENU[vodSection]
                : CHANNELS
            }
          />
          {!inVod && (
            <GroupList
              rows={groups.rows}
              index={groupIndex}
              open={openRow}
              focused={area === 'groups'}
              width={layout.groupsWidth}
            />
          )}
        </View>
      )}
      {area === 'settings' && (
        <SettingsPanel
          title={
            {
              root: 'Settings',
              general: 'General',
              playlists: 'Playlists',
              playlist: settingsSource?.name ?? '',
              sorting: 'Playlists sorting',
              reorder: 'Reorder playlists',
            }[settings.page]
          }
        >
          <SettingsList
            {...settingsList()}
            index={settings.index}
            moving={settings.moving}
          />
        </SettingsPanel>
      )}
      {area === 'form' && textSetting && (
        <SettingsPanel title={textSetting.label}>
          <TextSetting
            value={general[textSetting.key]}
            placeholder={textSetting.placeholder}
            hint={textSetting.hint}
            onDone={textSettingDone}
          />
        </SettingsPanel>
      )}
      {area === 'form' && !textSetting && (
        <SettingsPanel title="Add playlist">
          <AddPlaylist onDone={playlistAdded} />
        </SettingsPanel>
      )}
      {toast && (
        <View style={styles.toast}>
          <Text style={styles.toastText}>{toast.text}</Text>
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    flex: 1,
    backgroundColor: colors.background,
  },
  focusAnchor: {
    position: 'absolute',
    width: 1,
    height: 1,
    opacity: 0,
  },
  // Black bars, not the app's blue, around video that doesn't fill the box.
  video: {
    backgroundColor: 'black',
  },
  columns: {
    position: 'absolute',
    top: 0,
    left: 0,
    bottom: 0,
    flexDirection: 'row',
  },
  previewFrame: {
    position: 'absolute',
    borderRadius: px(12),
    backgroundColor: colors.backgroundDark,
    alignItems: 'center',
    justifyContent: 'center',
  },
  hint: {
    color: colors.textDim,
    fontSize: fonts.small,
  },
  empty: {
    position: 'absolute',
    top: px(GUIDE_TOP + 100),
    color: colors.textDim,
    fontSize: fonts.normal,
  },
  toast: {
    position: 'absolute',
    bottom: px(50),
    alignSelf: 'center',
    maxWidth: px(1200),
    paddingVertical: px(20),
    paddingHorizontal: px(36),
    borderRadius: px(12),
    backgroundColor: 'rgba(20, 28, 40, 0.92)',
  },
  toastText: {
    color: colors.text,
    fontSize: fonts.normal,
  },
});
