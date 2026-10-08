import { describe, expect, it, vi } from 'vitest';
import { reactive } from 'vue';
import {
  handleMessage,
  panelEvent,
  plainSessions,
  type BridgeDeps,
  type PanelContext,
} from './pluginBridge';
import { initialState, taskSessions } from './store';
import type { Session, Subagent } from './types';

const session = (id: number, task_id: number): Session => ({
  id,
  task_id,
  kind: { type: 'agent', name: 'claude' },
  status: 'working',
});
const sub: Subagent = {
  id: 't1',
  parent_id: null,
  kind: 'Explore',
  description: 'd',
  status: 'running',
  started_at: 1,
  ended_at: null,
};
const sessions = [session(1, 10), session(2, 20)];
const deps = (): BridgeDeps => ({
  sessions: (taskId) => sessions.filter((s) => s.task_id === taskId),
  subagents: vi.fn(async () => [sub]),
  focus: vi.fn(),
  theme: () => ({ name: 'dark', vars: { '--bg': '#000' } }),
});
const ctx = (over: Partial<PanelContext> = {}): PanelContext => ({
  taskId: 10,
  permissions: ['ui:sessions'],
  subscribed: false,
  ...over,
});

describe('handleMessage', () => {
  it('answers context without permissions', async () => {
    expect(
      await handleMessage({ id: 1, method: 'context' }, ctx({ permissions: [] }), deps()),
    ).toEqual({
      id: 1,
      result: { taskId: 10, theme: { name: 'dark', vars: { '--bg': '#000' } }, protocol: 1 },
    });
  });

  it('lists only the task sessions with their subagents', async () => {
    const res = await handleMessage({ id: 2, method: 'sessions.list' }, ctx(), deps());
    expect(res).toEqual({ id: 2, result: [{ ...session(1, 10), subagents: [sub] }] });
  });

  it('lists sessions from the reactive store in a postable form', async () => {
    const st = reactive({ ...initialState(), sessions: [session(1, 10)] });
    const d = { ...deps(), sessions: (taskId: number) => plainSessions(taskSessions(st, taskId)) };
    const res = await handleMessage({ id: 2, method: 'sessions.list' }, ctx(), d);
    expect(structuredClone(res)).toEqual({
      id: 2,
      result: [{ ...session(1, 10), subagents: [sub] }],
    });
  });

  it('lists a session with no subagents when fetching them fails', async () => {
    const d = { ...deps(), subagents: vi.fn(async () => Promise.reject(new Error('gone'))) };
    const res = await handleMessage({ id: 2, method: 'sessions.list' }, ctx(), d);
    expect(res).toEqual({ id: 2, result: [{ ...session(1, 10), subagents: [] }] });
  });

  it('checks permissions, methods and params', async () => {
    const d = deps();
    const code = async (data: unknown, c = ctx()) => {
      const res = await handleMessage(data, c, d);
      return res && 'error' in res ? res.error.code : null;
    };
    expect(await code({ id: 3, method: 'sessions.list' }, ctx({ permissions: [] }))).toBe(
      'permission_denied',
    );
    expect(await code({ id: 4, method: 'fs.read' })).toBe('method_not_found');
    expect(await code({ id: 5, method: 'ui.focusSession', params: { sessionId: 2 } })).toBe(
      'invalid_params',
    );
    expect(await code({ id: 6, method: 'ui.focusSession', params: {} })).toBe('invalid_params');
    expect(d.focus).not.toHaveBeenCalled();
    expect(await code({ id: 7, method: 'ui.focusSession', params: { sessionId: 1 } })).toBe(null);
    expect(d.focus).toHaveBeenCalledWith(sessions[0]);
  });

  it('marks the panel subscribed', async () => {
    const c = ctx();
    expect(await handleMessage({ id: 8, method: 'events.subscribe' }, c, deps())).toEqual({
      id: 8,
      result: null,
    });
    expect(c.subscribed).toBe(true);
  });

  it('ignores anything that is not a call', async () => {
    for (const data of [null, 'x', { method: 'context' }, { id: 1 }, { id: 1, method: 3 }]) {
      expect(await handleMessage(data, ctx(), deps())).toBeNull();
    }
  });
});

describe('panelEvent', () => {
  it('forwards session and subagent events of the task only', () => {
    expect(
      panelEvent(
        { method: 'subagent.started', params: { session_id: 1, subagent: sub } },
        10,
        sessions,
      ),
    ).toEqual({ event: 'subagent.started', data: { session_id: 1, subagent: sub } });
    expect(
      panelEvent(
        { method: 'subagent.started', params: { session_id: 2, subagent: sub } },
        10,
        sessions,
      ),
    ).toBeNull();
    expect(
      panelEvent({ method: 'session.changed', params: session(1, 10) }, 10, sessions)?.event,
    ).toBe('session.changed');
    expect(
      panelEvent({ method: 'session.removed', params: { session_id: 1 } }, 10, sessions)?.event,
    ).toBe('session.removed');
    expect(panelEvent({ method: 'plugins.changed', params: {} }, 10, sessions)).toBeNull();
  });
});
