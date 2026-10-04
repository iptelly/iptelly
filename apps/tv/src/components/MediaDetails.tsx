import { Image, StyleSheet, Text, View } from 'react-native';
import type { Channel, MediaInfo } from 'react-native-iptelly';
import Svg, { Defs, LinearGradient, Rect, Stop } from 'react-native-svg';
import { detailsLine, formatRating } from '../media';
import { colors, fonts, px } from '../theme';

// The highlighted movie or series: its title, rating, year, length, genre,
// cast, director and plot, as far as the provider gives them.
export function MediaDetails({
  item,
  info,
}: {
  item?: Channel;
  info?: MediaInfo;
}) {
  if (!item) {
    return null;
  }
  const rating = info?.rating ?? item.rating;
  const line = detailsLine(info);
  return (
    <View>
      <Text numberOfLines={1} style={styles.title}>
        {item.name}
      </Text>
      {(rating != null || line !== '') && (
        <View style={styles.line}>
          {rating != null && (
            <View style={styles.rating}>
              <Text style={styles.ratingText}>{formatRating(rating)}</Text>
            </View>
          )}
          <Text numberOfLines={1} style={styles.text}>
            {line}
          </Text>
        </View>
      )}
      {info?.cast && <Credit label="Cast:" names={info.cast} />}
      {info?.director && <Credit label="Director:" names={info.director} />}
      {info?.plot && (
        <Text numberOfLines={3} style={[styles.text, styles.plot]}>
          {info.plot}
        </Text>
      )}
    </View>
  );
}

function Credit({ label, names }: { label: string; names: string }) {
  return (
    <View style={styles.credit}>
      <Text style={[styles.text, styles.label]}>{label}</Text>
      <Text numberOfLines={1} style={[styles.text, styles.names]}>
        {names}
      </Text>
    </View>
  );
}

// The backdrop behind the details, faded into the background on its left
// and bottom edges so the text stays readable.
export function Backdrop({
  uri,
  left,
  height,
}: {
  uri?: string;
  left: number;
  height: number;
}) {
  if (!uri) {
    return null;
  }
  const box = { left: px(left), height: px(height) };
  return (
    <View style={[styles.backdrop, box]}>
      <Image
        source={{ uri }}
        style={StyleSheet.absoluteFill}
        resizeMode="cover"
      />
      <Svg style={StyleSheet.absoluteFill} width="100%" height="100%">
        <Defs>
          <LinearGradient id="left" x1="0" y1="0" x2="1" y2="0">
            <Stop offset="0" stopColor={colors.background} stopOpacity={1} />
            <Stop
              offset="0.6"
              stopColor={colors.background}
              stopOpacity={0.55}
            />
            <Stop offset="1" stopColor={colors.background} stopOpacity={0.2} />
          </LinearGradient>
          <LinearGradient id="bottom" x1="0" y1="0" x2="0" y2="1">
            <Stop offset="0.5" stopColor={colors.background} stopOpacity={0} />
            <Stop offset="1" stopColor={colors.background} stopOpacity={1} />
          </LinearGradient>
        </Defs>
        <Rect width="100%" height="100%" fill="url(#left)" />
        <Rect width="100%" height="100%" fill="url(#bottom)" />
      </Svg>
    </View>
  );
}

const styles = StyleSheet.create({
  title: {
    color: colors.text,
    fontSize: px(58),
    fontWeight: 'bold',
  },
  line: {
    flexDirection: 'row',
    alignItems: 'center',
    gap: px(24),
    marginTop: px(16),
  },
  rating: {
    paddingHorizontal: px(12),
    paddingVertical: px(2),
    borderRadius: px(6),
    backgroundColor: colors.pill,
  },
  ratingText: {
    color: colors.textDark,
    fontSize: fonts.small,
  },
  text: {
    color: colors.text,
    fontSize: fonts.normal,
  },
  credit: {
    flexDirection: 'row',
    marginTop: px(12),
  },
  label: {
    color: colors.textDim,
    width: px(150),
  },
  names: {
    flex: 1,
  },
  plot: {
    marginTop: px(24),
    lineHeight: px(44),
  },
  backdrop: {
    position: 'absolute',
    top: 0,
    right: 0,
  },
});
