import { toRaw } from 'vue';
import type { NodeEvent, Session, Subagent } from './types';

export type BridgeErrorCode = 'method_not_found' | 'permission_denied' | 'invalid_params';
export interface ThemeInfo {
  name: 'light' | 'dark';
  vars: Record<string, string>;
}
export interface PanelContext {
  taskId: number;
  permissions: string[];
  subscribed: boolean;
}
export interface BridgeDeps {
  sessions(taskId: number): Session[];
  subagents(sessionId: number): Promise<Subagent[]>;
  focus(session: Session): void;
  theme(): ThemeInfo;
}
export type BridgeResponse =
  | { id: unknown; result: unknown }
  | { id: unknown; error: { code: BridgeErrorCode; message: string } };

export const THEME_VARS = [
  '--bg',
  '--panel',
  '--text',
  '--muted',
  '--border',
  '--accent',
  '--select',
  '--danger',
  '--idle',
  '--waiting',
  '--exited',
];

/** Store sessions are reactive proxies, which postMessage cannot clone. */
export const plainSessions = (sessions: Session[]): Session[] => sessions.map((s) => toRaw(s));

/** The context of the registered frame whose current window sent a message, if any. */
export function frameFor(
  source: unknown,
  frames: Map<{ contentWindow: unknown }, PanelContext>,
): PanelContext | null {
  if (!source) return null;
  for (const [el, ctx] of frames) if (el.contentWindow === source) return ctx;
  return null;
}

const NEEDS_SESSIONS = new Set(['sessions.list', 'events.subscribe', 'ui.focusSession']);

export async function handleMessage(
  data: unknown,
  ctx: PanelContext,
  deps: BridgeDeps,
): Promise<BridgeResponse | null> {
  if (typeof data !== 'object' || data === null) return null;
  const { id, method, params } = data as { id?: unknown; method?: unknown; params?: unknown };
  if (id === undefined || typeof method !== 'string') return null;
  const fail = (code: BridgeErrorCode, message: string): BridgeResponse => ({
    id,
    error: { code, message },
  });
  if (method !== 'context' && !NEEDS_SESSIONS.has(method)) {
    return fail('method_not_found', `unknown method ${method}`);
  }
  if (NEEDS_SESSIONS.has(method) && !ctx.permissions.includes('ui:sessions')) {
    return fail('permission_denied', `${method} needs ui:sessions`);
  }
  const own = deps.sessions(ctx.taskId);
  switch (method) {
    case 'context':
      return { id, result: { taskId: ctx.taskId, theme: deps.theme(), protocol: 1 } };
    case 'sessions.list':
      return {
        id,
        result: await Promise.all(
          own.map(async (s) => ({ ...s, subagents: await deps.subagents(s.id).catch(() => []) })),
        ),
      };
    case 'events.subscribe':
      ctx.subscribed = true;
      return { id, result: null };
    default: {
      const sessionId = (params as { sessionId?: unknown } | undefined)?.sessionId;
      const session = own.find((s) => s.id === sessionId);
      if (!session) return fail('invalid_params', 'sessionId is not a session of this task');
      deps.focus(session);
      return { id, result: null };
    }
  }
}

export function panelEvent(
  event: NodeEvent,
  taskId: number,
  sessions: Session[],
): { event: string; data: unknown } | null {
  const ofTask = (sessionId: number) =>
    sessions.some((s) => s.id === sessionId && s.task_id === taskId);
  switch (event.method) {
    case 'session.changed':
      return event.params.task_id === taskId ? { event: event.method, data: event.params } : null;
    case 'session.status_changed':
    case 'session.removed':
    case 'subagent.started':
    case 'subagent.updated':
      return ofTask(event.params.session_id) ? { event: event.method, data: event.params } : null;
    default:
      return null;
  }
}
