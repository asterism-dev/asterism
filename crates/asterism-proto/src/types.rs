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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AgentInfo {
    pub name: String,
    pub available: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct HelloResult {
    pub proto_version: u32,
    pub daemon_version: String,
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
    #[serde(default)]
    pub force: bool,
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

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(tag = "method", content = "params")]
pub enum Event {
    #[serde(rename = "session.status_changed")]
    SessionStatusChanged { session_id: i64, status: SessionStatus },
    #[serde(rename = "session.output")]
    SessionOutput { session_id: i64, data: String },
    #[serde(rename = "session.changed")]
    SessionChanged(Session),
    #[serde(rename = "task.changed")]
    TaskChanged(Task),
    #[serde(rename = "project.changed")]
    ProjectChanged(Project),
    #[serde(rename = "project.removed")]
    ProjectRemoved { project_id: i64 },
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
