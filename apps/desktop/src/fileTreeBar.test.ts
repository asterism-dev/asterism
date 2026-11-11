import { beforeEach, describe, expect, it, vi } from 'vitest';

const store = new Map<string, string>();

beforeEach(() => {
  store.clear();
  vi.resetModules();
  vi.stubGlobal('localStorage', {
    getItem: (k: string) => store.get(k) ?? null,
    setItem: (k: string, v: string) => void store.set(k, v),
  });
});

describe('file tree bar state', () => {
  it('round-trips open state per task', async () => {
    const { isTreeOpen, setTreeOpen } = await import('./fileTreeBar');
    expect(isTreeOpen(1)).toBe(false);
    setTreeOpen(1, true);
    expect(isTreeOpen(1)).toBe(true);
    expect(isTreeOpen(2)).toBe(false);
    expect(store.get('asterism.fileTree.1')).toBe('1');
  });

  it('restores open state from storage', async () => {
    store.set('asterism.fileTree.5', '1');
    const { isTreeOpen } = await import('./fileTreeBar');
    expect(isTreeOpen(5)).toBe(true);
  });

  it('clamps and persists the width', async () => {
    const { treeWidth, setTreeWidth } = await import('./fileTreeBar');
    expect(treeWidth.value).toBe(260);
    setTreeWidth(50);
    expect(treeWidth.value).toBe(180);
    setTreeWidth(9999);
    expect(treeWidth.value).toBe(600);
    expect(store.get('asterism.fileTree.width')).toBe('600');
  });
});
