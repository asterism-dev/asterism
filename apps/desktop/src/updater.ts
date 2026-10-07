import { getVersion } from '@tauri-apps/api/app';
import { relaunch } from '@tauri-apps/plugin-process';
import { check, type Update } from '@tauri-apps/plugin-updater';
import { reactive } from 'vue';
import { errorMessage } from './api';
import { toast } from './store';

export const RELEASES_URL = 'https://github.com/asterism-dev/asterism/releases';
const CHECK_INTERVAL_MS = 6 * 60 * 60 * 1000;

export const updater = reactive({
  current: '',
  available: null as { version: string; notes: string } | null,
  checked: false,
  checking: false,
  installing: false,
  progress: null as number | null,
});

let pending: Update | null = null;

export async function checkForUpdates(manual = false) {
  if (updater.checking || updater.installing) return;
  updater.checking = true;
  try {
    pending = await check();
    updater.available = pending ? { version: pending.version, notes: pending.body ?? '' } : null;
    updater.checked = true;
  } catch (e) {
    if (manual) toast(`Update check failed: ${errorMessage(e)}`);
    else console.warn('update check failed', e);
  } finally {
    updater.checking = false;
  }
}

export async function installUpdate() {
  if (!pending || updater.installing) return;
  updater.installing = true;
  updater.progress = null;
  let total = 0;
  let received = 0;
  try {
    await pending.downloadAndInstall((e) => {
      if (e.event === 'Started') total = e.data.contentLength ?? 0;
      else if (e.event === 'Progress') {
        received += e.data.chunkLength;
        updater.progress = total ? Math.min(100, Math.round((received * 100) / total)) : null;
      }
    });
    await relaunch();
  } catch (e) {
    toast(`Update failed: ${errorMessage(e)}`);
    updater.installing = false;
    updater.progress = null;
  }
}

export function updateLabel(): string {
  if (updater.installing)
    return updater.progress === null ? 'Installing…' : `Installing… ${updater.progress}%`;
  return updater.available ? `Update to ${updater.available.version}` : '';
}

export async function startUpdater() {
  updater.current = await getVersion();
  if (import.meta.env.DEV) return;
  void checkForUpdates();
  setInterval(() => void checkForUpdates(), CHECK_INTERVAL_MS);
}
