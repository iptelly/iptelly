import { StyleSheet, Text, View } from 'react-native';
import type { Channel } from 'react-native-iptelly';
import { formatTime, progress, type Block } from '../guide';
import { colors, fonts, px } from '../theme';
import { Icon } from './Icon';

// The programme's title, times and how far through it is.
export function ProgrammeTimes({ block, now }: { block: Block; now: number }) {
  const onNow = block.start <= now && now < block.end;
  const left = Math.ceil((block.end - now) / 60);
  return (
    <View style={styles.times}>
      <Text numberOfLines={1} style={styles.time}>
        {formatTime(block.start)} – {formatTime(block.end)}
      </Text>
      {onNow && (
        <>
          <View style={styles.bar}>
            <View
              style={[
                styles.barDone,
                { width: `${progress(block, now) * 100}%` },
              ]}
            />
          </View>
          <Text style={styles.time}>{left} min</Text>
        </>
      )}
    </View>
  );
}

// Beside the preview: the highlighted programme and, in the full guide, the
// channel's favourite star and the group being shown.
export function Details({
  x,
  block,
  channel,
  group,
  now,
}: {
  x: number;
  block?: Block;
  channel?: Channel;
  group?: string;
  now: number;
}) {
  return (
    <View style={[styles.details, { left: px(x) }]}>
      {block && (
        <>
          <Text
            numberOfLines={1}
            style={[styles.title, group != null && styles.besideCorner]}
          >
            {block.programme?.title || 'No information'}
          </Text>
          <ProgrammeTimes block={block} now={now} />
          {!!block.programme?.description && (
            <Text numberOfLines={5} style={styles.description}>
              {block.programme.description}
            </Text>
          )}
        </>
      )}
      {group != null && channel && (
        <View style={styles.corner}>
          <Icon
            name={channel.favorite ? 'star' : 'starOutline'}
            size={px(40)}
            color={colors.text}
          />
          <Text numberOfLines={1} style={styles.group}>
            {group}
          </Text>
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  details: {
    position: 'absolute',
    top: px(40),
    right: px(40),
    height: px(340),
    overflow: 'hidden',
  },
  title: {
    color: colors.text,
    fontSize: fonts.title,
    fontWeight: 'bold',
    marginTop: px(6),
  },
  besideCorner: {
    marginRight: px(360),
  },
  times: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: px(24),
    marginTop: px(14),
  },
  time: {
    color: colors.text,
    fontSize: fonts.normal,
  },
  bar: {
    width: px(80),
    height: px(6),
    borderRadius: px(3),
    backgroundColor: colors.line,
    overflow: 'hidden',
  },
  barDone: {
    height: '100%',
    backgroundColor: colors.text,
  },
  description: {
    color: colors.textDim,
    fontSize: fonts.small,
    marginTop: px(20),
    lineHeight: px(36),
  },
  corner: {
    position: 'absolute',
    top: 0,
    right: 0,
    alignItems: 'flex-end',
    gap: px(20),
    maxWidth: px(340),
  },
  group: {
    color: colors.text,
    fontSize: fonts.normal,
  },
});
