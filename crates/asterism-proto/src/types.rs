use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::BTreeMap;

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
    pub const REVIEW_GET: &str = "review.get";
    pub const REVIEW_COMMENT: &str = "review.comment";
    pub const REVIEW_REPLY: &str = "review.reply";
    pub const REVIEW_RESOLVE: &str = "review.resolve";
    pub const REVIEW_SET_VIEWED: &str = "review.set_viewed";
    pub const REVIEW_SUBMIT: &str = "review.submit";
    pub const REVIEW_PUBLISH: &str = "review.publish";
    pub const REVIEW_PROMPT: &str = "review.prompt";
    pub const REVIEW_COMMITS: &str = "review.commits";
    pub const REVIEW_COMMIT_DIFF: &str = "review.commit_diff";
    pub const REVIEW_ADD_COMMENT: &str = "review.add_comment";
    pub const REVIEW_CHECK_LOG: &str = "review.check_log";
    pub const REVIEW_CHECK_RERUN: &str = "review.check_rerun";
    pub const TASK_FILE: &str = "task.file";
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
    pub const STORE_LIST: &str = "store.list";
    pub const STORE_ADD: &str = "store.add";
    pub const STORE_REMOVE: &str = "store.remove";
    pub const STORE_REFRESH: &str = "store.refresh";
    pub const STORE_SET_AUTO_UPDATE: &str = "store.set_auto_update";
    pub const PLUGIN_SEARCH: &str = "plugin.search";
    pub const PLUGIN_DETAILS: &str = "plugin.details";
    pub const PLUGIN_INSTALL: &str = "plugin.install";
    pub const PLUGIN_UPDATE: &str = "plugin.update";
    pub const PLUGIN_ROLLBACK: &str = "plugin.rollback";
    pub const PLUGIN_UNINSTALL: &str = "plugin.uninstall";
    pub const PLUGIN_SET_ENABLED: &str = "plugin.set_enabled";
    pub const TASK_SOURCE_LIST: &str = "task_source.list";
    pub const TASK_SOURCE_SEARCH: &str = "task_source.search";
    pub const TASK_SOURCE_GET: &str = "task_source.get";
    pub const PR_LIST: &str = "pr.list";
    pub const PR_REFRESH: &str = "pr.refresh";
    pub const PROJECT_BRANCHES: &str = "project.branches";
    pub const PROJECT_UPDATE: &str = "project.update";
    pub const PR_SEARCH: &str = "pr.search";
    pub const SESSION_SUBAGENTS: &str = "session.subagents";
    pub const PLUGIN_UI_FILE: &str = "plugin.ui_file";
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
    Hibernate,
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
    /// Base for new tasks; `None` means automatic (origin's default branch).
    #[serde(default)]
    pub default_base: Option<String>,
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
pub struct ProjectUpdateParams {
    pub project_id: i64,
    #[serde(default)]
    pub default_base: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProjectBranches {
    pub branches: Vec<String>,
    /// What `task.create` uses without an explicit base; `None` when the repository has no commits.
    pub default: Option<String>,
    /// The default when no base is configured.
    pub automatic: Option<String>,
    pub configured: Option<String>,
    pub fetch_error: Option<String>,
    /// Directory new task worktrees of this project are created in.
    #[serde(default)]
    pub worktree_root: Option<String>,
    #[serde(default)]
    pub local: Vec<String>,
    /// Branches of `origin` as `origin/<branch>`, the only remote tasks check out from.
    #[serde(default)]
    pub remote: Vec<String>,
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
    Installed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum PluginState {
    Ok,
    NeedsSetup { missing: Vec<String> },
    Broken { reason: String },
    Failing { reason: String },
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CapabilityKind {
    Forge,
    Agent,
    Command,
    TaskSource,
    Panel,
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
    /// Store an installed plugin came from.
    #[serde(default)]
    pub store: Option<String>,
    #[serde(default)]
    pub update_available: bool,
    #[serde(default)]
    pub previous_version: Option<String>,
    #[serde(default)]
    pub panels: Vec<PanelInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PanelInfo {
    pub id: String,
    pub title: String,
    pub entry: String,
    pub slot: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginUiFileParams {
    pub plugin: String,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginUiFile {
    pub mime: String,
    /// Base64-encoded file contents.
    pub data: String,
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
pub struct StoreInfo {
    pub name: String,
    pub source: String,
    pub official: bool,
    /// Unix seconds of the last successful refresh in this daemon's lifetime.
    pub last_refreshed: Option<i64>,
    pub last_error: Option<String>,
    pub plugin_count: usize,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreList {
    pub auto_update: bool,
    pub stores: Vec<StoreInfo>,
    /// Set when stores.toml cannot be read.
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreAddParams {
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreRemoveParams {
    pub name: String,
    #[serde(default)]
    pub uninstall_plugins: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct StoreRefreshParams {
    #[serde(default)]
    pub name: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AutoUpdateParams {
    pub enabled: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginSearchParams {
    #[serde(default)]
    pub query: Option<String>,
    #[serde(default)]
    pub capability: Option<CapabilityKind>,
    #[serde(default)]
    pub store: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchHit {
    pub store: String,
    pub name: String,
    pub description: String,
    pub tags: Vec<String>,
    /// Set when this plugin is installed from this store.
    pub installed_version: Option<String>,
    pub update_available: bool,
    /// A linked development copy overrides this plugin.
    pub linked: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginRefParams {
    pub store: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginDetails {
    pub store: String,
    pub name: String,
    pub version: String,
    pub description: String,
    pub permissions: Vec<String>,
    pub capabilities: Vec<Capability>,
    pub readme: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginInstallParams {
    pub store: String,
    pub name: String,
    #[serde(default)]
    pub accept_permissions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginUpdateParams {
    pub name: String,
    /// Without it, an update that adds permissions fails with PermissionsChanged.
    #[serde(default)]
    pub accept_permissions: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PluginEnableParams {
    pub name: String,
    pub enabled: bool,
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
    #[serde(default)]
    pub issue: Option<IssueRef>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskListParams {
    #[serde(default)]
    pub project_id: Option<i64>,
    #[serde(default)]
    pub include_archived: bool,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCreateParams {
    pub project_id: i64,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub prompt: Option<String>,
    #[serde(default)]
    pub agent: Option<String>,
    #[serde(default)]
    pub base: Option<String>,
    #[serde(default)]
    pub issue: Option<TaskIssue>,
    /// Name of the new branch; derived from the title or issue when absent.
    #[serde(default)]
    pub branch: Option<String>,
    /// Check out this existing branch instead of creating one; excludes `base` and `branch`.
    #[serde(default)]
    pub checkout: Option<String>,
    #[serde(default)]
    pub push: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueRef {
    pub source: String,
    pub key: String,
    pub url: String,
}

/// The issue a task is created from; `branch` is the branch to create.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskIssue {
    pub source: String,
    pub key: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub branch: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueHit {
    pub key: String,
    pub title: String,
    pub url: String,
    pub state: String,
    #[serde(default)]
    pub assignee: Option<String>,
    /// RFC 3339, as the source reports it.
    #[serde(default)]
    pub updated_at: Option<String>,
}

/// An issue as a task-source plugin returns it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Issue {
    pub key: String,
    pub title: String,
    pub url: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub branch: Option<String>,
}

/// An issue plus the task name, branch and prompt the daemon derives from it.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct IssueDetails {
    pub source: String,
    pub key: String,
    pub title: String,
    pub url: String,
    pub description: String,
    pub name: String,
    pub branch: String,
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum PrListState {
    #[default]
    Open,
    Closed,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrHit {
    pub number: u64,
    pub title: String,
    pub url: String,
    pub author: String,
    pub head_branch: String,
    pub draft: bool,
    pub from_fork: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrSearchParams {
    pub project_id: i64,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub state: PrListState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrSearchResult {
    pub forge: String,
    /// `owner/name` of the origin repository.
    pub repo: String,
    pub hits: Vec<PrHit>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum PrState {
    Open,
    Draft,
    Merged,
    Closed,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewState {
    Approved,
    ChangesRequested,
    ReviewRequired,
    None,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ChecksState {
    Pending,
    Success,
    Failure,
    None,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrChecks {
    pub state: ChecksState,
    #[serde(default)]
    pub failing: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequest {
    pub number: u64,
    pub url: String,
    pub title: String,
    pub state: PrState,
    pub review: ReviewState,
    pub checks: PrChecks,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskPr {
    pub task_id: i64,
    pub branch: String,
    pub pr: PullRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrProjectError {
    pub project_id: i64,
    pub message: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrList {
    pub prs: Vec<TaskPr>,
    pub errors: Vec<PrProjectError>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrListParams {
    #[serde(default)]
    pub project_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSourceInfo {
    pub id: String,
    pub display_name: String,
    pub plugin: String,
    pub available: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSourceListParams {
    #[serde(default)]
    pub project_id: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSourceSearchParams {
    pub project_id: i64,
    pub source: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub assigned_to_me: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSourceGetParams {
    pub project_id: i64,
    pub source: String,
    pub key: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskCreateResult {
    pub task: Task,
    pub session: Option<Session>,
    #[serde(default)]
    pub warning: Option<String>,
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

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum DiffSide {
    Old,
    New,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewSource {
    Pr,
    Local,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CommentTarget {
    Local,
    Single,
    Review,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewEvent {
    Comment,
    Approve,
    RequestChanges,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewComment {
    pub id: String,
    pub author: String,
    pub body: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewThread {
    pub id: String,
    pub path: String,
    pub line: u32,
    pub side: DiffSide,
    pub outdated: bool,
    pub resolved: bool,
    #[serde(default)]
    pub local: bool,
    #[serde(default)]
    pub pending: bool,
    pub comments: Vec<ReviewComment>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PendingReview {
    pub id: String,
    pub comments: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ForgeReview {
    pub head_sha: String,
    pub base_sha: String,
    /// A ref `git fetch origin <head_ref>` can fetch, also for forks.
    pub head_ref: String,
    pub threads: Vec<ReviewThread>,
    pub conversation: Vec<ConversationItem>,
    pub viewed_files: Vec<String>,
    pub pending_review: Option<PendingReview>,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub checks: Vec<CheckRun>,
    /// Commit sha → the forge's combined check state for it (lowercase).
    #[serde(default)]
    pub commit_checks: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewGetParams {
    pub task_id: i64,
    pub source: ReviewSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewResult {
    pub source: ReviewSource,
    pub pr: Option<u64>,
    pub reviews_supported: bool,
    pub patch: String,
    pub threads: Vec<ReviewThread>,
    pub conversation: Vec<ConversationItem>,
    pub viewed_files: Vec<String>,
    pub pending_review: Option<PendingReview>,
    /// The worktree differs from the pull request head (PR source only).
    pub local_ahead: bool,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub body: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub checks: Vec<CheckRun>,
    /// Commit sha → the forge's combined check state for it (lowercase).
    #[serde(default)]
    pub commit_checks: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewCommentParams {
    pub task_id: i64,
    pub source: ReviewSource,
    pub path: String,
    pub line: u32,
    pub side: DiffSide,
    pub body: String,
    pub target: CommentTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewReplyParams {
    pub task_id: i64,
    pub thread_id: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewResolveParams {
    pub task_id: i64,
    pub thread_id: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewViewedParams {
    pub task_id: i64,
    pub source: ReviewSource,
    pub path: String,
    pub viewed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewSubmitParams {
    pub task_id: i64,
    pub event: ReviewEvent,
    #[serde(default)]
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewPublishParams {
    pub task_id: i64,
    pub thread_id: String,
    pub target: CommentTarget,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewPromptParams {
    pub task_id: i64,
    pub source: ReviewSource,
    /// `None` means every thread (open ones unless `include_resolved`).
    #[serde(default)]
    pub thread_ids: Option<Vec<String>>,
    #[serde(default)]
    pub include_resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewPromptResult {
    pub prompt: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum CheckStatus {
    Queued,
    Running,
    Done,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckRun {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub workflow: String,
    pub status: CheckStatus,
    /// Lowercase forge value once done, e.g. `success` or `failure`.
    #[serde(default)]
    pub conclusion: Option<String>,
    #[serde(default)]
    pub started_at: Option<String>,
    #[serde(default)]
    pub completed_at: Option<String>,
    pub url: String,
    #[serde(default)]
    pub has_log: bool,
    #[serde(default)]
    pub rerunnable: bool,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ConversationKind {
    Comment,
    Review,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ConversationItem {
    pub id: String,
    pub kind: ConversationKind,
    pub author: String,
    pub body: String,
    pub created_at: String,
    /// Reviews only: `approved`, `changes_requested`, `commented` or `dismissed`.
    #[serde(default)]
    pub state: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CheckLog {
    pub text: String,
    pub truncated: bool,
    pub url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewCommit {
    pub sha: String,
    pub short: String,
    pub subject: String,
    pub author: String,
    /// Author date, ISO 8601 with offset.
    pub date: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewCommitsParams {
    pub task_id: i64,
    pub source: ReviewSource,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewCommitsResult {
    /// Newest first.
    pub commits: Vec<ReviewCommit>,
    /// The worktree has changes that are not committed (Local source only).
    pub uncommitted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewCommitDiffParams {
    pub task_id: i64,
    pub source: ReviewSource,
    /// `None` means the uncommitted changes of the worktree.
    #[serde(default)]
    pub sha: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewAddCommentParams {
    pub task_id: i64,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewCheckParams {
    pub task_id: i64,
    pub check_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskFileParams {
    pub task_id: i64,
    pub path: String,
    /// The caller's last seen mtime; when it still matches, `content` is omitted.
    pub known_mtime: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskFileResult {
    /// Worktree-relative when inside the worktree, else absolute with `HOME` as `~`.
    pub path: String,
    /// Modification time in milliseconds since the epoch.
    pub mtime: i64,
    pub content: Option<String>,
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
    Hibernated,
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
    SubagentStart,
    SubagentStop,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SubagentHook {
    pub id: String,
    #[serde(default)]
    pub parent_id: Option<String>,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub failed: bool,
    /// A second id the agent learned for this subagent; a later stop may use it.
    #[serde(default)]
    pub alias: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SubagentStatus {
    Running,
    Done,
    Failed,
    /// The session ended while the subagent was still running.
    Ended,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Subagent {
    pub id: String,
    pub parent_id: Option<String>,
    pub kind: String,
    pub description: String,
    pub status: SubagentStatus,
    pub started_at: i64,
    pub ended_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionSubagentsParams {
    pub session_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SessionHookParams {
    pub session_id: i64,
    pub event: HookEvent,
    /// The agent's own session id, used to resume after a daemon restart.
    #[serde(default)]
    pub agent_ref: Option<String>,
    #[serde(default)]
    pub subagent: Option<SubagentHook>,
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
    /// Minutes without output before an idle, unattached session hibernates; `None` is the default, 0 disables.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hibernate_after_min: Option<u32>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentConfigRaw {
    #[serde(default)]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: EnvSettings,
    pub mcp_text: Option<String>,
    pub hooks_text: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hibernate_after_min: Option<u32>,
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
    SessionStatusChanged {
        session_id: i64,
        status: SessionStatus,
    },
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
    #[serde(rename = "stores.changed")]
    StoresChanged {},
    #[serde(rename = "pr.changed")]
    PrChanged {
        task_id: i64,
        pr: Option<PullRequest>,
    },
    #[serde(rename = "review.changed")]
    ReviewChanged { task_id: i64 },
    #[serde(rename = "subagent.started")]
    SubagentStarted { session_id: i64, subagent: Subagent },
    #[serde(rename = "subagent.updated")]
    SubagentUpdated { session_id: i64, subagent: Subagent },
}

impl Event {
    pub fn to_notification(&self) -> Notification {
        let value = serde_json::to_value(self).unwrap_or(Value::Null);
        Notification::new(
            value["method"].as_str().unwrap_or_default(),
            value["params"].clone(),
        )
    }

    pub fn from_notification(notification: &Notification) -> Option<Self> {
        serde_json::from_value(
            json!({"method": notification.method, "params": notification.params}),
        )
        .ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn old_task_create_requests_still_parse() {
        let p: TaskCreateParams =
            serde_json::from_value(serde_json::json!({"project_id": 1, "title": "t"})).unwrap();
        assert_eq!((p.branch, p.checkout, p.push), (None, None, false));
        let s: PrSearchParams =
            serde_json::from_value(serde_json::json!({"project_id": 1})).unwrap();
        assert_eq!((s.query.as_str(), s.state), ("", PrListState::Open));
        assert_eq!(serde_json::to_value(PrListState::Closed).unwrap(), "closed");
    }
}
