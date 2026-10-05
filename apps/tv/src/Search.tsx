// The search screen, like TiviMate's: a search box, the search history as
// pills, and rows of matching movies, series and channels. The cog opens
// the search settings on the right.

import { useEffect, useMemo, useRef, useState } from 'react';
import { Keyboard, StyleSheet, Text, TextInput, View } from 'react-native';
import { setFavorite, type Channel } from 'react-native-iptelly';
import { Icon } from './components/Icon';
import { PosterRow, rowHeight } from './components/PosterRow';
import type { Shape } from './components/PosterGrid';
import { SettingsList, SettingsPanel } from './components/SettingsPanel';
import { MediaType, errorMessage, searchByName } from './core';
import { useRemote, type Key } from './remote';
import {
  DEFAULT_STORE,
  loadSearchStore,
  saveSearchStore,
  withSearch,
  type SearchSettings,
  type SearchStore,
} from './searchStore';
import { colors, fonts, px } from './theme';

type Area = 'box' | 'cog' | 'bin' | 'history' | 'results' | 'settings';

type Results = { movies: Channel[]; series: Channel[]; channels: Channel[] };
const NO_RESULTS: Results = { movies: [], series: [], channels: [] };

type Row = { title: string; items: Channel[]; shape: Shape };

const SETTINGS: { key: keyof SearchSettings; label: string }[] = [
  { key: 'showHistory', label: 'Show search history' },
  { key: 'favouritesFirst', label: 'Show favourite channels first' },
  {
    key: 'stayOnSearch',
    label: 'Stay on search screen when switching channels',
  },
];

// Searching waits for a pause in typing, and at least this many letters.
const DEBOUNCE_MS = 400;
const MIN_LENGTH = 2;
const RESULTS_TOP = 220;

// The result rows, leaving out empty ones. Favourite channels can go first.
export function resultRows(results: Results, favouritesFirst: boolean): Row[] {
  const channels = favouritesFirst
    ? [
        ...results.channels.filter(c => c.favorite),
        ...results.channels.filter(c => !c.favorite),
      ]
    : results.channels;
  return [
    { title: 'Movies', items: results.movies, shape: 'smallPoster' as Shape },
    { title: 'Series', items: results.series, shape: 'smallPoster' as Shape },
    { title: 'Channels', items: channels, shape: 'smallLogo' as Shape },
  ].filter(row => row.items.length > 0);
}

export function Search({
  sourceIds,
  active,
  onExit,
  onOpen,
  say,
}: {
  sourceIds: bigint[];
  // Whether the screen has the remote. It stays mounted while something
  // plays, so the results are still there afterwards.
  active: boolean;
  onExit: () => void;
  // Plays a movie or channel, or opens a series. `stayOnSearch` is the
  // setting for where Back from a channel goes.
  onOpen: (item: Channel, stayOnSearch: boolean) => void;
  say: (text: string) => void;
}) {
  const [store, setStore] = useState<SearchStore>(DEFAULT_STORE);
  const [area, setArea] = useState<Area>('box');
  const [query, setQuery] = useState('');
  const [editing, setEditing] = useState(false);
  const input = useRef<TextInput>(null);
  const [results, setResults] = useState<Results>(NO_RESULTS);
  const [searching, setSearching] = useState(false);
  const [historyIndex, setHistoryIndex] = useState(0);
  const [rowIndex, setRowIndex] = useState(0);
  const [columns, setColumns] = useState([0, 0, 0]);
  const [settingsIndex, setSettingsIndex] = useState(0);

  useEffect(() => {
    loadSearchStore().then(setStore);
  }, []);

  const update = (next: SearchStore) => {
    setStore(next);
    saveSearchStore(next).catch(e => say(errorMessage(e)));
  };

  // Searches once typing pauses.
  const trimmed = query.trim();
  useEffect(() => {
    if (trimmed.length < MIN_LENGTH) {
      setResults(NO_RESULTS);
      return;
    }
    let current = true;
    const timer = setTimeout(() => {
      setSearching(true);
      Promise.all(
        [MediaType.MOVIE, MediaType.SERIE, MediaType.LIVESTREAM].map(type =>
          searchByName(sourceIds, trimmed, type),
        ),
      )
        .then(([movies, series, channels]) => {
          if (current) {
            setResults({ movies, series, channels });
            setRowIndex(0);
            setColumns([0, 0, 0]);
          }
        })
        .catch(e => say(errorMessage(e)))
        .finally(() => current && setSearching(false));
    }, DEBOUNCE_MS);
    return () => {
      current = false;
      clearTimeout(timer);
    };
  }, [trimmed, sourceIds, say]);

  const rows = useMemo(
    () => resultRows(results, store.settings.favouritesFirst),
    [results, store.settings.favouritesFirst],
  );
  const showingHistory =
    trimmed === '' && store.settings.showHistory && store.history.length > 0;

  // The keyboard: OK on the box makes it editable and focuses it, which
  // opens the keyboard; closing the keyboard leaves the box and saves the
  // search. The box is only editable then, so the remote's arrows can't
  // move Android's focus into it. Focusing waits a moment for it to become
  // editable.
  useEffect(() => {
    if (!editing) {
      return;
    }
    const timer = setTimeout(() => input.current?.focus(), 100);
    return () => clearTimeout(timer);
  }, [editing]);
  useEffect(() => {
    const hidden = Keyboard.addListener('keyboardDidHide', () => {
      input.current?.blur();
    });
    return () => hidden.remove();
  }, []);
  const stopEditing = () => {
    if (!editing) {
      return;
    }
    setEditing(false);
    if (trimmed) {
      update({ ...store, history: withSearch(store.history, trimmed) });
    }
  };

  const clamp = (value: number, count: number) =>
    Math.max(0, Math.min(value, count - 1));

  // Down from the box or the cog: into the results, or the history.
  const down = () => {
    if (trimmed && rows.length > 0) {
      setArea('results');
    } else if (showingHistory) {
      setArea('history');
    }
  };

  const toggleFavourite = (item: Channel) => {
    if (item.id == null) {
      return;
    }
    const favorite = !item.favorite;
    setFavorite(item.id, favorite)
      .then(() => {
        const swap = (list: Channel[]) =>
          list.map(c => (c === item ? { ...c, favorite } : c));
        setResults(r => ({
          movies: swap(r.movies),
          series: swap(r.series),
          channels: swap(r.channels),
        }));
        say(favorite ? 'Added to Favourites.' : 'Removed from Favourites.');
      })
      .catch(e => say(errorMessage(e)));
  };

  const keys: Record<Area, (key: Key) => boolean | void> = {
    box: key => {
      if (key === 'select') {
        setEditing(true);
      } else if (key === 'right') {
        setArea('cog');
      } else if (key === 'down') {
        down();
      } else if (key === 'left' || key === 'back') {
        onExit();
      }
    },
    cog: key => {
      if (key === 'select') {
        setSettingsIndex(0);
        setArea('settings');
      } else if (key === 'left') {
        setArea('box');
      } else if (key === 'down') {
        down();
      } else if (key === 'back') {
        onExit();
      }
    },
    bin: key => {
      if (key === 'select') {
        update({ ...store, history: [] });
        setArea('box');
      } else if (key === 'down' || key === 'left') {
        setArea(showingHistory ? 'history' : 'box');
      } else if (key === 'up' || key === 'back') {
        setArea('box');
      }
    },
    history: key => {
      const count = store.history.length;
      const i = historyIndex;
      if (key === 'up') {
        if (i < 2) {
          setArea(i === 1 ? 'bin' : 'box');
        } else {
          setHistoryIndex(i - 2);
        }
      } else if (key === 'down' && i + 2 < count) {
        setHistoryIndex(i + 2);
      } else if (key === 'left' && i % 2 === 1) {
        setHistoryIndex(i - 1);
      } else if (key === 'right' && i % 2 === 0 && i + 1 < count) {
        setHistoryIndex(i + 1);
      } else if (key === 'select') {
        setQuery(store.history[i] ?? '');
        setArea('results');
      } else if (key === 'longSelect') {
        // Holding OK forgets one search.
        const history = store.history.filter((_, at) => at !== i);
        update({ ...store, history });
        setHistoryIndex(clamp(i, history.length));
      } else if (key === 'back') {
        setArea('box');
      }
    },
    results: key => {
      const row = rows[rowIndex];
      if (!row) {
        if (key === 'up' || key === 'back') {
          setArea('box');
        }
        return;
      }
      const column = columns[rowIndex] ?? 0;
      const item = row.items[column];
      if (key === 'up') {
        if (rowIndex === 0) {
          setArea('box');
        } else {
          setRowIndex(rowIndex - 1);
        }
      } else if (key === 'down') {
        setRowIndex(clamp(rowIndex + 1, rows.length));
      } else if (key === 'left' || key === 'right') {
        const next = clamp(
          column + (key === 'left' ? -1 : 1),
          row.items.length,
        );
        setColumns(c => c.map((v, at) => (at === rowIndex ? next : v)));
      } else if (key === 'select' && item) {
        onOpen(item, store.settings.stayOnSearch);
      } else if (key === 'longSelect' && item) {
        toggleFavourite(item);
      } else if (key === 'back') {
        setArea('box');
      }
    },
    settings: key => {
      if (key === 'up' || key === 'down') {
        setSettingsIndex(i =>
          clamp(i + (key === 'up' ? -1 : 1), SETTINGS.length),
        );
      } else if (key === 'select') {
        const setting = SETTINGS[settingsIndex].key;
        update({
          ...store,
          settings: { ...store.settings, [setting]: !store.settings[setting] },
        });
      } else if (key === 'back' || key === 'left') {
        setArea('cog');
      }
    },
  };

  // The keyboard has the remote while it's open.
  // While typing the keyboard has the remote, but Back still reaches here
  // once it's closed, and leaves the box rather than the app.
  useRemote(key => {
    if (editing) {
      if (key === 'back') {
        input.current?.blur();
        stopEditing();
      }
      return true;
    }
    return keys[area](key);
  }, active);

  // The highlighted row goes to the top once past the first.
  const scroll = rows
    .slice(0, area === 'results' ? rowIndex : 0)
    .reduce((sum, row) => sum + rowHeight(row.shape), 0);

  return (
    <View style={[styles.screen, !active && styles.hidden]}>
      <View
        style={[styles.box, (area === 'box' || editing) && styles.boxFocused]}
      >
        <TextInput
          ref={input}
          value={query}
          onChangeText={setQuery}
          editable={editing}
          placeholder="Search"
          placeholderTextColor={
            area === 'box' || editing ? 'rgba(51, 68, 90, 0.6)' : colors.textDim
          }
          returnKeyType="search"
          onSubmitEditing={() => input.current?.blur()}
          onBlur={stopEditing}
          style={[
            styles.input,
            (area === 'box' || editing) && styles.inputFocused,
          ]}
        />
      </View>
      <View style={[styles.cog, area === 'cog' && styles.iconFocused]}>
        <Icon
          name="settings"
          size={px(48)}
          color={area === 'cog' ? colors.textDark : colors.text}
        />
      </View>

      {showingHistory && (
        <View style={styles.history}>
          <View style={styles.historyHeader}>
            <Text style={styles.historyTitle}>Search history</Text>
            <View style={[styles.bin, area === 'bin' && styles.iconFocused]}>
              <Icon
                name="delete"
                size={px(40)}
                color={area === 'bin' ? colors.textDark : colors.text}
              />
            </View>
          </View>
          <View style={styles.pills}>
            {store.history.map((text, i) => {
              const focused = area === 'history' && i === historyIndex;
              return (
                <View key={text} style={styles.pillCell}>
                  <View style={[styles.pill, focused && styles.pillFocused]}>
                    <Text
                      numberOfLines={1}
                      style={[styles.pillText, focused && styles.dark]}
                    >
                      {text}
                    </Text>
                  </View>
                </View>
              );
            })}
          </View>
        </View>
      )}

      {trimmed.length >= MIN_LENGTH && (
        <View style={styles.results}>
          <View style={{ transform: [{ translateY: -px(scroll) }] }}>
            {rows.map((row, i) => (
              <PosterRow
                key={row.title}
                title={row.title}
                items={row.items}
                index={columns[i] ?? 0}
                focused={area === 'results' && i === rowIndex}
                shape={row.shape}
              />
            ))}
          </View>
          {rows.length === 0 && (
            <Text style={styles.empty}>
              {searching ? 'Searching…' : `Nothing matches "${trimmed}".`}
            </Text>
          )}
        </View>
      )}

      {area === 'settings' && (
        <SettingsPanel title="Search">
          <SettingsList
            items={SETTINGS.map(s => s.label)}
            toggles={SETTINGS.map(s => store.settings[s.key])}
            index={settingsIndex}
          />
        </SettingsPanel>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  screen: {
    ...StyleSheet.absoluteFillObject,
    backgroundColor: colors.background,
  },
  hidden: {
    display: 'none',
  },
  box: {
    position: 'absolute',
    top: px(70),
    left: px(300),
    width: px(1000),
    height: px(90),
    borderRadius: px(14),
    justifyContent: 'center',
    paddingHorizontal: px(36),
    backgroundColor: colors.block,
  },
  boxFocused: {
    backgroundColor: colors.pill,
  },
  input: {
    color: colors.text,
    fontSize: fonts.large,
    padding: 0,
  },
  inputFocused: {
    color: colors.textDark,
  },
  cog: {
    position: 'absolute',
    top: px(85),
    right: px(80),
    padding: px(12),
    borderRadius: px(40),
  },
  iconFocused: {
    backgroundColor: colors.pill,
  },
  history: {
    position: 'absolute',
    top: px(230),
    left: px(300),
    width: px(1000),
  },
  historyHeader: {
    flexDirection: 'row',
    alignItems: 'center',
    justifyContent: 'space-between',
    marginBottom: px(20),
    paddingLeft: px(26),
  },
  historyTitle: {
    color: colors.text,
    fontSize: fonts.normal,
    fontWeight: 'bold',
  },
  bin: {
    padding: px(10),
    borderRadius: px(30),
  },
  pills: {
    flexDirection: 'row',
    flexWrap: 'wrap',
  },
  pillCell: {
    width: '50%',
    alignItems: 'flex-start',
    marginBottom: px(16),
  },
  pill: {
    paddingHorizontal: px(26),
    paddingVertical: px(10),
    borderRadius: px(30),
    backgroundColor: colors.block,
  },
  pillFocused: {
    backgroundColor: colors.pill,
  },
  pillText: {
    color: colors.text,
    fontSize: fonts.normal,
  },
  dark: {
    color: colors.textDark,
  },
  results: {
    position: 'absolute',
    top: px(RESULTS_TOP),
    left: px(60),
    right: 0,
    bottom: 0,
    overflow: 'hidden',
  },
  empty: {
    marginTop: px(40),
    marginLeft: px(240),
    color: colors.textDim,
    fontSize: fonts.normal,
  },
});
