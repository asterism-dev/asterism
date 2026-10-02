import { reactive } from 'vue';
import { api } from './api';
import type { NodeEvent, NodeStatus, Project, Session, SessionStatus, Task } from './types';

export type Tab = 'diff' | number;
export interface MenuItem { label: string; action: () => void; danger?: boolean }
export interface Toast { id: number; message: string }

export interface State {
  node: NodeStatus;
  projects: Project[];
  tasks: Task[];
  sessions: Session[];
  selectedTaskId: number | null;
  selectedTab: Record<number, Tab>;
  tabOrder: Record<number, number[]>;
  toasts: Toast[];
  menu: { x: number; y: number; items: MenuItem[] } | null;
  newTaskFor: number | null;
  settingsOpen: boolean;
  settingsDirty: boolean;
}

export function initialState(): State {
  return {
    node: { state: 'connecting' },
    projects: [],
    tasks: [],
    sessions: [],
    selectedTaskId: null,
    selectedTab: {},
    tabOrder: {},
    toasts: [],
    menu: null,
    newTaskFor: null,
    settingsOpen: false,
    settingsDirty: false,
  };
}

export const state = reactive<State>(initialState());

const TOAST_MS = 6000;
const RANK: Record<SessionStatus, number> = { exited: 0, idle: 1, working: 2, waiting_input: 3 };

export function isConnected(node: NodeStatus): boolean {
  return node.state === 'connected' || node.state === 'update_available';
}

export function aggregate(statuses: SessionStatus[]): SessionStatus | null {
  let best: SessionStatus | null = null;
  for (const status of statuses) {
    if (best === null || RANK[status] > RANK[best]) best = status;
  }
  return best;
}

export function taskStatus(s: State, taskId: number): SessionStatus | null {
  return aggregate(s.sessions.filter((x) => x.task_id === taskId).map((x) => x.status));
}

export function projectStatus(s: State, projectId: number): SessionStatus | null {
  const ids = new Set(s.tasks.filter((t) => t.project_id === projectId).map((t) => t.id));
  return aggregate(s.sessions.filter((x) => ids.has(x.task_id)).map((x) => x.status));
}

export function nodeAggregateStatus(s: State): SessionStatus | null {
  const ids = new Set(s.tasks.map((t) => t.id));
  return aggregate(s.sessions.filter((x) => ids.has(x.task_id)).map((x) => x.status));
}

export function taskSessions(s: State, taskId: number): Session[] {
  const order = s.tabOrder[taskId] ?? [];
  const position = (id: number) => {
    const i = order.indexOf(id);
    return i === -1 ? Number.POSITIVE_INFINITY : i;
  };
  return s.sessions
    .filter((x) => x.task_id === taskId)
    .sort((a, b) => position(a.id) - position(b.id) || a.id - b.id);
}

/** The tab a task shows: the chosen one if it still exists, else its first live session, else the diff. */
export function activeTab(s: State, taskId: number): Tab {
  const sessions = taskSessions(s, taskId);
  const chosen = s.selectedTab[taskId];
  if (chosen === 'diff' || sessions.some((x) => x.id === chosen)) return chosen;
  return sessions.find((x) => x.status !== 'exited')?.id ?? sessions[0]?.id ?? 'diff';
}

export function moveTab(s: State, taskId: number, draggedId: number, targetId: number) {
  const ids = taskSessions(s, taskId).map((x) => x.id).filter((id) => id !== draggedId);
  const at = ids.indexOf(targetId);
  ids.splice(at === -1 ? ids.length : at, 0, draggedId);
  s.tabOrder[taskId] = ids;
}

export function waitingSessions(s: State): Session[] {
  const known = new Set(s.tasks.map((t) => t.id));
  return s.sessions.filter((x) => x.status === 'waiting_input' && known.has(x.task_id)).sort((a, b) => a.id - b.id);
}

export function nextWaiting(s: State): Session | null {
  const waiting = waitingSessions(s);
  if (!waiting.length) return null;
  const tab = s.selectedTaskId === null ? undefined : s.selectedTab[s.selectedTaskId];
  const current = typeof tab === 'number' ? tab : -1;
  return waiting.find((x) => x.id > current) ?? waiting[0];
}

export function selectSession(s: State, session: Session) {
  s.selectedTaskId = session.task_id;
  s.selectedTab[session.task_id] = session.id;
}

function upsert<T extends { id: number }>(list: T[], item: T) {
  const i = list.findIndex((x) => x.id === item.id);
  if (i === -1) list.push(item);
  else list[i] = item;
}

export function addTask(s: State, task: Task) {
  if (!task.archived) upsert(s.tasks, task);
}

export function addSession(s: State, session: Session) {
  upsert(s.sessions, session);
  selectSession(s, session);
}

function dropTask(s: State, taskId: number) {
  s.tasks = s.tasks.filter((t) => t.id !== taskId);
  s.sessions = s.sessions.filter((x) => x.task_id !== taskId);
  delete s.selectedTab[taskId];
  delete s.tabOrder[taskId];
  if (s.selectedTaskId === taskId) s.selectedTaskId = null;
}

/** Applies a daemon event; returns the session that just started waiting for the user, if any. */
export function applyEvent(s: State, event: NodeEvent): Session | null {
  switch (event.method) {
    case 'session.status_changed': {
      const session = s.sessions.find((x) => x.id === event.params.session_id);
      if (!session) return null;
      const started = session.status !== 'waiting_input' && event.params.status === 'waiting_input';
      session.status = event.params.status;
      return started ? session : null;
    }
    case 'session.changed':
      upsert(s.sessions, event.params);
      return null;
    case 'session.removed': {
      const removed = s.sessions.find((x) => x.id === event.params.session_id);
      if (!removed) return null;
      s.sessions = s.sessions.filter((x) => x.id !== removed.id);
      const order = s.tabOrder[removed.task_id];
      if (order) s.tabOrder[removed.task_id] = order.filter((id) => id !== removed.id);
      if (s.selectedTab[removed.task_id] === removed.id) delete s.selectedTab[removed.task_id];
      return null;
    }
    case 'task.changed':
      if (event.params.archived) dropTask(s, event.params.id);
      else upsert(s.tasks, event.params);
      return null;
    case 'project.changed':
      upsert(s.projects, event.params);
      return null;
    case 'project.removed':
      s.projects = s.projects.filter((p) => p.id !== event.params.project_id);
      for (const t of s.tasks.filter((t) => t.project_id === event.params.project_id)) dropTask(s, t.id);
      return null;
  }
}

export function showMenu(event: MouseEvent, items: MenuItem[]) {
  event.preventDefault();
  event.stopPropagation();
  state.menu = items.length ? { x: event.clientX, y: event.clientY, items } : null;
}

let nextToastId = 1;

export function toast(message: string) {
  const id = nextToastId++;
  state.toasts.push({ id, message });
  setTimeout(() => {
    state.toasts = state.toasts.filter((t) => t.id !== id);
  }, TOAST_MS);
}

export async function refresh() {
  const [projects, tasks, sessions] = await Promise.all([api.projects(), api.tasks(), api.sessions()]);
  state.projects = projects;
  state.tasks = tasks;
  state.sessions = sessions;
  if (state.selectedTaskId !== null && !tasks.some((t) => t.id === state.selectedTaskId)) state.selectedTaskId = null;
}
