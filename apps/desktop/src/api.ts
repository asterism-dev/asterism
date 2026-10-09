import { Channel, invoke } from '@tauri-apps/api/core';
import type {
  AgentConfig,
  AgentConfigRaw,
  AgentInfo,
  CommentTarget,
  DiffSide,
  CapabilityKind,
  PluginDetails,
  PluginInfo,
  SearchHit,
  StoreInfo,
  StoreList,
  Subagent,
  PluginSettings,
  SettingValue,
  ForgeInfo,
  ForgeRepo,
  ForgeStatus,
  NodeConfig,
  NodeConfigInfo,
  NodeStats,
  NodeStatus,
  PrList,
  PrListState,
  PrSearchResult,
  Project,
  ProjectBranches,
  ProjectCreateResult,
  RemoteTarget,
  ReviewEvent,
  ReviewResult,
  ReviewSource,
  Session,
  SessionAttachResult,
  SessionKind,
  SessionReadResult,
  IssueDetails,
  IssueHit,
  Task,
  TaskCreateRequest,
  TaskCreateResult,
  TaskSourceInfo,
  TaskDeleteCheck,
  TaskDeleteResult,
  TaskDiffResult,
  TaskFileResult,
  Worktree,
  WorktreeSize,
} from './types';

export class RpcError extends Error {
  constructor(
    public kind: string,
    message: string,
  ) {
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
    await call<null>('session.send', { session_id: sessionId, text: next, submit: false }).catch(
      () => {},
    );
    next = sendBuffers.get(sessionId) ?? '';
  }
  sendBuffers.delete(sessionId);
}

export const api = {
  subagents: (sessionId: number) =>
    call<Subagent[]>('session.subagents', { session_id: sessionId }),
  nodeStatus: () => command<NodeStatus>('node_status'),
  restartDaemon: () => command<void>('restart_daemon'),
  appPid: () => command<number>('app_pid'),
  quit: (stopDaemon: boolean) => command<void>('quit', { stopDaemon }),
  projects: () => call<Project[]>('project.list'),
  addProject: (path: string) => call<Project>('project.add', { path }),
  removeProject: (projectId: number) => call<null>('project.remove', { project_id: projectId }),
  tasks: () => call<Task[]>('task.list', { include_archived: false }),
  allTasks: () => call<Task[]>('task.list', { include_archived: true }),
  createTask: (p: TaskCreateRequest) => call<TaskCreateResult>('task.create', p),
  searchPullRequests: (projectId: number, query: string, state: PrListState) =>
    call<PrSearchResult>('pr.search', { project_id: projectId, query, state }),
  taskSources: (projectId: number) =>
    call<TaskSourceInfo[]>('task_source.list', { project_id: projectId }),
  searchIssues: (projectId: number, source: string, query: string, assignedToMe: boolean) =>
    call<IssueHit[]>('task_source.search', {
      project_id: projectId,
      source,
      query,
      assigned_to_me: assignedToMe,
    }),
  getIssue: (projectId: number, source: string, key: string) =>
    call<IssueDetails>('task_source.get', { project_id: projectId, source, key }),
  projectBranches: (projectId: number) =>
    call<ProjectBranches>('project.branches', { project_id: projectId }),
  updateProject: (projectId: number, defaultBase: string | null) =>
    call<Project>('project.update', { project_id: projectId, default_base: defaultBase }),
  archiveTask: (taskId: number) => call<Task>('task.archive', { task_id: taskId }),
  restoreTask: (taskId: number) => call<Task>('task.restore', { task_id: taskId }),
  deleteCheck: (taskId: number) => call<TaskDeleteCheck>('task.delete_check', { task_id: taskId }),
  deleteTask: (taskId: number, deleteBranch: boolean) =>
    call<TaskDeleteResult>('task.delete', { task_id: taskId, delete_branch: deleteBranch }),
  prList: (projectId?: number) => call<PrList>('pr.list', { project_id: projectId ?? null }),
  refreshPrs: (projectId: number) => call<PrList>('pr.refresh', { project_id: projectId }),
  projectTasks: (projectId: number) =>
    call<Task[]>('task.list', { project_id: projectId, include_archived: true }),
  worktrees: (projectId: number) =>
    call<Worktree[]>('project.worktrees', { project_id: projectId }),
  worktreeSizes: (projectId: number) =>
    call<WorktreeSize[]>('project.worktree_sizes', { project_id: projectId }),
  removeWorktree: (projectId: number, path: string) =>
    call<void>('project.worktree_remove', { project_id: projectId, path }),
  pruneWorktrees: (projectId: number) =>
    call<void>('project.worktree_prune', { project_id: projectId }),
  diff: (taskId: number) => call<TaskDiffResult>('task.diff', { task_id: taskId }),
  file: (taskId: number, path: string, knownMtime: number | null = null) =>
    call<TaskFileResult>('task.file', { task_id: taskId, path, known_mtime: knownMtime }),
  sessions: () => call<Session[]>('session.list'),
  startSession: (taskId: number, kind: SessionKind, prompt?: string) =>
    call<Session>('session.start', { task_id: taskId, kind, prompt: prompt ?? null }),
  sendPrompt: (sessionId: number, text: string) =>
    call<null>('session.send', { session_id: sessionId, text, submit: true }),
  review: (taskId: number, source: ReviewSource) =>
    call<ReviewResult>('review.get', { task_id: taskId, source }),
  reviewComment: (p: {
    task_id: number;
    source: ReviewSource;
    path: string;
    line: number;
    side: DiffSide;
    body: string;
    target: CommentTarget;
  }) => call<null>('review.comment', p),
  reviewReply: (taskId: number, threadId: string, body: string) =>
    call<null>('review.reply', { task_id: taskId, thread_id: threadId, body }),
  reviewResolve: (taskId: number, threadId: string, resolved: boolean) =>
    call<null>('review.resolve', { task_id: taskId, thread_id: threadId, resolved }),
  reviewSetViewed: (taskId: number, source: ReviewSource, path: string, viewed: boolean) =>
    call<null>('review.set_viewed', { task_id: taskId, source, path, viewed }),
  reviewSubmit: (taskId: number, event: ReviewEvent, body: string) =>
    call<null>('review.submit', { task_id: taskId, event, body }),
  reviewPublish: (taskId: number, threadId: string, target: CommentTarget) =>
    call<null>('review.publish', { task_id: taskId, thread_id: threadId, target }),
  reviewPrompt: (taskId: number, source: ReviewSource, threadIds: string[]) =>
    call<{ prompt: string }>('review.prompt', { task_id: taskId, source, thread_ids: threadIds }),
  killSession: (sessionId: number) => call<null>('session.kill', { session_id: sessionId }),
  send,
  resize: (sessionId: number, rows: number, cols: number) =>
    call<null>('session.resize', { session_id: sessionId, rows, cols }),
  read: (sessionId: number, lines: number) =>
    call<SessionReadResult>('session.read', { session_id: sessionId, lines }),
  attach: (sessionId: number, onOutput: Channel<string>) =>
    serialized(sessionId, () =>
      command<SessionAttachResult>('session_attach', { sessionId, onOutput }),
    ),
  detach: (sessionId: number) =>
    serialized(sessionId, () => command<void>('session_detach', { sessionId })),
  agents: () => call<AgentInfo[]>('agent.list'),
  plugins: () => call<PluginInfo[]>('plugin.list'),
  pluginSettings: (name: string) => call<PluginSettings>('plugin.settings', { name }),
  setPluginSettings: (name: string, values: Record<string, SettingValue | null>) =>
    call<null>('plugin.set_settings', { name, values }),
  reloadPlugins: () => call<null>('plugin.reload', {}),
  storeList: () => call<StoreList>('store.list'),
  addStore: (source: string) => call<StoreInfo>('store.add', { source }),
  removeStore: (name: string, uninstallPlugins: boolean) =>
    call<null>('store.remove', { name, uninstall_plugins: uninstallPlugins }),
  refreshStores: (name?: string) => call<null>('store.refresh', name ? { name } : {}),
  setAutoUpdate: (enabled: boolean) => call<null>('store.set_auto_update', { enabled }),
  searchPlugins: (p: { query?: string; capability?: CapabilityKind; store?: string }) =>
    call<SearchHit[]>('plugin.search', p),
  pluginDetails: (store: string, name: string) =>
    call<PluginDetails>('plugin.details', { store, name }),
  installPlugin: (store: string, name: string, acceptPermissions: string[]) =>
    call<PluginInfo>('plugin.install', { store, name, accept_permissions: acceptPermissions }),
  updatePlugin: (name: string, acceptPermissions?: string[]) =>
    call<PluginInfo>('plugin.update', { name, accept_permissions: acceptPermissions ?? null }),
  rollbackPlugin: (name: string) => call<PluginInfo>('plugin.rollback', { name }),
  uninstallPlugin: (name: string) => call<null>('plugin.uninstall', { name }),
  setPluginEnabled: (name: string, enabled: boolean) =>
    call<null>('plugin.set_enabled', { name, enabled }),
  linkPlugin: (path: string) => call<PluginInfo>('plugin.link', { path }),
  unlinkPlugin: (name: string) => call<null>('plugin.unlink', { name }),
  agentConfig: (agent: string) => call<AgentConfig>('agent_config.get', { agent }),
  agentConfigRaw: (agent: string) => call<AgentConfigRaw>('agent_config.get_raw', { agent }),
  setAgentConfig: (agent: string, config: AgentConfig) =>
    call<null>('agent_config.set', { agent, config }),
  removeSession: (sessionId: number) => call<null>('session.remove', { session_id: sessionId }),
  nodeConfig: () => call<NodeConfigInfo>('node_config.get'),
  nodeStats: (pids: number[]) => call<NodeStats>('node.stats', { pids }),
  setNodeConfig: (config: NodeConfig) => call<null>('node_config.set', { config }),
  forges: () => call<ForgeInfo[]>('forge.list'),
  forgeStatus: (forge: string) => call<ForgeStatus>('forge.status', { forge }),
  forgeRepos: (forge: string, owner: string) => call<ForgeRepo[]>('forge.repos', { forge, owner }),
  cloneProject: (source: string, forge?: string) =>
    call<Project>('project.clone', { source, forge }),
  createProject: (name: string, remote: RemoteTarget | null) =>
    call<ProjectCreateResult>('project.create', { name, remote }),
};
