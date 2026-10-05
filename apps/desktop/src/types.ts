export type SessionStatus = 'working' | 'idle' | 'waiting_input' | 'exited';
export type SessionKind = { type: 'agent'; name: string } | { type: 'shell' } | { type: 'command'; argv: string[] };

export interface EnvSettings { remove: string[]; set: Record<string, string> }
export interface AgentConfig {
  args: string[];
  env: EnvSettings;
  mcp: Record<string, unknown> | null;
  hooks: Record<string, unknown> | null;
}
export interface AgentConfigRaw { args: string[]; env: EnvSettings; mcp_text: string | null; hooks_text: string | null }

export interface Project { id: number; name: string; path: string; created_at: number }
export interface Task {
  id: number;
  project_id: number;
  title: string;
  slug: string;
  branch: string;
  base_branch: string;
  worktree_path: string;
  prompt: string | null;
  archived: boolean;
  created_at: number;
  last_activity_at: number;
}
export interface Session { id: number; task_id: number; kind: SessionKind; status: SessionStatus }
export interface AgentInfo { name: string; available: boolean }
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
  | { state: 'update_available'; hello: HelloResult; bundled_version: string; bundled_build: string }
  | { state: 'incompatible'; message: string }
  | { state: 'disconnected'; reason: string };

export type NodeEvent =
  | { method: 'session.status_changed'; params: { session_id: number; status: SessionStatus } }
  | { method: 'session.changed'; params: Session }
  | { method: 'session.removed'; params: { session_id: number } }
  | { method: 'task.changed'; params: Task }
  | { method: 'project.changed'; params: Project }
  | { method: 'project.removed'; params: { project_id: number } };

export interface TaskCreateResult { task: Task; session: Session | null }
export interface TaskDiffResult { patch: string }
export interface SessionAttachResult { snapshot: string; rows: number; cols: number }
export interface SessionReadResult { text: string }

export interface PathSettings { repos: string; worktrees: string }
export interface NodeConfig { paths: PathSettings }
export interface NodeConfigInfo { config: NodeConfig; defaults: PathSettings }
export interface GithubStatus { available: boolean; logged_in: boolean; login: string | null; orgs: string[]; error: string | null }
export interface GithubRepo { name_with_owner: string; description: string | null; private: boolean }
export type Visibility = 'public' | 'private' | 'internal';
export interface GithubTarget { owner: string; visibility: Visibility }
export interface ProjectCreateResult { project: Project; github_error: string | null }
