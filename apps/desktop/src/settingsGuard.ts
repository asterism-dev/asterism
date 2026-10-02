import { ask } from '@tauri-apps/plugin-dialog';
import { state } from './store';

/** Closes Settings unless the user wants to keep unsaved changes; returns whether Settings were left. */
export async function leaveSettings(): Promise<boolean> {
  if (!state.settingsOpen) return true;
  if (state.settingsDirty) {
    const discard = await ask('Discard unsaved settings changes?', { title: 'Settings', kind: 'warning' });
    if (!discard) return false;
  }
  state.settingsDirty = false;
  state.settingsOpen = false;
  return true;
}
