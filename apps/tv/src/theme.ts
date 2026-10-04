import { Dimensions } from 'react-native';

// Every size in the app is designed for a 1920×1080 screen and scaled to the
// real one with px(), so the layout looks the same on any TV.
const scale = Dimensions.get('window').width / 1920;

export function px(size: number): number {
  return Math.round(size * scale * 2) / 2;
}

export const colors = {
  background: '#2a3a4f',
  backgroundDark: '#1f2b3b',
  menu: '#4e6079',
  menuDark: '#3c4c63',
  groups: '#304056',
  settings: '#7d9ccc',
  settingsDark: '#5d7cae',
  text: '#ffffff',
  textDim: 'rgba(255, 255, 255, 0.65)',
  textDark: '#33445a',
  accent: '#3ea6f2',
  // The focused item.
  pill: 'rgba(255, 255, 255, 0.94)',
  // The chosen item while focus is somewhere else.
  pillSoft: 'rgba(255, 255, 255, 0.2)',
  block: 'rgba(255, 255, 255, 0.09)',
  blockFocusedRow: 'rgba(255, 255, 255, 0.17)',
  blockPlaying: 'rgba(255, 255, 255, 0.32)',
  line: 'rgba(255, 255, 255, 0.22)',
  error: '#ff8a80',
};

export const fonts = {
  small: px(26),
  normal: px(32),
  large: px(36),
  title: px(46),
};
