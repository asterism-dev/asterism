import { beforeEach, describe, expect, it, vi } from 'vitest';

const mocks = vi.hoisted(() => ({ check: vi.fn(), relaunch: vi.fn(), toast: vi.fn() }));
vi.mock('@tauri-apps/plugin-updater', () => ({ check: mocks.check }));
vi.mock('@tauri-apps/plugin-process', () => ({ relaunch: mocks.relaunch }));
vi.mock('@tauri-apps/api/app', () => ({ getVersion: async () => '0.3.0' }));
vi.mock('./store', () => ({ toast: mocks.toast }));

const { checkForUpdates, installUpdate, updateLabel, updater } = await import('./updater');

type Event = { event: 'Started'; data: { contentLength?: number } } | { event: 'Progress'; data: { chunkLength: number } } | { event: 'Finished' };

function fakeUpdate(download: (onEvent: (e: Event) => void) => Promise<void> = async () => {}) {
  return { version: '0.4.0', body: 'Fixes', downloadAndInstall: vi.fn(download) };
}

beforeEach(() => {
  vi.clearAllMocks();
  Object.assign(updater, { current: '0.3.0', available: null, checked: false, checking: false, installing: false, progress: null });
});

describe('checkForUpdates', () => {
  it('records an available update', async () => {
    mocks.check.mockResolvedValue(fakeUpdate());
    await checkForUpdates();
    expect(updater.available).toEqual({ version: '0.4.0', notes: 'Fixes' });
    expect(updater.checked).toBe(true);
    expect(updateLabel()).toBe('Update to 0.4.0');
  });

  it('clears the update when none is available', async () => {
    mocks.check.mockResolvedValue(null);
    await checkForUpdates(true);
    expect(updater.available).toBeNull();
    expect(updater.checked).toBe(true);
  });

  it('stays silent when an automatic check fails', async () => {
    mocks.check.mockRejectedValue(new Error('offline'));
    vi.spyOn(console, 'warn').mockImplementation(() => {});
    await checkForUpdates();
    expect(mocks.toast).not.toHaveBeenCalled();
    expect(updater.checking).toBe(false);
  });

  it('reports a failed manual check', async () => {
    mocks.check.mockRejectedValue(new Error('offline'));
    await checkForUpdates(true);
    expect(mocks.toast).toHaveBeenCalledWith('Update check failed: offline');
  });

  it('does not check while installing', async () => {
    updater.installing = true;
    await checkForUpdates();
    expect(mocks.check).not.toHaveBeenCalled();
  });
});

describe('installUpdate', () => {
  it('reports progress and relaunches', async () => {
    const seen: (number | null)[] = [];
    mocks.check.mockResolvedValue(fakeUpdate(async (on) => {
      on({ event: 'Started', data: { contentLength: 200 } });
      on({ event: 'Progress', data: { chunkLength: 50 } });
      seen.push(updater.progress);
      on({ event: 'Progress', data: { chunkLength: 150 } });
      seen.push(updater.progress);
      on({ event: 'Finished' });
    }));
    await checkForUpdates();
    await installUpdate();
    expect(seen).toEqual([25, 100]);
    expect(mocks.relaunch).toHaveBeenCalledOnce();
  });

  it('shows no percentage without a content length', async () => {
    let label = '';
    mocks.check.mockResolvedValue(fakeUpdate(async (on) => {
      on({ event: 'Started', data: {} });
      on({ event: 'Progress', data: { chunkLength: 50 } });
      label = updateLabel();
    }));
    await checkForUpdates();
    await installUpdate();
    expect(label).toBe('Installing…');
  });

  it('ignores a second click while installing', async () => {
    let finish!: () => void;
    const update = fakeUpdate(() => new Promise<void>((r) => (finish = r)));
    mocks.check.mockResolvedValue(update);
    await checkForUpdates();
    const first = installUpdate();
    await installUpdate();
    finish();
    await first;
    expect(update.downloadAndInstall).toHaveBeenCalledOnce();
  });

  it('recovers from a failed install', async () => {
    mocks.check.mockResolvedValue(fakeUpdate(async () => { throw new Error('bad signature'); }));
    await checkForUpdates();
    await installUpdate();
    expect(mocks.toast).toHaveBeenCalledWith('Update failed: bad signature');
    expect(updater.installing).toBe(false);
    expect(updater.progress).toBeNull();
    expect(mocks.relaunch).not.toHaveBeenCalled();
    expect(updateLabel()).toBe('Update to 0.4.0');
  });
});
