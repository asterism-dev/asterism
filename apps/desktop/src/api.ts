import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  AgentConfig, AgentConfigRaw, GithubRepo, GithubStatus, GithubTarget, NodeConfig, NodeConfigInfo, NodeStats, NodeStatus, Project, ProjectBranches, ProjectCreateResult, Session, SessionAttachResult, SessionKind,
  SessionReadResult, Task, TaskCreateResult, TaskDeleteCheck, TaskDeleteResult, TaskDiffResult, Worktree, WorktreeSize,
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

const sendBuffers = new Map<number, string>();

// Each keystroke is its own invoke and the daemon serves requests concurrently, so one send per session is in flight.
async function send(sessionId: number, text: string): Promise<void> {
  const buffered = sendBuffers.get(sessionId);
  if (buffered !== undefined) {
    sendBuffers.set(sessionId, buffered + text);
    return;
  }
  let next = text;
  while (next) {
    sendBuffers.set(sessionId, '');
    await call<null>('session.send', { session_id: sessionId, text: next, submit: false }).catch(() => {});
    next = sendBuffers.get(sessionId) ?? '';
  }
  sendBuffers.delete(sessionId);
}

export const api = {
  nodeStatus: () => command<NodeStatus>('node_status'),
  restartDaemon: () => command<void>('restart_daemon'),
  appPid: () => command<number>('app_pid'),
  projects: () => call<Project[]>('project.list'),
  addProject: (path: string) => call<Project>('project.add', { path }),
  removeProject: (projectId: number) => call<null>('project.remove', { project_id: projectId }),
  tasks: () => call<Task[]>('task.list', { include_archived: false }),
  allTasks: () => call<Task[]>('task.list', { include_archived: true }),
  createTask: (p: { project_id: number; title: string; prompt: string | null; agent: string | null; base: string | null }) =>
    call<TaskCreateResult>('task.create', p),
  projectBranches: (projectId: number) => call<ProjectBranches>('project.branches', { project_id: projectId }),
  updateProject: (projectId: number, defaultBase: string | null) =>
    call<Project>('project.update', { project_id: projectId, default_base: defaultBase }),
  archiveTask: (taskId: number) => call<Task>('task.archive', { task_id: taskId }),
  restoreTask: (taskId: number) => call<Task>('task.restore', { task_id: taskId }),
  deleteCheck: (taskId: number) => call<TaskDeleteCheck>('task.delete_check', { task_id: taskId }),
  deleteTask: (taskId: number, deleteBranch: boolean) => call<TaskDeleteResult>('task.delete', { task_id: taskId, delete_branch: deleteBranch }),
  projectTasks: (projectId: number) => call<Task[]>('task.list', { project_id: projectId, include_archived: true }),
  worktrees: (projectId: number) => call<Worktree[]>('project.worktrees', { project_id: projectId }),
  worktreeSizes: (projectId: number) => call<WorktreeSize[]>('project.worktree_sizes', { project_id: projectId }),
  removeWorktree: (projectId: number, path: string) => call<void>('project.worktree_remove', { project_id: projectId, path }),
  pruneWorktrees: (projectId: number) => call<void>('project.worktree_prune', { project_id: projectId }),
  diff: (taskId: number) => call<TaskDiffResult>('task.diff', { task_id: taskId }),
  sessions: () => call<Session[]>('session.list'),
  startSession: (taskId: number, kind: SessionKind) => call<Session>('session.start', { task_id: taskId, kind }),
  killSession: (sessionId: number) => call<null>('session.kill', { session_id: sessionId }),
  send,
  resize: (sessionId: number, rows: number, cols: number) =>
    call<null>('session.resize', { session_id: sessionId, rows, cols }),
  read: (sessionId: number, lines: number) => call<SessionReadResult>('session.read', { session_id: sessionId, lines }),
  attach: (sessionId: number, onOutput: Channel<string>) =>
    serialized(sessionId, () => command<SessionAttachResult>('session_attach', { sessionId, onOutput })),
  detach: (sessionId: number) => serialized(sessionId, () => command<void>('session_detach', { sessionId })),
  agentConfig: (agent: string) => call<AgentConfig>('agent_config.get', { agent }),
  agentConfigRaw: (agent: string) => call<AgentConfigRaw>('agent_config.get_raw', { agent }),
  setAgentConfig: (agent: string, config: AgentConfig) => call<null>('agent_config.set', { agent, config }),
  removeSession: (sessionId: number) => call<null>('session.remove', { session_id: sessionId }),
  nodeConfig: () => call<NodeConfigInfo>('node_config.get'),
  nodeStats: (pids: number[]) => call<NodeStats>('node.stats', { pids }),
  setNodeConfig: (config: NodeConfig) => call<null>('node_config.set', { config }),
  githubStatus: () => call<GithubStatus>('github.status'),
  githubRepos: (owner: string) => call<GithubRepo[]>('github.repos', { owner }),
  cloneProject: (source: string) => call<Project>('project.clone', { source }),
  createProject: (name: string, github: GithubTarget | null) =>
    call<ProjectCreateResult>('project.create', { name, github }),
};
