import type { ReactNode } from 'react';
import { StyleSheet, View, type StyleProp, type ViewStyle } from 'react-native';
import Svg, { Defs, LinearGradient, Rect, Stop } from 'react-native-svg';

// A panel with a soft diagonal gradient behind its children.
export function Panel({
  from,
  to,
  style,
  children,
}: {
  from: string;
  to: string;
  style?: StyleProp<ViewStyle>;
  children?: ReactNode;
}) {
  return (
    <View style={[{ backgroundColor: to }, style]}>
      {/* Redrawn whenever the panel's style changes: the gradient doesn't
          stretch when the panel resizes, as the menu does when it expands. */}
      <Svg
        key={JSON.stringify(StyleSheet.flatten(style))}
        style={StyleSheet.absoluteFill}
      >
        <Defs>
          <LinearGradient id="panel" x1="0" y1="0" x2="1" y2="1">
            <Stop offset="0" stopColor={from} />
            <Stop offset="1" stopColor={to} />
          </LinearGradient>
        </Defs>
        <Rect width="100%" height="100%" fill="url(#panel)" />
      </Svg>
      {children}
    </View>
  );
}
