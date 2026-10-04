import { StyleSheet, Text, View } from 'react-native';
import type { Channel } from 'react-native-iptelly';
import type { Block } from '../guide';
import { colors, fonts, px } from '../theme';
import { ProgrammeTimes } from './Details';

// Shown over full-screen video for a few seconds after changing channel.
export function InfoBar({
  channel,
  number,
  current,
  next,
  now,
}: {
  channel: Channel;
  number?: number;
  current?: Block;
  next?: Block;
  now: number;
}) {
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
});
