import { StyleSheet, Text, View } from 'react-native';
import { colors, fonts, px } from '../theme';
import { Icon, type IconName } from './Icon';
import { Panel } from './Panel';

export type MenuItem = {
  key: 'search' | 'channels' | 'movies' | 'series' | 'favourites' | 'settings';
  label: string;
  icon: IconName;
};

export const MENU_ITEMS: MenuItem[] = [
  { key: 'search', label: 'Search', icon: 'search' },
  { key: 'channels', label: 'Channels', icon: 'tv' },
  { key: 'movies', label: 'Movies', icon: 'movie' },
  { key: 'series', label: 'Series', icon: 'series' },
  { key: 'favourites', label: 'Favourites', icon: 'starOutline' },
  { key: 'settings', label: 'Settings', icon: 'settings' },
];

export const MENU_WIDTH = 470;
export const RAIL_WIDTH = 110;

const ITEM_HEIGHT = 92;

// The left-hand menu. Expanded while it has focus, otherwise a rail of
// icons. `index` is the highlighted item and `section` the open one.
export function Menu({
  expanded,
  focused,
  index,
  section,
}: {
  expanded: boolean;
  focused: boolean;
  index: number;
  section: number;
}) {
  const settings = MENU_ITEMS.length - 1;
  const item = (i: number) => {
    const { label, icon } = MENU_ITEMS[i];
    const highlighted = focused && i === index;
    const open = !focused && i === section && expanded;
    const color = highlighted
      ? colors.textDark
      : i === section || focused
      ? colors.text
      : colors.textDim;
    return (
      <View
        key={label}
        style={[
          styles.item,
          !expanded && styles.railItem,
          highlighted && styles.highlighted,
          open && styles.open,
        ]}
      >
        <Icon name={icon} size={px(40)} color={color} />
        {expanded && <Text style={[styles.label, { color }]}>{label}</Text>}
      </View>
    );
  };
  return (
    <Panel
      from={expanded ? colors.menu : colors.menuDark}
      to={colors.menuDark}
      style={[styles.menu, { width: px(expanded ? MENU_WIDTH : RAIL_WIDTH) }]}
    >
      <Text style={[styles.logo, !expanded && styles.railLogo]}>
        <Text style={styles.logoAccent}>IP</Text>
        {expanded ? 'Telly' : ''}
      </Text>
      <View style={styles.items}>
        {MENU_ITEMS.slice(0, settings).map((_, i) => item(i))}
      </View>
      {item(settings)}
    </Panel>
  );
}

const styles = StyleSheet.create({
  menu: {
    height: '100%',
    paddingVertical: px(50),
    paddingHorizontal: px(20),
    overflow: 'hidden',
  },
  logo: {
    color: colors.text,
    fontSize: px(64),
    fontWeight: '300',
    marginLeft: px(30),
  },
  railLogo: {
    marginLeft: px(4),
    fontSize: px(48),
  },
  logoAccent: {
    color: colors.accent,
    fontWeight: 'bold',
  },
  items: {
    flex: 1,
    justifyContent: 'center',
  },
  item: {
    height: px(ITEM_HEIGHT - 12),
    marginVertical: px(6),
    flexDirection: 'row',
    alignItems: 'center',
    paddingHorizontal: px(30),
    borderRadius: px(10),
    gap: px(24),
  },
  railItem: {
    paddingHorizontal: px(15),
  },
  highlighted: {
    backgroundColor: colors.pill,
  },
  open: {
    backgroundColor: colors.pillSoft,
  },
  label: {
    fontSize: fonts.large,
  },
});
