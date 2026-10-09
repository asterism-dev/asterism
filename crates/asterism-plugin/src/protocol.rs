use asterism_proto::types::{DiffSide, PrListState, PullRequest, ReviewEvent, Visibility};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const PROTOCOL: u32 = 1;
/// Prefix of requests a plugin sends to the daemon's host API.
pub const HOST_PREFIX: &str = "host.";

pub mod method {
    pub const INITIALIZE: &str = "initialize";
    pub const SETTINGS_CHANGED: &str = "settings.changed";
    pub const FORGE_STATUS: &str = "forge.status";
    pub const FORGE_REVIEW_GET: &str = "forge.review.get";
    pub const FORGE_REVIEW_COMMENT: &str = "forge.review.comment";
    pub const FORGE_REVIEW_REPLY: &str = "forge.review.reply";
    pub const FORGE_REVIEW_RESOLVE: &str = "forge.review.resolve";
    pub const FORGE_REVIEW_SET_VIEWED: &str = "forge.review.set_viewed";
    pub const FORGE_REVIEW_SUBMIT: &str = "forge.review.submit";
    pub const TASK_SOURCE_CHECK: &str = "task_source.check";
    pub const TASK_SOURCE_SEARCH: &str = "task_source.search";
    pub const TASK_SOURCE_GET: &str = "task_source.get";
    pub const FORGE_LIST_REPOS: &str = "forge.list_repos";
    pub const FORGE_RESOLVE_OWNER: &str = "forge.resolve_owner";
    pub const FORGE_CLONE: &str = "forge.clone";
    pub const FORGE_CREATE_REMOTE: &str = "forge.create_remote";
    pub const FORGE_PULL_REQUESTS: &str = "forge.pull_requests";
    pub const FORGE_SEARCH_PULL_REQUESTS: &str = "forge.search_pull_requests";
    pub const AGENT_PREPARE: &str = "agent.prepare";
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct InitializeParams {
    pub protocol: u32,
    pub plugin_dir: String,
    pub data_dir: String,
    pub asterism_version: String,
    /// Every setting value, secrets included.
    #[serde(default)]
    pub settings: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InitializeResult {
    pub capabilities: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SettingsChangedParams {
    pub settings: Map<String, Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ListReposParams {
    pub owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolveOwnerParams {
    pub owner: String,
    pub visibility: Visibility,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ResolveOwnerResult {
    pub owner: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CloneParams {
    pub owner: String,
    pub repo: String,
    pub target: String,
    /// Complete extra environment for git, already including asterism's non-interactive defaults.
    #[serde(default)]
    pub git_env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CreateRemoteParams {
    pub owner: String,
    pub name: String,
    pub visibility: Visibility,
    pub dir: String,
    #[serde(default)]
    pub git_env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchMode {
    Start,
    Resume,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq)]
pub struct LaunchSettings {
    pub args: Vec<String>,
    /// Path of a validated MCP config file.
    pub mcp_config: Option<String>,
    /// The user's extra hooks; already validated by the daemon.
    pub hooks: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PrepareParams {
    pub agent: String,
    pub mode: LaunchMode,
    pub prompt: Option<String>,
    pub agent_ref: Option<String>,
    pub settings: LaunchSettings,
    /// Absolute path of the worktree the agent runs in.
    pub cwd: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrepareResult {
    pub argv: Vec<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSourceCheckParams {
    pub source: String,
    pub project_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TaskSourceCheck {
    pub available: bool,
    #[serde(default)]
    pub reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchIssuesParams {
    pub source: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub assigned_to_me: bool,
    pub project_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct GetIssueParams {
    pub source: String,
    pub key: String,
    pub project_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PullRequestsParams {
    pub forge: String,
    pub project_path: String,
    pub branches: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BranchPr {
    pub branch: String,
    pub pr: PullRequest,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SearchPullRequestsParams {
    pub forge: String,
    pub project_path: String,
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub state: PrListState,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrRef {
    pub forge: String,
    pub project_path: String,
    pub number: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ForgeCommentMode {
    Single,
    Review,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewGetForgeParams {
    #[serde(flatten)]
    pub pr: PrRef,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewCommentForgeParams {
    #[serde(flatten)]
    pub pr: PrRef,
    pub path: String,
    pub line: u32,
    pub side: DiffSide,
    pub body: String,
    pub mode: ForgeCommentMode,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewReplyForgeParams {
    #[serde(flatten)]
    pub pr: PrRef,
    pub thread_id: String,
    pub body: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewResolveForgeParams {
    #[serde(flatten)]
    pub pr: PrRef,
    pub thread_id: String,
    pub resolved: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewViewedForgeParams {
    #[serde(flatten)]
    pub pr: PrRef,
    pub path: String,
    pub viewed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReviewSubmitForgeParams {
    #[serde(flatten)]
    pub pr: PrRef,
    pub event: ReviewEvent,
    pub body: String,
}
