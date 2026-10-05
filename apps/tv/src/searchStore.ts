// The search screen's history and settings, kept in a small file in the
// app's own storage so they survive restarts.

import {
  DocumentDirectoryPath,
  readFile,
  writeFile,
} from '@dr.pogodin/react-native-fs';

export type SearchSettings = {
  showHistory: boolean;
  favouritesFirst: boolean;
  // Back from a channel played from search returns to search, rather than
  // to the guide.
  stayOnSearch: boolean;
};

export type SearchStore = { history: string[]; settings: SearchSettings };

export const DEFAULT_STORE: SearchStore = {
  history: [],
  settings: { showHistory: true, favouritesFirst: true, stayOnSearch: false },
};

const HISTORY_LIMIT = 20;

function path(): string {
  return `${DocumentDirectoryPath}/search.json`;
}

export async function loadSearchStore(): Promise<SearchStore> {
  try {
    const saved = JSON.parse(await readFile(path(), 'utf8'));
    return {
      history: Array.isArray(saved.history) ? saved.history : [],
      settings: { ...DEFAULT_STORE.settings, ...saved.settings },
    };
  } catch {
    // Not saved yet, or unreadable: start afresh.
    return DEFAULT_STORE;
  }
}

export function saveSearchStore(store: SearchStore): Promise<void> {
  return writeFile(path(), JSON.stringify(store), 'utf8');
}

// Puts a search at the front of the history, without repeats.
export function withSearch(history: string[], query: string): string[] {
  const trimmed = query.trim();
  if (!trimmed) {
    return history;
  }
  const rest = history.filter(h => h.toLowerCase() !== trimmed.toLowerCase());
  return [trimmed, ...rest].slice(0, HISTORY_LIMIT);
}
