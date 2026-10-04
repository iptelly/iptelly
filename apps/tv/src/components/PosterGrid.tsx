import { memo, useState } from 'react';
import { Image, StyleSheet, Text, View } from 'react-native';
import type { Channel } from 'react-native-iptelly';
import { formatRating } from '../media';
import { colors, fonts, px } from '../theme';

// Movie and series posters are tall; episode pictures are wide.
export const SHAPES = {
  poster: { columns: 5, width: 232, height: 348 },
  wide: { columns: 3, width: 400, height: 225 },
};
export type Shape = keyof typeof SHAPES;

const GAP = 24;
const CAPTION_HEIGHT = 56;

// A grid of posters with their titles. The highlighted poster's row is
// always the top one, with the next row peeking out below it.
export function PosterGrid({
  items,
  index,
  focused,
  shape,
  height,
}: {
  items: Channel[];
  index: number;
  focused: boolean;
  shape: Shape;
  // The grid's height in design pixels; rows below it are cut off.
  height: number;
}) {
  const { columns, height: posterHeight } = SHAPES[shape];
  const rowHeight = posterHeight + CAPTION_HEIGHT + GAP;
  const firstRow = Math.floor(index / columns);
  const rows = Math.ceil(height / rowHeight);
  const shown = items.slice(firstRow * columns, (firstRow + rows) * columns);
  return (
    <View style={[styles.grid, { height: px(height) }]}>
      {shown.map((item, i) => {
        const at = firstRow * columns + i;
        return (
          <Poster
            key={`${item.id}:${at}`}
            item={item}
            shape={shape}
            highlighted={focused && at === index}
          />
        );
      })}
    </View>
  );
}

const Poster = memo(function PosterCard({
  item,
  shape,
  highlighted,
}: {
  item: Channel;
  shape: Shape;
  highlighted: boolean;
}) {
  const { width, height } = SHAPES[shape];
  const [failed, setFailed] = useState(false);
  return (
    <View style={{ width: px(width) }}>
      <View
        style={[
          styles.picture,
          { height: px(height) },
          highlighted && styles.highlightedPicture,
        ]}
      >
        {item.image && !failed ? (
          <Image
            source={{ uri: item.image }}
            style={StyleSheet.absoluteFill}
            resizeMode="cover"
            onError={() => setFailed(true)}
          />
        ) : (
          <Text numberOfLines={4} style={styles.noPicture}>
            {item.name}
          </Text>
        )}
        {item.rating != null && (
          <View style={styles.rating}>
            <Text style={styles.ratingText}>{formatRating(item.rating)}</Text>
          </View>
        )}
      </View>
      <View style={[styles.caption, highlighted && styles.highlightedCaption]}>
        <Text
          numberOfLines={1}
          style={[styles.title, highlighted && { color: colors.textDark }]}
        >
          {item.name}
        </Text>
      </View>
    </View>
  );
});

const styles = StyleSheet.create({
  grid: {
    flexDirection: 'row',
    flexWrap: 'wrap',
    columnGap: px(GAP),
    rowGap: px(GAP),
    overflow: 'hidden',
  },
  picture: {
    borderTopLeftRadius: px(10),
    borderTopRightRadius: px(10),
    overflow: 'hidden',
    backgroundColor: colors.backgroundDark,
    justifyContent: 'center',
    borderWidth: px(4),
    borderBottomWidth: 0,
    borderColor: 'transparent',
  },
  highlightedPicture: {
    borderColor: colors.pill,
  },
  noPicture: {
    color: colors.textDim,
    fontSize: fonts.normal,
    textAlign: 'center',
    padding: px(16),
  },
  rating: {
    position: 'absolute',
    left: px(12),
    bottom: px(12),
    paddingHorizontal: px(10),
    paddingVertical: px(2),
    borderRadius: px(6),
    backgroundColor: colors.accent,
  },
  ratingText: {
    color: colors.text,
    fontSize: fonts.small,
  },
  caption: {
    height: px(CAPTION_HEIGHT),
    justifyContent: 'center',
    paddingHorizontal: px(14),
    borderBottomLeftRadius: px(10),
    borderBottomRightRadius: px(10),
    backgroundColor: colors.block,
  },
  highlightedCaption: {
    backgroundColor: colors.pill,
  },
  title: {
    color: colors.text,
    fontSize: fonts.small,
  },
});
