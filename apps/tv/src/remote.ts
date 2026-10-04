// The remote control. The home screen moves its own highlight instead of
// using Android's focus, so it can scroll the guide in time and keep the
// layout exactly where it wants it.

import { useEffect, useRef } from 'react';
import { BackHandler, TVEventHandler } from 'react-native';

export type Key =
  | 'up'
  | 'down'
  | 'left'
  | 'right'
  | 'select'
  | 'longSelect'
  | 'back'
  | 'playPause'
  | 'info'
  | 'menu'
  | 'channelUp'
  | 'channelDown';

const KEYS = new Set<string>([
  'up',
  'down',
  'left',
  'right',
  'select',
  'playPause',
  'info',
  'menu',
  'channelUp',
  'channelDown',
]);

const LONG_ARROWS: Record<string, Key> = {
  longUp: 'up',
  longDown: 'down',
  longLeft: 'left',
  longRight: 'right',
};

// How often a held arrow repeats.
const REPEAT_MS = 90;

// Calls onKey for each press while `enabled`. For 'back', returning false
// lets Android handle it, which closes the app.
export function useRemote(onKey: (key: Key) => boolean | void, enabled = true) {
  const handler = useRef(onKey);
  handler.current = onKey;

  useEffect(() => {
    if (!enabled) {
      return;
    }
    let repeat: ReturnType<typeof setInterval> | undefined;
    const stopRepeat = () => {
      if (repeat !== undefined) {
        clearInterval(repeat);
        repeat = undefined;
      }
    };
    // Android TV only sends an arrow when it's let go, and a held arrow as
    // one longX when it starts (eventKeyAction 0) and one when it ends (1).
    // A held arrow repeats here instead, so long lists scroll. Any other
    // key stops it too, in case the end of the hold never arrives.
    const subscription = TVEventHandler.addListener(event => {
      const type = event.eventType;
      const arrow = LONG_ARROWS[type];
      stopRepeat();
      if (arrow) {
        if (event.eventKeyAction === 0) {
          handler.current(arrow);
          repeat = setInterval(() => handler.current(arrow), REPEAT_MS);
        }
      } else if (type === 'longSelect') {
        if (event.eventKeyAction === 0) {
          handler.current('longSelect');
        }
      } else if (KEYS.has(type) && event.eventKeyAction !== 0) {
        handler.current(type as Key);
      }
    });
    const back = BackHandler.addEventListener('hardwareBackPress', () => {
      stopRepeat();
      return handler.current('back') !== false;
    });
    return () => {
      stopRepeat();
      subscription?.remove();
      back.remove();
    };
  }, [enabled]);
}
