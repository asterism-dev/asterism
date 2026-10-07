import { beforeEach, describe, expect, it, vi } from 'vitest';

const invoke = vi.fn();
vi.mock('@tauri-apps/api/core', () => ({
  invoke: (...a: unknown[]) => invoke(...a),
  Channel: class {},
}));

const { api } = await import('./api');

type Deferred = { resolve: (v?: unknown) => void; reject: (e: unknown) => void };
let pending: Deferred[];
const tick = () => new Promise((r) => setTimeout(r, 0));
const names = () => invoke.mock.calls.map((c) => c[0]);

beforeEach(() => {
  pending = [];
  invoke.mockReset();
  invoke.mockImplementation(
    () => new Promise((resolve, reject) => pending.push({ resolve, reject })),
  );
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

describe('send ordering', () => {
  const texts = () =>
    invoke.mock.calls.map((c) => (c[1] as { params: { text: string } }).params.text);

  it('keeps one send in flight and coalesces text typed meanwhile', async () => {
    const a = api.send(3, 'a');
    const b = api.send(3, 'b');
    const c = api.send(3, 'c');
    await tick();
    expect(texts()).toEqual(['a']);
    pending[0].reject({ kind: 'internal', message: 'boom' });
    await tick();
    expect(texts()).toEqual(['a', 'bc']);
    const d = api.send(3, 'd');
    pending[1].resolve(null);
    await tick();
    expect(texts()).toEqual(['a', 'bc', 'd']);
    pending[2].resolve(null);
    await Promise.all([a, b, c, d]);
    api.send(3, 'e');
    await tick();
    expect(texts()).toEqual(['a', 'bc', 'd', 'e']);
    pending[3].resolve(null);
  });
});
