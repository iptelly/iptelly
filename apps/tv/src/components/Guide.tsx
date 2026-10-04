import { memo } from 'react';
import { Image, StyleSheet, Text, View } from 'react-native';
import type { Channel } from 'react-native-iptelly';
import { channelKey } from '../core';
import { HALF_HOUR, formatDate, formatTime, type Block } from '../guide';
import { colors, fonts, px } from '../theme';
import { Icon } from './Icon';

// Where the guide sits, in 1920×1080 design pixels.
export const GUIDE_TOP = 410;
const HEADER_HEIGHT = 64;
const ROW_HEIGHT = 70;
const PADDING = 40;
const CHANNEL_COLUMN = 520;
// 30 minutes take 300 pixels.
const PX_PER_SECOND = 10 / 60;
export const VISIBLE_ROWS = Math.ceil(
  (1080 - GUIDE_TOP - HEADER_HEIGHT) / ROW_HEIGHT,
);

// How many seconds of programmes fit beside the channels when the guide
// starts at `x`.
export function windowLength(x: number): number {
  const width = 1920 - x - PADDING - CHANNEL_COLUMN;
  return Math.floor(width / PX_PER_SECOND);
}

export type GuideRow = {
  channel: Channel;
  number: number;
  blocks: Block[];
};

export function Guide({
  x,
  rows,
  focusedRow,
  focusedBlock,
  focused,
  playing,
  windowStart,
  now,
}: {
  x: number;
  rows: GuideRow[];
  focusedRow: number;
  focusedBlock?: Block;
  focused: boolean;
  playing?: string;
  windowStart: number;
  now: number;
}) {
  const length = windowLength(x);
  const marks = [];
  for (let t = windowStart; t < windowStart + length; t += HALF_HOUR) {
    marks.push(t);
  }
  const nowX = (now - windowStart) * PX_PER_SECOND;
  return (
    <View style={[styles.guide, { left: px(x) }]}>
      <View style={styles.header}>
        <Text style={styles.date}>{formatDate(now)}</Text>
        <View style={styles.programmes}>
          {marks.map(t => (
            <Text
              key={t}
              style={[
                styles.mark,
                { left: px((t - windowStart) * PX_PER_SECOND) },
              ]}
            >
              {formatTime(t)}
            </Text>
          ))}
        </View>
      </View>
      <View style={styles.headerLine} />
      {rows.map(row => {
        const key = channelKey(row.channel);
        return (
          <Row
            key={key}
            row={row}
            focused={focused && row.number - 1 === focusedRow}
            focusedBlock={
              row.number - 1 === focusedRow ? focusedBlock : undefined
            }
            guideFocused={focused}
            playing={key === playing}
            windowStart={windowStart}
            length={length}
            now={now}
          />
        );
      })}
      {nowX >= 0 && nowX < length * PX_PER_SECOND && (
        <View
          pointerEvents="none"
          style={[
            styles.nowLine,
            { left: px(PADDING + CHANNEL_COLUMN + nowX) },
          ]}
        >
          <View style={styles.nowDot} />
        </View>
      )}
    </View>
  );
}

const Row = memo(function ChannelRow({
  row,
  focused,
  focusedBlock,
  guideFocused,
  playing,
  windowStart,
  length,
  now,
}: {
  row: GuideRow;
  focused: boolean;
  focusedBlock?: Block;
  guideFocused: boolean;
  playing: boolean;
  windowStart: number;
  length: number;
  now: number;
}) {
  const { channel } = row;
  const windowEnd = windowStart + length;
  const nameColor = focused ? colors.accent : colors.text;
  return (
    <View style={styles.row}>
      <View style={styles.channel}>
        <Text style={[styles.number, { color: nameColor }]}>{row.number}</Text>
        <Logo channel={channel} />
        <Text numberOfLines={1} style={[styles.name, { color: nameColor }]}>
          {channel.name}
        </Text>
        <View style={styles.icons}>
          {playing && <Icon name="play" size={px(30)} color={colors.accent} />}
          {channel.tvArchive && (
            <Icon name="catchup" size={px(30)} color={colors.textDim} />
          )}
        </View>
      </View>
      <View style={styles.programmes}>
        {row.blocks
          .filter(b => b.end > windowStart && b.start < windowEnd)
          .map(b => {
            const start = Math.max(b.start, windowStart);
            const isFocused =
              guideFocused &&
              focusedBlock != null &&
              b.start === focusedBlock.start;
            const isNow = b.start <= now && now < b.end;
            return (
              <View
                key={b.start}
                style={[
                  styles.block,
                  {
                    left: px((start - windowStart) * PX_PER_SECOND),
                    width: px((b.end - start) * PX_PER_SECOND) - px(6),
                  },
                  focused && styles.blockFocusedRow,
                  playing && isNow && !guideFocused && styles.blockPlaying,
                  isFocused && styles.blockFocused,
                ]}
              >
                <Text
                  numberOfLines={1}
                  style={[
                    styles.title,
                    isFocused && { color: colors.textDark },
                  ]}
                >
                  {b.programme?.title || 'No information'}
                </Text>
              </View>
            );
          })}
      </View>
    </View>
  );
});

function Logo({ channel }: { channel: Channel }) {
  if (channel.image) {
    return (
      <Image
        source={{ uri: channel.image }}
        resizeMode="contain"
        style={styles.logo}
      />
    );
  }
  const initials = channel.name
    .replace(/^[A-Z]{2,3}[:|]\s*/, '')
    .split(/\s+/)
    .slice(0, 2)
    .map(w => w[0])
    .join('')
    .toUpperCase();
  return (
    <View style={[styles.logo, styles.initials]}>
      <Text style={styles.initialsText}>{initials}</Text>
    </View>
  );
}

const styles = StyleSheet.create({
  guide: {
    position: 'absolute',
    top: px(GUIDE_TOP),
    right: 0,
    bottom: 0,
    overflow: 'hidden',
  },
  header: {
    height: px(HEADER_HEIGHT - 2),
    flexDirection: 'row',
    alignItems: 'center',
    paddingLeft: px(PADDING),
  },
  headerLine: {
    height: px(2),
    marginLeft: px(PADDING),
    backgroundColor: colors.line,
  },
  date: {
    width: px(CHANNEL_COLUMN),
    color: colors.accent,
    fontSize: fonts.normal,
  },
  mark: {
    position: 'absolute',
    color: colors.text,
    fontSize: fonts.small,
  },
  programmes: {
    flex: 1,
    alignSelf: 'stretch',
    justifyContent: 'center',
    overflow: 'hidden',
  },
  row: {
    height: px(ROW_HEIGHT),
    flexDirection: 'row',
    alignItems: 'center',
    paddingLeft: px(PADDING),
  },
  channel: {
    width: px(CHANNEL_COLUMN),
    flexDirection: 'row',
    alignItems: 'center',
  },
  number: {
    width: px(70),
    fontSize: fonts.small,
    textAlign: 'center',
  },
  logo: {
    width: px(64),
    height: px(56),
    marginHorizontal: px(16),
  },
  initials: {
    borderRadius: px(6),
    backgroundColor: colors.menuDark,
    alignItems: 'center',
    justifyContent: 'center',
  },
  initialsText: {
    color: colors.text,
    fontSize: px(24),
    fontWeight: 'bold',
  },
  name: {
    flex: 1,
    fontSize: fonts.small,
    fontWeight: 'bold',
  },
  icons: {
    width: px(84),
    flexDirection: 'row',
    justifyContent: 'flex-end',
    gap: px(6),
    paddingRight: px(12),
  },
  block: {
    position: 'absolute',
    top: px(4),
    bottom: px(4),
    justifyContent: 'center',
    paddingHorizontal: px(18),
    borderRadius: px(6),
    backgroundColor: colors.block,
  },
  blockFocusedRow: {
    backgroundColor: colors.blockFocusedRow,
  },
  blockPlaying: {
    backgroundColor: colors.blockPlaying,
  },
  blockFocused: {
    backgroundColor: colors.pill,
  },
  title: {
    color: colors.text,
    fontSize: fonts.small,
  },
  nowLine: {
    position: 'absolute',
    top: px(HEADER_HEIGHT - 6),
    bottom: 0,
    width: px(2),
    backgroundColor: colors.accent,
  },
  nowDot: {
    position: 'absolute',
    top: 0,
    left: px(-5),
    width: px(12),
    height: px(12),
    borderRadius: px(6),
    backgroundColor: colors.accent,
  },
});
