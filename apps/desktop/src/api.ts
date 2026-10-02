import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  NodeStatus, Project, Session, SessionAttachResult, SessionKind, SessionReadResult, Task, TaskCreateResult,
  TaskDiffResult,
} from './types';

export class RpcError extends Error {
  constructor(public kind: string, message: string) {
    super(message);
  }
}

export function errorMessage(e: unknown): string {
  return e instanceof Error ? e.message : String(e);
}

export function decodeBase64(data: string): Uint8Array {
  return Uint8Array.from(atob(data), (c) => c.charCodeAt(0));
}

async function command<T>(name: string, args: Record<string, unknown> = {}): Promise<T> {
  try {
    return await invoke<T>(name, args);
  } catch (e) {
    const err = e as { kind?: string; message?: string };
    throw new RpcError(err.kind ?? 'internal', err.message ?? String(e));
  }
}

function call<T>(method: string, params: Record<string, unknown> = {}): Promise<T> {
  return command<T>('node_call', { method, params });
}

const queues = new Map<number, Promise<unknown>>();

// Attach/detach for one session must reach the node in call order.
function serialized<T>(sessionId: number, op: () => Promise<T>): Promise<T> {
  const run = (queues.get(sessionId) ?? Promise.resolve()).catch(() => {}).then(op);
  const tail = run.catch(() => {});
  queues.set(sessionId, tail);
  void tail.then(() => {
    if (queues.get(sessionId) === tail) queues.delete(sessionId);
  });
  return run;
}

export const api = {
  nodeStatus: () => command<NodeStatus>('node_status'),
  restartDaemon: () => command<void>('restart_daemon'),
  projects: () => call<Project[]>('project.list'),
  addProject: (path: string) => call<Project>('project.add', { path }),
  removeProject: (projectId: number) => call<null>('project.remove', { project_id: projectId }),
  tasks: () => call<Task[]>('task.list', { include_archived: false }),
  createTask: (p: { project_id: number; title: string; prompt: string | null; agent: string | null }) =>
    call<TaskCreateResult>('task.create', p),
  archiveTask: (taskId: number, force: boolean) => call<Task>('task.archive', { task_id: taskId, force }),
  diff: (taskId: number) => call<TaskDiffResult>('task.diff', { task_id: taskId }),
  sessions: () => call<Session[]>('session.list'),
  startSession: (taskId: number, kind: SessionKind) => call<Session>('session.start', { task_id: taskId, kind }),
  killSession: (sessionId: number) => call<null>('session.kill', { session_id: sessionId }),
  send: (sessionId: number, text: string) => call<null>('session.send', { session_id: sessionId, text, submit: false }),
  resize: (sessionId: number, rows: number, cols: number) =>
    call<null>('session.resize', { session_id: sessionId, rows, cols }),
  read: (sessionId: number, lines: number) => call<SessionReadResult>('session.read', { session_id: sessionId, lines }),
  attach: (sessionId: number, onOutput: Channel<string>) =>
    serialized(sessionId, () => command<SessionAttachResult>('session_attach', { sessionId, onOutput })),
  detach: (sessionId: number) => serialized(sessionId, () => command<void>('session_detach', { sessionId })),
};
