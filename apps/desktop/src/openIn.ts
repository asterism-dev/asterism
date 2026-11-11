import { invoke } from '@tauri-apps/api/core';

export const OPEN_IN_CANDIDATES = [
  'Finder',
  'Terminal',
  'iTerm',
  'Ghostty',
  'Zed',
  'Visual Studio Code',
  'Cursor',
  'Xcode',
  'IntelliJ IDEA',
  'PyCharm',
  'WebStorm',
  'GoLand',
  'RustRover',
];

const KEY = 'asterism.openIn';

export function lastApp(available: string[]): string {
  try {
    const stored = localStorage.getItem(KEY);
    if (stored && available.includes(stored)) return stored;
  } catch {
    // Storage unavailable: use the default.
  }
  return 'Finder';
}

export function rememberApp(app: string) {
  try {
    localStorage.setItem(KEY, app);
  } catch {
    // Not remembering is harmless.
  }
}

export const fetchInstalledApps = () =>
  invoke<string[]>('installed_apps', { candidates: OPEN_IN_CANDIDATES });

export const openIn = (app: string, path: string) => invoke<void>('open_in', { app, path });
