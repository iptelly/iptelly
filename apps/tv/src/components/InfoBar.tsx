import { StyleSheet, Text, View } from 'react-native';
import type { Channel } from 'react-native-iptelly';
import type { Block } from '../guide';
import {
  audioText,
  frameRateText,
  resolutionText,
  type StreamInfo,
} from '../streamInfo';
import { colors, fonts, px } from '../theme';
import { ProgrammeTimes } from './Details';

// Shown over full-screen video for a few seconds after changing channel,
// with the stream's resolution, frame rate and audio in boxes along the
// bottom once the player knows them.
export function InfoBar({
  channel,
  number,
  current,
  next,
  now,
  stream = {},
}: {
  channel: Channel;
  number?: number;
  current?: Block;
  next?: Block;
  now: number;
  stream?: StreamInfo;
}) {
  const boxes = [
    resolutionText(stream),
    frameRateText(stream),
    audioText(stream),
  ].filter((box): box is string => box != null);
  return (
    <View style={styles.bar}>
      <View style={styles.channel}>
        {number != null && <Text style={styles.number}>{number}</Text>}
        <Text numberOfLines={1} style={styles.name}>
          {channel.name}
        </Text>
      </View>
      {current && (
        <>
          <Text numberOfLines={1} style={styles.title}>
            {current.programme?.title || 'No information'}
          </Text>
          <ProgrammeTimes block={current} now={now} />
        </>
      )}
      {next?.programme && (
        <Text numberOfLines={1} style={styles.next}>
          Next: {next.programme.title}
        </Text>
      )}
      {boxes.length > 0 && (
        <View style={styles.boxes}>
          {boxes.map(box => (
            <Text key={box} style={styles.box}>
              {box}
            </Text>
          ))}
        </View>
      )}
    </View>
  );
}

const styles = StyleSheet.create({
  bar: {
    position: 'absolute',
    left: px(60),
    right: px(60),
    bottom: px(50),
    padding: px(36),
    borderRadius: px(16),
    backgroundColor: 'rgba(31, 43, 59, 0.88)',
  },
  channel: {
    flexDirection: 'row',
    alignItems: 'baseline',
    gap: px(24),
  },
  number: {
    color: colors.accent,
    fontSize: fonts.title,
    fontWeight: 'bold',
  },
  name: {
    flex: 1,
    color: colors.text,
    fontSize: fonts.title,
    fontWeight: 'bold',
  },
  title: {
    color: colors.text,
    fontSize: fonts.large,
    marginTop: px(16),
  },
  next: {
    color: colors.textDim,
    fontSize: fonts.normal,
    marginTop: px(12),
  },
  boxes: {
    flexDirection: 'row',
    gap: px(16),
    marginTop: px(20),
  },
  box: {
    paddingHorizontal: px(14),
    paddingVertical: px(4),
    borderWidth: px(2),
    borderColor: colors.textDim,
    borderRadius: px(6),
    color: colors.text,
    fontSize: fonts.small,
    fontWeight: 'bold',
  },
});
