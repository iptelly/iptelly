import { StyleSheet, Text, View } from 'react-native';
import type { Channel } from 'react-native-iptelly';
import { colors, fonts, px } from '../theme';
import { CAPTION_HEIGHT, GAP, Poster, SHAPES, type Shape } from './PosterGrid';
import { useFirstVisible } from './scroll';

const TITLE_HEIGHT = 60;

// A row's height in design pixels: its title, cards and the gap below.
export function rowHeight(shape: Shape): number {
  return TITLE_HEIGHT + SHAPES[shape].height + CAPTION_HEIGHT + GAP * 2;
}

// A titled row of cards that scrolls sideways to keep the highlighted one
// in view, like TiviMate's search results.
export function PosterRow({
  title,
  items,
  index,
  focused,
  shape,
}: {
  title: string;
  items: Channel[];
  index: number;
  focused: boolean;
  shape: Shape;
}) {
  const visible = SHAPES[shape].columns;
  const first = useFirstVisible(index, items.length, visible);
  return (
    <View style={{ height: px(rowHeight(shape)) }}>
      <Text style={styles.title}>{title}</Text>
      <View style={styles.cards}>
        {items.slice(first, first + visible + 1).map((item, i) => (
          <Poster
            key={`${item.id}:${first + i}`}
            item={item}
            shape={shape}
            highlighted={focused && first + i === index}
          />
        ))}
      </View>
    </View>
  );
}

const styles = StyleSheet.create({
  title: {
    height: px(TITLE_HEIGHT),
    color: colors.text,
    fontSize: fonts.large,
    fontWeight: 'bold',
    textAlignVertical: 'center',
  },
  cards: {
    flexDirection: 'row',
    columnGap: px(GAP),
    overflow: 'hidden',
  },
});
