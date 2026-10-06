use std::collections::BTreeMap;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};

use crate::rpc::Notification;

pub mod method {
    pub const HELLO: &str = "hello";
    pub const SHUTDOWN: &str = "shutdown";
    pub const SUBSCRIBE: &str = "subscribe";
    pub const PROJECT_LIST: &str = "project.list";
    pub const PROJECT_ADD: &str = "project.add";
    pub const PROJECT_REMOVE: &str = "project.remove";
    pub const TASK_LIST: &str = "task.list";
    pub const TASK_CREATE: &str = "task.create";
    pub const TASK_ARCHIVE: &str = "task.archive";
    pub const TASK_DIFF: &str = "task.diff";
    pub const TASK_RESTORE: &str = "task.restore";
    pub const TASK_DELETE_CHECK: &str = "task.delete_check";
    pub const TASK_DELETE: &str = "task.delete";
    pub const SESSION_LIST: &str = "session.list";
    pub const SESSION_START: &str = "session.start";
    pub const SESSION_KILL: &str = "session.kill";
    pub const SESSION_SEND: &str = "session.send";
    pub const SESSION_RESIZE: &str = "session.resize";
    pub const SESSION_READ: &str = "session.read";
    pub const SESSION_ATTACH: &str = "session.attach";
    pub const SESSION_DETACH: &str = "session.detach";
    pub const SESSION_WAIT: &str = "session.wait";
    pub const SESSION_HOOK: &str = "session.hook";
    pub const AGENT_CONFIG_GET: &str = "agent_config.get";
    pub const AGENT_CONFIG_GET_RAW: &str = "agent_config.get_raw";
    pub const AGENT_CONFIG_SET: &str = "agent_config.set";
    pub const SESSION_REMOVE: &str = "session.remove";
    pub const NODE_CONFIG_GET: &str = "node_config.get";
    pub const NODE_CONFIG_SET: &str = "node_config.set";
    pub const PROJECT_CLONE: &str = "project.clone";
    pub const PROJECT_CREATE: &str = "project.create";
    pub const NODE_STATS: &str = "node.stats";
    pub const PROJECT_WORKTREES: &str = "project.worktrees";
    pub const PROJECT_WORKTREE_SIZES: &str = "project.worktree_sizes";
    pub const PROJECT_WORKTREE_REMOVE: &str = "project.worktree_remove";
    pub const PROJECT_WORKTREE_PRUNE: &str = "project.worktree_prune";
    pub const FORGE_LIST: &str = "forge.list";
    pub const FORGE_STATUS: &str = "forge.status";
    pub const FORGE_REPOS: &str = "forge.repos";
    pub const AGENT_LIST: &str = "agent.list";
    pub const PLUGIN_LIST: &str = "plugin.list";
    pub const PLUGIN_LINK: &str = "plugin.link";
    pub const PLUGIN_UNLINK: &str = "plugin.unlink";
    pub const PLUGIN_RELOAD: &str = "plugin.reload";
    pub const PLUGIN_SETTINGS: &str = "plugin.settings";
    pub const PLUGIN_SET_SETTINGS: &str = "plugin.set_settings";
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ClientKind {
    App,
    Cli,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloParams {
    pub proto_version: u32,
    pub client_kind: ClientKind,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum AgentSettingKind {
    Args,
    Mcp,
    Hooks,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentInfo {
    pub name: String,
    pub available: bool,
    #[serde(default)]
    pub display_name: String,
    /// Which agent settings sections apply; environment settings always do.
    #[serde(default)]
    pub settings: Vec<AgentSettingKind>,
    #[serde(default)]
    pub plugin: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloResult {
    pub proto_version: u32,
    pub daemon_version: String,
    /// Empty from daemons that predate build ids.
    #[serde(default)]
    pub daemon_build: String,
    pub pid: u32,
    pub hostname: String,
    pub os: String,
    pub agents: Vec<AgentInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Project {
    pub id: i64,
    pub name: String,
    pub path: String,
    #[serde(default)]
    pub created_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectAddParams {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectIdParams {
    pub project_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectWorktreeParams {
    pub project_id: i64,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Worktree {
    pub path: String,
    pub head: String,
    /// Short branch name; `None` for a detached HEAD.
    pub branch: Option<String>,
    pub is_main: bool,
    pub locked: bool,
    pub prunable: bool,
    pub task_id: Option<i64>,
    pub base_branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorktreeSize {
    pub path: String,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PathSettings {
    pub repos: String,
    pub worktrees: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeConfig {
    pub paths: PathSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeConfigInfo {
    pub config: NodeConfig,
    pub defaults: PathSettings,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeConfigSetParams {
    pub config: NodeConfig,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct NodeStatsParams {
    /// Extra processes to measure on their own, e.g. the app itself.
    #[serde(default)]
    pub pids: Vec<u32>,
}

/// CPU is a percentage of one core since the previous `node.stats` call.
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq)]
pub struct ProcStats {
    pub memory_bytes: u64,
    pub cpu_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SessionStats {
    pub session_id: i64,
    pub stats: ProcStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PidStats {
    pub pid: u32,
    pub stats: ProcStats,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeStats {
    pub daemon: ProcStats,
    /// Each running session with its whole process tree.
    pub sessions: Vec<SessionStats>,
    pub processes: Vec<PidStats>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum Visibility {
    Public,
    Private,
    Internal,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeInfo {
    pub id: String,
    pub display_name: String,
    pub hosts: Vec<String>,
    pub plugin: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeStatus {
    pub available: bool,
    pub authenticated: bool,
    pub account: Option<String>,
    /// Organizations or groups the account can create repositories in.
    #[serde(default)]
    pub owners: Vec<String>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeRepo {
    pub owner: String,
    pub name: String,
    pub description: Option<String>,
    pub private: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeParams {
    pub forge: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeOwnerParams {
    pub forge: String,
    pub owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RemoteTarget {
    pub forge: String,
    pub owner: String,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PluginOrigin {
    Builtin,
    Linked,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PluginState {
    Ok,
    NeedsSetup { missing: Vec<String> },
    Broken { reason: String },
    Failing { reason: String },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Forge,
    Agent,
    Command,
    TaskSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Capability {
    pub kind: CapabilityKind,
    pub id: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginInfo {
    pub name: String,
    /// `None` when the manifest could not be read.
    pub version: Option<String>,
    pub description: String,
    pub origin: PluginOrigin,
    pub path: String,
    pub capabilities: Vec<Capability>,
    pub permissions: Vec<String>,
    pub state: PluginState,
    /// The backend argv with its program resolved; the CLI runs plugin commands with it.
    pub backend: Option<Vec<String>>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SettingType {
    String,
    Secret,
    Bool,
    Number,
    Enum,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub struct SettingSpec {
    pub key: String,
    pub title: String,
    #[serde(rename = "type")]
    pub kind: SettingType,
    #[serde(default)]
    pub required: bool,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub default: Option<Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub options: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginSettings {
    pub schema: Vec<SettingSpec>,
    /// Non-secret values, defaults included.
    pub values: BTreeMap<String, Value>,
    /// Keys of secrets that are set; their values never leave the daemon.
    pub secrets_set: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginPathParams {
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginNameParams {
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginReloadParams {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PluginSetSettingsParams {
    pub name: String,
    /// A `null` value restores the default, or clears a secret.
    pub values: BTreeMap<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectCloneParams {
    pub source: String,
    /// Forge for an `owner/repo` shorthand; the default forge when absent.
    #[serde(default)]
    pub forge: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectCreateParams {
    pub name: String,
    #[serde(default)]
    pub remote: Option<RemoteTarget>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectCreateResult {
    pub project: Project,
    /// Set when the local repository was created but the remote was not.
    pub remote_error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Task {
    pub id: i64,
    pub project_id: i64,
    pub title: String,
    pub slug: String,
    pub branch: String,
    pub base_branch: String,
    pub worktree_path: String,
    pub prompt: Option<String>,
    pub archived: bool,
    #[serde(default)]
    pub created_at: i64,
    /// Unix seconds (like `created_at`) of the last session start or status change.
    #[serde(default)]
    pub last_activity_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskListParams {
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCreateParams {
    pub project_id: i64,
    pub title: String,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCreateResult {
    pub task: Task,
    pub session: Option<Session>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskIdParams {
    pub task_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskArchiveParams {
    pub task_id: i64,
    /// Ignored since archiving keeps the worktree; kept so older clients still deserialize.
    #[serde(default)]
    pub force: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDeleteParams {
    pub task_id: i64,
    #[serde(default)]
    pub delete_branch: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDeleteCheck {
    pub dirty: bool,
    pub branch: String,
    pub branch_exists: bool,
    /// Commits on the branch that are not on its base branch.
    pub unmerged_commits: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDeleteResult {
    pub warning: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskDiffResult {
    pub patch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum SessionKind {
    Agent { name: String },
    Shell,
    Command { argv: Vec<String> },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SessionStatus {
    Working,
    Idle,
    WaitingInput,
    Exited,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Session {
    pub id: i64,
    pub task_id: i64,
    pub kind: SessionKind,
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionListParams {
    #[serde(default)]
    pub task_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionStartParams {
    pub task_id: i64,
    pub kind: SessionKind,
    #[serde(default)]
    pub prompt: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionIdParams {
    pub session_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSendParams {
    pub session_id: i64,
    pub text: String,
    #[serde(default)]
    pub submit: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionResizeParams {
    pub session_id: i64,
    pub rows: u16,
    pub cols: u16,
}

fn default_lines() -> usize {
    50
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionReadParams {
    pub session_id: i64,
    #[serde(default = "default_lines")]
    pub lines: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionReadResult {
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionAttachResult {
    /// Base64-encoded VT sequences that redraw the current screen.
    pub snapshot: String,
    pub rows: u16,
    pub cols: u16,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionWaitParams {
    pub session_id: i64,
    pub until: SessionStatus,
    #[serde(default)]
    pub timeout_ms: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionWaitResult {
    pub status: SessionStatus,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "kebab-case")]
pub enum HookEvent {
    PromptSubmit,
    Tool,
    Stop,
    Notification,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionHookParams {
    pub session_id: i64,
    pub event: HookEvent,
    /// The agent's own session id, used to resume after a daemon restart.
    #[serde(default)]
    pub agent_ref: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct EnvSettings {
    #[serde(default)]
    pub remove: Vec<String>,
    #[serde(default)]
    pub set: BTreeMap<String, String>,
}

/// Per-agent settings; `mcp` and `hooks` are Claude's native JSON formats.
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfig {
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: EnvSettings,
    #[serde(default)]
    pub mcp: Option<Value>,
    #[serde(default)]
    pub hooks: Option<Value>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfigRaw {
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: EnvSettings,
    pub mcp_text: Option<String>,
    pub hooks_text: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentParams {
    pub agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfigSetParams {
    pub agent: String,
    pub config: AgentConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "method", content = "params")]
pub enum Event {
    #[serde(rename = "session.status_changed")]
    SessionStatusChanged { session_id: i64, status: SessionStatus },
    #[serde(rename = "session.output")]
    SessionOutput { session_id: i64, data: String },
    #[serde(rename = "session.changed")]
    SessionChanged(Session),
    #[serde(rename = "session.removed")]
    SessionRemoved { session_id: i64 },
    #[serde(rename = "task.changed")]
    TaskChanged(Task),
    #[serde(rename = "task.removed")]
    TaskRemoved { task_id: i64 },
    #[serde(rename = "project.changed")]
    ProjectChanged(Project),
    #[serde(rename = "project.removed")]
    ProjectRemoved { project_id: i64 },
    #[serde(rename = "plugins.changed")]
    PluginsChanged {},
}

impl Event {
    pub fn to_notification(&self) -> Notification {
        let value = serde_json::to_value(self).unwrap_or(Value::Null);
        Notification::new(value["method"].as_str().unwrap_or_default(), value["params"].clone())
    }

    pub fn from_notification(notification: &Notification) -> Option<Self> {
        serde_json::from_value(json!({"method": notification.method, "params": notification.params})).ok()
    }
}
