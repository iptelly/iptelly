// A made-up playlist and guide for trying the app without a provider. The
// channels are public test streams from Mux, Apple and Google, and the
// movies are open films from the Internet Archive.

import {
  DocumentDirectoryPath,
  mkdir,
  writeFile,
} from '@dr.pogodin/react-native-fs';
import {
  addSource,
  getSources,
  refreshEpg,
  refreshSource,
} from 'react-native-iptelly';
import { SourceType } from './core';
import { HOUR, floorHour } from './guide';

export const DEMO_NAME = 'Demo';

const STREAMS = [
  'https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8',
  'https://devstreaming-cdn.apple.com/videos/streaming/examples/img_bipbop_adv_example_fmp4/master.m3u8',
  'https://storage.googleapis.com/shaka-demo-assets/angel-one-hls/hls.m3u8',
];

const GROUPS: Record<string, { channels: string[]; titles: string[] }> = {
  News: {
    channels: ['Demo News', 'Demo News 24', 'Demo Business', 'Demo Weather'],
    titles: [
      'Breakfast',
      'The Headlines',
      'World Report',
      'Business Today',
      'Weather',
      'Newsnight',
    ],
  },
  Sport: {
    channels: ['Demo Sport 1', 'Demo Sport 2', 'Demo Football', 'Demo Racing'],
    titles: [
      'Live Football',
      'Match of the Day',
      'Tennis Highlights',
      'Grand Prix',
      'Cricket',
      'Golf',
    ],
  },
  Films: {
    channels: ['Demo Cinema', 'Demo Classics', 'Demo Action', 'Demo Comedy'],
    titles: [
      'The Long Night',
      'River Run',
      'Moonlight Express',
      'The Last Harbour',
      'Paper Planes',
      'City Lights',
    ],
  },
  Kids: {
    channels: ['Demo Kids', 'Demo Cartoons', 'Demo Junior'],
    titles: [
      'Cartoon Club',
      'Story Time',
      'Space Explorers',
      'Animal Friends',
      'Build It',
      'Music Box',
    ],
  },
};

// The Blender Foundation's open films, from the Internet Archive, as
// movies. The core counts an M3U entry as a movie when its link ends in .mp4.
const ARCHIVE = 'https://archive.org';
const MOVIES = [
  [
    'Big Buck Bunny',
    'BigBuckBunny_124',
    'Content/big_buck_bunny_720p_surround.mp4',
  ],
  ['Elephants Dream', 'ElephantsDream', 'ed_1024_512kb.mp4'],
  ['Sintel', 'Sintel', 'sintel-2048-surround_512kb.mp4'],
  ['Tears of Steel', 'Tears-of-Steel', 'tears_of_steel_720p.mp4'],
];

// Programme lengths in half hours, cycled through with a different start
// for each channel.
const LENGTHS = [1, 2, 1, 3, 2, 1, 4, 2];

function xmltvTime(time: number): string {
  return `${new Date(time * 1000)
    .toISOString()
    .replace(/[-:T]/g, '')
    .slice(0, 14)} +0000`;
}

function escape(text: string): string {
  return text
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}

function programme(id: string, start: number, end: number, title: string) {
  const desc = `${title}. A made-up programme for trying out the guide.`;
  const times = `start="${xmltvTime(start)}" stop="${xmltvTime(end)}"`;
  return (
    `<programme channel="${id}" ${times}>` +
    `<title>${escape(title)}</title><desc>${escape(desc)}</desc>` +
    '</programme>'
  );
}

export function demoFiles(now: number): { m3u: string; xml: string } {
  const m3u = ['#EXTM3U'];
  const xml = ['<?xml version="1.0" encoding="UTF-8"?>', '<tv>'];
  let n = 0;
  for (const [group, { channels, titles }] of Object.entries(GROUPS)) {
    channels.forEach((name, i) => {
      n += 1;
      // The last channel in each group has no guide, to show how that looks.
      const id = i < channels.length - 1 ? `demo${n}.tv` : undefined;
      const tvgId = id ? ` tvg-id="${id}"` : '';
      m3u.push(`#EXTINF:-1${tvgId} group-title="${group}",${name}`);
      m3u.push(STREAMS[n % STREAMS.length]);
      if (!id) {
        return;
      }
      xml.push(
        `<channel id="${id}"><display-name>${escape(
          name,
        )}</display-name></channel>`,
      );
      let start = floorHour(now) - 6 * HOUR;
      for (let k = n; start < now + 3 * 24 * HOUR; k++) {
        const end = start + (LENGTHS[k % LENGTHS.length] * HOUR) / 2;
        xml.push(programme(id, start, end, titles[k % titles.length]));
        start = end;
      }
    });
  }
  for (const [name, item, file] of MOVIES) {
    const logo = `${ARCHIVE}/services/img/${item}`;
    m3u.push(`#EXTINF:-1 tvg-logo="${logo}" group-title="Demo Movies",${name}`);
    m3u.push(`${ARCHIVE}/download/${item}/${file}`);
  }
  xml.push('</tv>');
  return { m3u: `${m3u.join('\n')}\n`, xml: `${xml.join('\n')}\n` };
}

// Writes the demo files and adds them as a playlist, with its guide.
export async function addDemoPlaylist(): Promise<void> {
  const dir = `${DocumentDirectoryPath}/demo`;
  await mkdir(dir);
  const { m3u, xml } = demoFiles(Math.floor(Date.now() / 1000));
  await writeFile(`${dir}/demo.m3u`, m3u, 'utf8');
  await writeFile(`${dir}/demo.xml`, xml, 'utf8');
  const existing = (await getSources()).find(s => s.name === DEMO_NAME);
  if (existing?.id != null) {
    // Picks up anything new in the demo since it was added.
    await refreshSource(existing.id);
  } else {
    await addSource({
      name: DEMO_NAME,
      url: `${dir}/demo.m3u`,
      sourceType: SourceType.M3U,
      epgUrl: `${dir}/demo.xml`,
      enabled: true,
    });
  }
  const source = (await getSources()).find(s => s.name === DEMO_NAME);
  if (source?.id != null) {
    await refreshEpg(source.id);
  }
}
