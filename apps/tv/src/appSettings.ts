// The app's own settings (Settings > General, and the playlists' order),
// kept as settings.json in the app's files folder. The Android side reads the
// auto-start settings from there too (android/.../AutoStart.kt), so their
// names mustn't change.

import {
  DocumentDirectoryPath,
  readFile,
  writeFile,
} from '@dr.pogodin/react-native-fs';
import type { PlaylistSort } from './playlists';

export type GeneralSettings = {
  autoStartOnBoot: boolean;
  autoStartOnWake: boolean;
  lastChannelOnStart: boolean;
  pipOnHome: boolean;
  confirmExit: boolean;
  // For playback, when a playlist doesn't set its own.
  userAgent: string;
  // A udpxy-style proxy for udp:// and rtp:// streams, "address:port".
  udpProxy: string;
  // Settings > Playlists: how playlists are ordered, and the manual order
  // (playlist ids).
  playlistSort: PlaylistSort;
  playlistOrder: string[];
};

export const DEFAULT_SETTINGS: GeneralSettings = {
  autoStartOnBoot: false,
  autoStartOnWake: false,
  lastChannelOnStart: true,
  pipOnHome: false,
  confirmExit: false,
  userAgent: '',
  udpProxy: '',
  playlistSort: 'name',
  playlistOrder: [],
};

// The rows of Settings > General, in order.
export type GeneralItem =
  | {
      kind: 'toggle';
      key: keyof GeneralSettings;
      label: string;
      detail?: string;
    }
  | {
      kind: 'text';
      key: 'userAgent' | 'udpProxy';
      label: string;
      placeholder: string;
      hint?: string;
    }
  | { kind: 'backup' | 'restore'; label: string };

export const GENERAL_ITEMS: GeneralItem[] = [
  { kind: 'toggle', key: 'autoStartOnBoot', label: 'Auto start app on boot' },
  {
    kind: 'toggle',
    key: 'autoStartOnWake',
    label: 'Auto start app on wake up from sleep mode',
    detail: 'May not work on all devices',
  },
  {
    kind: 'toggle',
    key: 'lastChannelOnStart',
    label: 'Turn on last channel on app start',
  },
  {
    kind: 'toggle',
    key: 'pipOnHome',
    label: 'Switch to picture-in-picture mode on press Home',
  },
  {
    kind: 'toggle',
    key: 'confirmExit',
    label: 'Confirm exit by second press Back',
  },
  {
    kind: 'text',
    key: 'userAgent',
    label: 'User-Agent',
    placeholder: 'User-Agent',
    hint: "Used to play streams from playlists that don't set their own.",
  },
  {
    kind: 'text',
    key: 'udpProxy',
    label: 'UDP proxy (address:port)',
    placeholder: 'e.g. 192.168.1.2:4022',
    hint: 'A udpxy proxy for udp:// and rtp:// multicast streams.',
  },
  { kind: 'backup', label: 'Back up data' },
  { kind: 'restore', label: 'Restore data' },
];

// The General list's switches and second lines (a value, or "Not set").
export function generalList(settings: GeneralSettings): {
  toggles: (boolean | undefined)[];
  details: (string | undefined)[];
} {
  return {
    toggles: GENERAL_ITEMS.map(item =>
      item.kind === 'toggle' ? Boolean(settings[item.key]) : undefined,
    ),
    details: GENERAL_ITEMS.map(item => {
      if (item.kind === 'toggle') {
        return item.detail;
      }
      if (item.kind === 'text') {
        return settings[item.key] || 'Not set';
      }
      return undefined;
    }),
  };
}

function path(): string {
  return `${DocumentDirectoryPath}/settings.json`;
}

export async function loadSettings(): Promise<GeneralSettings> {
  try {
    const saved = JSON.parse(await readFile(path(), 'utf8'));
    return { ...DEFAULT_SETTINGS, ...saved };
  } catch {
    // Not saved yet, or unreadable: the defaults.
    return DEFAULT_SETTINGS;
  }
}

export function saveSettings(settings: GeneralSettings): Promise<void> {
  return writeFile(path(), JSON.stringify(settings), 'utf8');
}

// A multicast stream through the UDP proxy, the way udpxy expects it:
// udp://@239.1.1.1:1234 becomes http://proxy/udp/239.1.1.1:1234. Other
// streams, or no proxy, are left alone.
export function streamUrl(url: string, udpProxy: string): string {
  const proxy = udpProxy
    .trim()
    .replace(/^https?:\/\//, '')
    .replace(/\/$/, '');
  const match = /^(udp|rtp):\/\/@?(.+)$/i.exec(url.trim());
  if (!proxy || !match) {
    return url;
  }
  return `http://${proxy}/${match[1].toLowerCase()}/${match[2]}`;
}
