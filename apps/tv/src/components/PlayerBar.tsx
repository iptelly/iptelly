import { StyleSheet, Text, View } from 'react-native';
import { formatPosition } from '../media';
import { colors, fonts, px } from '../theme';
import { Icon } from './Icon';

// Shown over a full-screen movie or episode while it's paused, and for a
// few seconds after seeking.
export function PlayerBar({
  title,
  position,
  duration,
  paused,
}: {
  title: string;
  position: number;
  duration: number;
  paused: boolean;
}) {
  const progress = duration > 0 ? Math.min(1, position / duration) : 0;
  return (
    <View style={styles.bar}>
      <Text numberOfLines={1} style={styles.title}>
        {title}
      </Text>
      <View style={styles.row}>
        <Icon
          name={paused ? 'play' : 'pause'}
          size={px(48)}
          color={colors.text}
        />
        <Text style={styles.time}>{formatPosition(position)}</Text>
        <View style={styles.track}>
          <View style={[styles.done, { width: `${progress * 100}%` }]} />
        </View>
        <Text style={styles.time}>
          {duration > 0 ? formatPosition(duration) : ''}
        </Text>
      </View>
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
  title: {
    color: colors.text,
    fontSize: fonts.title,
    fontWeight: 'bold',
  },
  row: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: px(24),
    marginTop: px(20),
  },
  time: {
    color: colors.text,
    fontSize: fonts.normal,
    minWidth: px(120),
  },
  track: {
    flex: 1,
    height: px(8),
    borderRadius: px(4),
    overflow: 'hidden',
    backgroundColor: colors.line,
  },
  done: {
    height: '100%',
    backgroundColor: colors.accent,
  },
});
