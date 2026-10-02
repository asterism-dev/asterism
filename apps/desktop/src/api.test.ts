import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({ invoke: (...a: unknown[]) => invoke(...a), Channel: class {} }));

const { api } = await import('./api');

type Deferred = { resolve: (v?: unknown) => void; reject: (e: unknown) => void };
let pending: Deferred[];
const tick = () => new Promise((r) => setTimeout(r, 0));
const names = () => invoke.mock.calls.map((c) => c[0]);

beforeEach(() => {
  pending = [];
  invoke.mockReset();
  invoke.mockImplementation(() => new Promise((resolve, reject) => pending.push({ resolve, reject })));
});

describe('attach/detach ordering', () => {
  it('sends operations for one session in call order', async () => {
    const a = api.attach(1, {} as never);
    const d = api.detach(1);
    const b = api.attach(1, {} as never);
    await tick();
    expect(names()).toEqual(['session_attach']);
    pending[0].resolve({});
    await tick();
    expect(names()).toEqual(['session_attach', 'session_detach']);
    pending[1].resolve();
    await tick();
    expect(names()).toEqual(['session_attach', 'session_detach', 'session_attach']);
    pending[2].resolve({});
    await Promise.all([a, d, b]);
  });

  it('does not block on a rejected operation', async () => {
    const a = api.attach(2, {} as never);
    const d = api.detach(2);
    await tick();
    pending[0].reject({ kind: 'not_found', message: 'gone' });
    await expect(a).rejects.toThrow('gone');
    await tick();
    expect(names()).toEqual(['session_attach', 'session_detach']);
    pending[1].resolve();
    await d;
  });
});
