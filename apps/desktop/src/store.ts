import { reactive } from 'vue';
import { api } from './api';
import { updateCount } from './pluginsView';
import type { AgentInfo, NodeEvent, NodeStatus, PrList, Project, PullRequest, Session, SessionStatus, Task } from './types';

export type Tab = number | null;
export type ProjectDialogTab = 'folder' | 'clone' | 'create';
export interface MenuItem { label: string; action: () => void; danger?: boolean; checked?: boolean }
export interface Toast { id: number; message: string }

export interface State {
  node: NodeStatus;
  projects: Project[];
  tasks: Task[];
  sessions: Session[];
  agents: AgentInfo[];
  pluginsVersion: number;
  storesVersion: number;
  pluginUpdates: number;
  selectedTaskId: number | null;
  selectedTab: Record<number, number>;
  tabOrder: Record<number, number[]>;
  toasts: Toast[];
  menu: { x: number; y: number; items: MenuItem[] } | null;
  newTaskFor: number | null;
  settingsOpen: boolean;
  pluginSettingsRequest: string | null;
  settingsDirty: boolean;
  projectPage: number | null;
  tasksVersion: number;
  collapsed: Record<number, boolean>;
  projectDialog: ProjectDialogTab | null;
  prs: Record<number, PullRequest>;
  prErrors: Record<number, string>;
}

export function initialState(): State {
  return {
    node: { state: 'connecting' },
    projects: [],
    tasks: [],
    sessions: [],
    agents: [],
    pluginsVersion: 0,
    storesVersion: 0,
    pluginUpdates: 0,
    selectedTaskId: null,
    selectedTab: {},
    tabOrder: {},
    toasts: [],
    menu: null,
    newTaskFor: null,
    settingsOpen: false,
    pluginSettingsRequest: null,
    settingsDirty: false,
    projectPage: null,
    tasksVersion: 0,
    collapsed: {},
    projectDialog: null,
    prs: {},
    prErrors: {},
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
  if (sessions.some((x) => x.id === chosen)) return chosen;
  return sessions.find((x) => x.status !== 'exited')?.id ?? sessions[0]?.id ?? null;
}

export function waitingSessions(s: State): Session[] {
  const known = new Set(s.tasks.map((t) => t.id));
  return s.sessions.filter((x) => x.status === 'waiting_input' && known.has(x.task_id)).sort((a, b) => a.id - b.id);
}

export function nextWaiting(s: State): Session | null {
  const waiting = waitingSessions(s);
  if (!waiting.length) return null;
  const tab = s.selectedTaskId === null ? undefined : s.selectedTab[s.selectedTaskId];
  const current = tab ?? -1;
  return waiting.find((x) => x.id > current) ?? waiting[0];
}

export function sessionLabel(session: Session): string {
  if (session.kind.type === 'agent') return session.kind.name;
  if (session.kind.type === 'shell') return 'shell';
  return session.kind.argv[0] ?? 'command';
}

export function selectSession(s: State, session: Session) {
  s.projectPage = null;
  s.selectedTaskId = session.task_id;
  s.selectedTab[session.task_id] = session.id;
  const projectId = s.tasks.find((t) => t.id === session.task_id)?.project_id;
  if (projectId !== undefined) delete s.collapsed[projectId];
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
    case 'plugins.changed':
      s.pluginsVersion++;
      return null;
    case 'stores.changed':
      s.storesVersion++;
      return null;
    case 'pr.changed':
      if (event.params.pr) s.prs[event.params.task_id] = event.params.pr;
      else delete s.prs[event.params.task_id];
      return null;
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
      s.tasksVersion++;
      if (event.params.archived) dropTask(s, event.params.id);
      else upsert(s.tasks, event.params);
      return null;
    case 'task.removed':
      s.tasksVersion++;
      dropTask(s, event.params.task_id);
      return null;
    case 'project.changed':
      upsert(s.projects, event.params);
      return null;
    case 'project.removed':
      if (s.projectPage === event.params.project_id) s.projectPage = null;
      s.projects = s.projects.filter((p) => p.id !== event.params.project_id);
      for (const t of s.tasks.filter((t) => t.project_id === event.params.project_id)) dropTask(s, t.id);
      return null;
  }
}

/** A project list replaces only that project's error; the full list (`null`) replaces everything. */
export function applyPrList(s: State, list: PrList, projectId: number | null) {
  if (projectId === null) {
    s.prs = {};
    s.prErrors = {};
  } else {
    delete s.prErrors[projectId];
  }
  for (const p of list.prs) s.prs[p.task_id] = p.pr;
  for (const e of list.errors) s.prErrors[e.project_id] = e.message;
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

/** Daemons without plugin stores leave the badge at zero. */
export async function refreshPluginUpdates() {
  state.pluginUpdates = updateCount(await api.plugins());
}

export async function refresh() {
  const [projects, tasks, sessions, agents] = await Promise.all([api.projects(), api.tasks(), api.sessions(), api.agents()]);
  state.agents = agents;
  state.projects = projects;
  state.tasks = tasks;
  state.sessions = sessions;
  refreshPluginUpdates().catch(() => {});
  api.prList().then((l) => applyPrList(state, l, null)).catch(() => {});
  if (state.selectedTaskId !== null && !tasks.some((t) => t.id === state.selectedTaskId)) state.selectedTaskId = null;
}
