import type { ReactNode } from 'react';
import { StyleSheet, Text, View } from 'react-native';
import { colors, fonts, px } from '../theme';
import { useFirstVisible } from './scroll';
import { Panel } from './Panel';

export const SETTINGS_WIDTH = 700;
const ROW_HEIGHT = 80;
const VISIBLE_ROWS = 10;

// The panel that slides over the right of the home screen.
export function SettingsPanel({
  title,
  children,
}: {
  title: string;
  children: ReactNode;
}) {
  return (
    <Panel from={colors.settings} to={colors.settingsDark} style={styles.panel}>
      <View style={styles.header}>
        <Text numberOfLines={1} style={styles.title}>
          {title}
        </Text>
      </View>
      {children}
    </Panel>
  );
}

// `toggles` gives each item an on/off switch, as in the Search settings.
export function SettingsList({
  items,
  index,
  note,
  toggles,
}: {
  items: string[];
  index: number;
  note?: string;
  toggles?: boolean[];
}) {
  const top = useFirstVisible(index, items.length, VISIBLE_ROWS);
  return (
    <>
      <View style={styles.list}>
        {items.slice(top, top + VISIBLE_ROWS).map((item, i) => {
          const focused = top + i === index;
          return (
            <View
              key={top + i}
              style={[styles.item, focused && styles.focused]}
            >
              <Text
                numberOfLines={2}
                style={[styles.label, focused && { color: colors.textDark }]}
              >
                {item}
              </Text>
              {toggles && (
                <View
                  style={[styles.track, toggles[top + i] && styles.trackOn]}
                >
                  <View
                    style={[
                      styles.knob,
                      toggles[top + i] ? styles.knobOn : styles.knobOff,
                    ]}
                  />
                </View>
              )}
            </View>
          );
        })}
      </View>
      {note != null && <Text style={styles.note}>{note}</Text>}
    </>
  );
}

const styles = StyleSheet.create({
  panel: {
    position: 'absolute',
    top: 0,
    right: 0,
    bottom: 0,
    width: px(SETTINGS_WIDTH),
  },
  header: {
    height: px(170),
    justifyContent: 'center',
    paddingHorizontal: px(50),
    backgroundColor: 'rgba(0, 0, 0, 0.06)',
  },
  title: {
    color: colors.text,
    fontSize: fonts.title,
    fontWeight: 'bold',
  },
  list: {
    paddingHorizontal: px(16),
    paddingTop: px(14),
  },
  item: {
    minHeight: px(ROW_HEIGHT - 6),
    marginVertical: px(3),
    flexDirection: 'row',
    alignItems: 'center',
    gap: px(24),
    paddingHorizontal: px(34),
    borderRadius: px(10),
  },
  focused: {
    backgroundColor: colors.pill,
  },
  label: {
    flex: 1,
    color: colors.text,
    fontSize: fonts.normal,
  },
  track: {
    width: px(64),
    height: px(30),
    borderRadius: px(15),
    justifyContent: 'center',
    backgroundColor: 'rgba(255, 255, 255, 0.35)',
  },
  trackOn: {
    backgroundColor: 'rgba(62, 166, 242, 0.45)',
  },
  knob: {
    width: px(38),
    height: px(38),
    borderRadius: px(19),
  },
  knobOff: {
    backgroundColor: '#eeeeee',
  },
  knobOn: {
    alignSelf: 'flex-end',
    backgroundColor: colors.accent,
  },
  note: {
    position: 'absolute',
    left: px(50),
    right: px(50),
    bottom: px(50),
    color: colors.text,
    fontSize: fonts.small,
  },
});
