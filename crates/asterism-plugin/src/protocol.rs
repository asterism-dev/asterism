use asterism_proto::types::Visibility;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const PROTOCOL: u32 = 1;
/// Prefix of requests a plugin sends to the daemon's host API.
pub const HOST_PREFIX: &str = "host.";

pub mod method {
    pub const INITIALIZE: &str = "initialize";
    pub const SETTINGS_CHANGED: &str = "settings.changed";
    pub const FORGE_STATUS: &str = "forge.status";
    pub const FORGE_LIST_REPOS: &str = "forge.list_repos";
    pub const FORGE_RESOLVE_OWNER: &str = "forge.resolve_owner";
    pub const FORGE_CLONE: &str = "forge.clone";
    pub const FORGE_CREATE_REMOTE: &str = "forge.create_remote";
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
}

#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct PrepareResult {
    pub argv: Vec<String>,
    #[serde(default)]
    pub env: Vec<(String, String)>,
}
