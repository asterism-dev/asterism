import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';
import { RpcError } from '../api';
import type { NodeEvent, NodeStatus } from '../types';
import { UNAUTHORIZED, WsTransport, type SocketLike } from './wsTransport';

class FakeSocket implements SocketLike {
  readyState = 0;
  sent: Record<string, unknown>[] = [];
  onopen: (() => void) | null = null;
  onmessage: ((ev: { data: unknown }) => void) | null = null;
  onclose: ((ev: { code: number; reason: string }) => void) | null = null;
  send(data: string) {
    this.sent.push(JSON.parse(data));
  }
  close() {}
  open() {
    this.readyState = 1;
    this.onopen?.();
  }
  reply(id: number, result: unknown) {
    this.onmessage?.({ data: JSON.stringify({ jsonrpc: '2.0', id, result }) });
  }
  fail(id: number, kind: string, message: string) {
    this.onmessage?.({
      data: JSON.stringify({ jsonrpc: '2.0', id, error: { code: -1, message, data: { kind } } }),
    });
  }
  notify(method: string, params: unknown) {
    this.onmessage?.({ data: JSON.stringify({ jsonrpc: '2.0', method, params }) });
  }
  drop(code = 1006) {
    this.onclose?.({ code, reason: '' });
  }
  lastId(): number {
    return this.sent[this.sent.length - 1].id as number;
  }
}

let sockets: FakeSocket[];
let statuses: NodeStatus[];
let events: NodeEvent[];
let unauthorized: number;
const flush = async () => {
  for (let i = 0; i < 10; i++) await Promise.resolve();
};

function transport() {
  return new WsTransport({
    url: 'ws://mac:7420/ws',
    token: 'tok',
    createSocket: () => {
      const s = new FakeSocket();
      sockets.push(s);
      return s;
    },
    onStatus: (s) => statuses.push(s),
    onEvent: (e) => events.push(e),
    onUnauthorized: () => unauthorized++,
  });
}

async function connected() {
  const t = transport();
  t.start();
  const s = sockets[0];
  s.open();
  s.reply(s.lastId(), { daemon_version: '1' });
  await flush();
  s.reply(s.lastId(), null);
  await flush();
  return { t, s };
}

beforeEach(() => {
  vi.useFakeTimers({ toFake: ['setTimeout', 'clearTimeout'] });
  sockets = [];
  statuses = [];
  events = [];
  unauthorized = 0;
});
afterEach(() => vi.useRealTimers());

describe('WsTransport', () => {
  it('authenticates, says hello and subscribes before reporting connected', async () => {
    const { s } = await connected();
    expect(s.sent[0]).toEqual({ auth: 'tok' });
    expect(s.sent[1]).toMatchObject({ method: 'hello', params: { client_kind: 'app' } });
    expect(s.sent[2]).toMatchObject({ method: 'subscribe' });
    expect(statuses.map((x) => x.state)).toEqual(['connecting', 'connected']);
  });

  it('matches responses by id and maps errors to RpcError', async () => {
    const { t, s } = await connected();
    const a = t.call<number>('a', {});
    const idA = s.lastId();
    const b = t.call<number>('b', {});
    const idB = s.lastId();
    s.reply(idB, 2);
    s.fail(idA, 'not_found', 'gone');
    await expect(b).resolves.toBe(2);
    await expect(a).rejects.toEqual(new RpcError('not_found', 'gone'));
  });

  it('routes output to the attached session and other notifications to onEvent', async () => {
    const { t, s } = await connected();
    const seen: string[] = [];
    const attached = t.attach(5, (d) => seen.push(d));
    s.notify('session.output', { session_id: 5, data: 'b3V0' });
    s.notify('session.output', { session_id: 6, data: 'x' });
    s.reply(s.lastId(), { snapshot: '', rows: 1, cols: 1 });
    await attached;
    s.notify('task.removed', { task_id: 1 });
    expect(seen).toEqual(['b3V0']);
    expect(events).toEqual([{ method: 'task.removed', params: { task_id: 1 } }]);
  });

  it('rejects_calls_while_disconnected', async () => {
    const t = transport();
    t.start();
    await expect(t.call('a', {})).rejects.toMatchObject({ kind: 'disconnected' });
  });

  it('rejects_pending_calls_on_close and reconnects with backoff', async () => {
    const { t, s } = await connected();
    const pending = t.call('a', {});
    s.drop();
    await expect(pending).rejects.toMatchObject({ kind: 'disconnected' });
    expect(statuses[statuses.length - 1].state).toBe('disconnected');
    vi.advanceTimersByTime(999);
    expect(sockets).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(sockets).toHaveLength(2);
    sockets[1].drop();
    vi.advanceTimersByTime(1999);
    expect(sockets).toHaveLength(2);
    vi.advanceTimersByTime(1);
    expect(sockets).toHaveLength(3);
  });

  it('wake_skips_backoff', async () => {
    const { t, s } = await connected();
    s.drop();
    t.wake();
    expect(sockets).toHaveLength(2);
  });

  it('stops for good when the device is unauthorized', async () => {
    const { t, s } = await connected();
    s.drop(UNAUTHORIZED);
    expect(unauthorized).toBe(1);
    vi.advanceTimersByTime(60_000);
    t.wake();
    expect(sockets).toHaveLength(1);
  });

  it('reports an incompatible daemon instead of retrying', async () => {
    const t = transport();
    t.start();
    const s = sockets[0];
    s.open();
    s.fail(s.lastId(), 'incompatible_version', 'reload the page');
    await flush();
    expect(statuses[statuses.length - 1]).toEqual({
      state: 'incompatible',
      message: 'reload the page',
    });
  });
});
