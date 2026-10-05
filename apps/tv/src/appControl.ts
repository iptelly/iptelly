// The app's own Android module (android/.../AppControlModule.kt): whether
// the app may "display over other apps", which it needs to start by itself
// after boot or wake-up, and the system screen that allows it.

import { NativeModules } from 'react-native';

type AppControl = {
  canDrawOverlays(): Promise<boolean>;
  openOverlaySettings(): Promise<boolean>;
};

const module = NativeModules.AppControl as AppControl | undefined;

export function canDrawOverlays(): Promise<boolean> {
  return module?.canDrawOverlays() ?? Promise.resolve(true);
}

// Resolves false on TVs without the screen.
export function openOverlaySettings(): Promise<boolean> {
  return module?.openOverlaySettings() ?? Promise.resolve(false);
}
