export type SessionStatus = 'working' | 'idle' | 'waiting_input' | 'exited' | 'hibernated';
export type SessionKind =
  { type: 'agent'; name: string } | { type: 'shell' } | { type: 'command'; argv: string[] };

export interface EnvSettings {
  remove: string[];
  set: Record<string, string>;
}
export interface AgentConfig {
  args: string[];
  env: EnvSettings;
  mcp: Record<string, unknown> | null;
  hooks: Record<string, unknown> | null;
  hibernate_after_min?: number | null;
}
export interface AgentConfigRaw {
  args: string[];
  env: EnvSettings;
  mcp_text: string | null;
  hooks_text: string | null;
  hibernate_after_min?: number | null;
}

export interface Project {
  id: number;
  name: string;
  path: string;
  created_at: number;
  default_base: string | null;
}
export interface ProjectBranches {
  branches: string[];
  default: string | null;
  automatic: string | null;
  configured: string | null;
  fetch_error: string | null;
  worktree_root: string | null;
  local: string[];
  remote: string[];
}
export interface Task {
  id: number;
  project_id: number;
  title: string;
  slug: string;
  branch: string;
  base_branch: string;
  worktree_path: string;
  prompt: string | null;
  issue: IssueRef | null;
  archived: boolean;
  created_at: number;
  last_activity_at: number;
}
export interface IssueRef {
  source: string;
  key: string;
  url: string;
}
export interface TaskIssue {
  source: string;
  key: string;
  title: string;
  url: string;
  branch: string | null;
}
export interface IssueHit {
  key: string;
  title: string;
  url: string;
  state: string;
  assignee: string | null;
  updated_at: string | null;
}
export interface IssueDetails {
  source: string;
  key: string;
  title: string;
  url: string;
  description: string;
  name: string;
  branch: string;
  prompt: string;
}
export interface TaskSourceInfo {
  id: string;
  display_name: string;
  plugin: string;
  available: boolean;
  reason: string | null;
}
export interface Session {
  id: number;
  task_id: number;
  kind: SessionKind;
  status: SessionStatus;
}
export interface AgentInfo {
  name: string;
  available: boolean;
}
export interface HelloResult {
  proto_version: number;
  daemon_version: string;
  daemon_build: string;
  pid: number;
  hostname: string;
  os: string;
  agents: AgentInfo[];
}

export type NodeStatus =
  | { state: 'connecting' }
  | { state: 'connected'; hello: HelloResult }
  | {
      state: 'update_available';
      hello: HelloResult;
      bundled_version: string;
      bundled_build: string;
    }
  | { state: 'incompatible'; message: string }
  | { state: 'disconnected'; reason: string };

export type PrState = 'open' | 'draft' | 'merged' | 'closed';
export type ReviewState = 'approved' | 'changes_requested' | 'review_required' | 'none';
export type ChecksState = 'pending' | 'success' | 'failure' | 'none';
export interface PullRequest {
  number: number;
  url: string;
  title: string;
  state: PrState;
  review: ReviewState;
  checks: { state: ChecksState; failing: string[] };
}
export interface TaskPr {
  task_id: number;
  branch: string;
  pr: PullRequest;
}
export interface PrList {
  prs: TaskPr[];
  errors: { project_id: number; message: string }[];
}

export type NodeEvent =
  | { method: 'session.status_changed'; params: { session_id: number; status: SessionStatus } }
  | { method: 'session.changed'; params: Session }
  | { method: 'session.removed'; params: { session_id: number } }
  | { method: 'task.changed'; params: Task }
  | { method: 'task.removed'; params: { task_id: number } }
  | { method: 'project.changed'; params: Project }
  | { method: 'project.removed'; params: { project_id: number } }
  | { method: 'plugins.changed'; params: Record<string, never> }
  | { method: 'stores.changed'; params: Record<string, never> }
  | { method: 'review.changed'; params: { task_id: number } }
  | { method: 'pr.changed'; params: { task_id: number; pr: PullRequest | null } }
  | { method: 'subagent.started'; params: { session_id: number; subagent: Subagent } }
  | { method: 'subagent.updated'; params: { session_id: number; subagent: Subagent } };

export type SubagentStatus = 'running' | 'done' | 'failed' | 'ended';
export interface Subagent {
  id: string;
  parent_id: string | null;
  kind: string;
  description: string;
  status: SubagentStatus;
  started_at: number;
  ended_at: number | null;
}

export interface TaskDeleteCheck {
  dirty: boolean;
  branch: string;
  branch_exists: boolean;
  unmerged_commits: number;
}
export interface TaskDeleteResult {
  warning: string | null;
}
export interface Worktree {
  path: string;
  head: string;
  branch: string | null;
  is_main: boolean;
  locked: boolean;
  prunable: boolean;
  task_id: number | null;
  base_branch: string | null;
}
export interface WorktreeSize {
  path: string;
  bytes: number;
}
export interface TaskCreateResult {
  task: Task;
  session: Session | null;
  warning: string | null;
}
export type PrListState = 'open' | 'closed';
export interface PrHit {
  number: number;
  title: string;
  url: string;
  author: string;
  head_branch: string;
  draft: boolean;
  from_fork: boolean;
}
export interface PrSearchResult {
  forge: string;
  repo: string;
  hits: PrHit[];
}
export type TaskCreateRequest = {
  project_id: number;
  title: string;
  prompt: string | null;
  agent: string | null;
  base: string | null;
  issue: TaskIssue | null;
  branch: string | null;
  checkout: string | null;
  push: boolean;
};
export interface TaskDiffResult {
  patch: string;
}
export interface TaskFileResult {
  path: string;
  mtime: number;
  content: string | null;
}
export interface SessionAttachResult {
  snapshot: string;
  rows: number;
  cols: number;
}
export interface SessionReadResult {
  text: string;
}

export interface PathSettings {
  repos: string;
  worktrees: string;
}
export interface NodeConfig {
  paths: PathSettings;
}
export interface NodeConfigInfo {
  config: NodeConfig;
  defaults: PathSettings;
}
export interface ForgeInfo {
  id: string;
  display_name: string;
  hosts: string[];
  plugin: string;
}
export interface ForgeStatus {
  available: boolean;
  authenticated: boolean;
  account: string | null;
  owners: string[];
  error: string | null;
}
export interface ForgeRepo {
  owner: string;
  name: string;
  description: string | null;
  private: boolean;
}
export type Visibility = 'public' | 'private' | 'internal';
export interface RemoteTarget {
  forge: string;
  owner: string;
  visibility: Visibility;
}
export interface ProcStats {
  memory_bytes: number;
  cpu_percent: number;
}
export interface NodeStats {
  daemon: ProcStats;
  sessions: { session_id: number; stats: ProcStats }[];
  processes: { pid: number; stats: ProcStats }[];
}
export interface ProjectCreateResult {
  project: Project;
  remote_error: string | null;
}

export type AgentSettingKind = 'args' | 'mcp' | 'hooks' | 'hibernate';
export interface AgentInfo {
  name: string;
  available: boolean;
  display_name: string;
  settings: AgentSettingKind[];
  plugin: string;
}
export type PluginState =
  | { state: 'ok' }
  | { state: 'needs_setup'; missing: string[] }
  | { state: 'broken'; reason: string }
  | { state: 'failing'; reason: string }
  | { state: 'disabled' };
export type CapabilityKind = 'forge' | 'agent' | 'command' | 'task_source' | 'panel';
export interface PanelInfo {
  id: string;
  title: string;
  entry: string;
  slot: string;
}
export interface Capability {
  kind: CapabilityKind;
  id: string;
  description: string;
}
export interface PluginInfo {
  name: string;
  version: string | null;
  description: string;
  origin: 'builtin' | 'linked' | 'installed';
  path: string;
  capabilities: Capability[];
  panels: PanelInfo[];
  permissions: string[];
  state: PluginState;
  backend: string[] | null;
  store: string | null;
  update_available: boolean;
  previous_version: string | null;
}
export interface StoreInfo {
  name: string;
  source: string;
  official: boolean;
  last_refreshed: number | null;
  last_error: string | null;
  plugin_count: number;
}
export interface StoreList {
  auto_update: boolean;
  stores: StoreInfo[];
  error: string | null;
}
export interface SearchHit {
  store: string;
  name: string;
  description: string;
  tags: string[];
  installed_version: string | null;
  update_available: boolean;
  linked: boolean;
}
export interface PluginDetails {
  store: string;
  name: string;
  version: string;
  description: string;
  permissions: string[];
  capabilities: Capability[];
  readme: string | null;
}
export type SettingType = 'string' | 'secret' | 'bool' | 'number' | 'enum';
export interface SettingSpec {
  key: string;
  title: string;
  type: SettingType;
  required: boolean;
  description: string | null;
  default: unknown;
  options?: string[];
}
export type SettingValue = string | number | boolean;
export interface PluginSettings {
  schema: SettingSpec[];
  values: Record<string, SettingValue>;
  secrets_set: string[];
}

export type DiffSide = 'old' | 'new';
export type ReviewSource = 'pr' | 'local';
export type CommentTarget = 'local' | 'single' | 'review';
export type ReviewEvent = 'comment' | 'approve' | 'request_changes';
export interface ReviewComment {
  id: string;
  author: string;
  body: string;
  created_at: string;
}
export interface ReviewThread {
  id: string;
  path: string;
  line: number;
  side: DiffSide;
  outdated: boolean;
  resolved: boolean;
  local: boolean;
  pending: boolean;
  comments: ReviewComment[];
}
export interface PendingReview {
  id: string;
  comments: number;
}
export interface ReviewResult {
  source: ReviewSource;
  pr: number | null;
  reviews_supported: boolean;
  patch: string;
  threads: ReviewThread[];
  conversation: ReviewComment[];
  viewed_files: string[];
  pending_review: PendingReview | null;
  local_ahead: boolean;
}
