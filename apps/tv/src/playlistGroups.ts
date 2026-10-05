// The groups column's rows: the fixed lists (Favourites, All channels...),
// then each playlist's name with its groups under it. OK on a playlist's
// name collapses or expands its groups.

import { useCallback, useEffect, useMemo, useState } from 'react';
import type { Source } from 'react-native-iptelly';
import { errorMessage, loadPlaylistGroups, type ChannelList } from './core';

export type GroupRow =
  | { kind: 'list'; list: ChannelList; indented: boolean }
  | { kind: 'playlist'; sourceId: bigint; name: string; expanded: boolean };

// Builds the rows. A playlist with no groups (of this media type) is left
// out once its groups have loaded.
export function groupRows(
  fixed: ChannelList[],
  playlists: { sourceId: bigint; name: string }[],
  groups: Map<string, ChannelList[]>,
  collapsed: Set<string>,
): GroupRow[] {
  const rows: GroupRow[] = fixed.map(list => ({
    kind: 'list',
    list,
    indented: false,
  }));
  for (const { sourceId, name } of playlists) {
    const loaded = groups.get(String(sourceId));
    if (loaded?.length === 0) {
      continue;
    }
    const expanded = !collapsed.has(String(sourceId));
    rows.push({ kind: 'playlist', sourceId, name, expanded });
    if (expanded) {
      for (const list of loaded ?? []) {
        rows.push({ kind: 'list', list, indented: true });
      }
    }
  }
  return rows;
}

// `version` loads the groups again when it changes, after a playlist is
// added, updated or deleted.
export function usePlaylistGroups(
  sources: Source[],
  fixed: ChannelList[],
  mediaType: number,
  version: number,
  say: (text: string) => void,
) {
  const [groups, setGroups] = useState(() => new Map<string, ChannelList[]>());
  const [collapsed, setCollapsed] = useState(() => new Set<string>());

  const playlists = useMemo(
    () =>
      sources
        .filter(s => s.enabled && s.id != null)
        .map(s => ({ sourceId: s.id!, name: s.name })),
    [sources],
  );

  useEffect(() => {
    let current = true;
    setGroups(new Map());
    for (const { sourceId } of playlists) {
      loadPlaylistGroups(sourceId, mediaType)
        .then(loaded => {
          if (current) {
            setGroups(previous =>
              new Map(previous).set(String(sourceId), loaded),
            );
          }
        })
        .catch(e => say(errorMessage(e)));
    }
    return () => {
      current = false;
    };
  }, [playlists, mediaType, version, say]);

  const toggle = useCallback((sourceId: bigint) => {
    setCollapsed(previous => {
      const next = new Set(previous);
      const key = String(sourceId);
      if (!next.delete(key)) {
        next.add(key);
      }
      return next;
    });
  }, []);

  const rows = useMemo(
    () => groupRows(fixed, playlists, groups, collapsed),
    [fixed, playlists, groups, collapsed],
  );
  return { rows, toggle };
}
