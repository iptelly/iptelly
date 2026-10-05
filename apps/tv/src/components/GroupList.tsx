import { StyleSheet, Text, View } from 'react-native';
import type { GroupRow } from '../playlistGroups';
import { colors, fonts, px } from '../theme';
import { useFirstVisible } from './scroll';

const ROW_HEIGHT = 74;
const VISIBLE_ROWS = 14;

// The groups column: Favourites, All channels, then each playlist's name
// with its groups under it. `index` is the highlighted row and `open` the
// one shown in the guide (-1 when it's in a collapsed playlist).
export function GroupList({
  rows,
  index,
  open,
  focused,
  width,
}: {
  rows: GroupRow[];
  index: number;
  open: number;
  focused: boolean;
  width: number;
}) {
  const top = useFirstVisible(
    focused || open < 0 ? index : open,
    rows.length,
    VISIBLE_ROWS,
  );
  return (
    <View style={[styles.column, { width: px(width) }]}>
      {rows.slice(top, top + VISIBLE_ROWS + 1).map((row, i) => {
        const at = top + i;
        const highlighted = at === (focused ? index : open);
        const dark = highlighted && focused;
        return (
          <View
            key={at}
            style={[
              styles.row,
              highlighted && (focused ? styles.focused : styles.open),
            ]}
          >
            {row.kind === 'playlist' ? (
              <Text
                numberOfLines={1}
                style={[styles.playlist, dark && styles.dark]}
              >
                {row.expanded ? '▾  ' : '▸  '}
                {row.name}
              </Text>
            ) : (
              <Text
                numberOfLines={1}
                style={[
                  styles.name,
                  row.indented && styles.indented,
                  dark && styles.dark,
                ]}
              >
                {row.list.name}
              </Text>
            )}
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
  indented: {
    marginLeft: px(28),
  },
  playlist: {
    color: colors.textDim,
    fontSize: fonts.normal,
    fontWeight: 'bold',
  },
  dark: {
    color: colors.textDark,
  },
});
