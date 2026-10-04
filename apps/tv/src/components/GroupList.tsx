import { StyleSheet, Text, View } from 'react-native';
import type { ChannelList } from '../core';
import { colors, fonts, px } from '../theme';
import { useFirstVisible } from './scroll';

const ROW_HEIGHT = 74;
const VISIBLE_ROWS = 14;

// The groups column: Favourites, All channels, then each playlist group.
// `index` is the highlighted group and `open` the one shown in the guide.
export function GroupList({
  groups,
  index,
  open,
  focused,
  width,
}: {
  groups: ChannelList[];
  index: number;
  open: number;
  focused: boolean;
  width: number;
}) {
  const top = useFirstVisible(
    focused ? index : open,
    groups.length,
    VISIBLE_ROWS,
  );
  return (
    <View style={[styles.column, { width: px(width) }]}>
      {groups.slice(top, top + VISIBLE_ROWS + 1).map((group, i) => {
        const at = top + i;
        const highlighted = at === (focused ? index : open);
        return (
          <View
            key={at}
            style={[
              styles.row,
              highlighted && (focused ? styles.focused : styles.open),
            ]}
          >
            <Text
              numberOfLines={1}
              style={[
                styles.name,
                highlighted && focused && { color: colors.textDark },
              ]}
            >
              {group.name}
            </Text>
          </View>
        );
      })}
    </View>
  );
}

const styles = StyleSheet.create({
  column: {
    height: '100%',
    backgroundColor: colors.groups,
    paddingTop: px(50),
    paddingHorizontal: px(30),
    overflow: 'hidden',
  },
  row: {
    height: px(ROW_HEIGHT - 8),
    marginVertical: px(4),
    justifyContent: 'center',
    paddingHorizontal: px(24),
    borderRadius: px(10),
  },
  focused: {
    backgroundColor: colors.pill,
  },
  open: {
    backgroundColor: colors.pillSoft,
  },
  name: {
    color: colors.text,
    fontSize: fonts.normal,
  },
});
