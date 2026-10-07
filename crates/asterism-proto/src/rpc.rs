use std::fmt;

use serde::{Deserialize, Serialize};
use serde_json::Value;

const JSONRPC: &str = "2.0";

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Request {
    pub jsonrpc: String,
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: Value,
}

impl Request {
    pub fn new(id: u64, method: &str, params: Value) -> Self {
        Self {
            jsonrpc: JSONRPC.into(),
            id,
            method: method.into(),
            params,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Response {
    pub jsonrpc: String,
    pub id: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<RpcError>,
}

impl Response {
    pub fn ok(id: u64, result: Value) -> Self {
        Self {
            jsonrpc: JSONRPC.into(),
            id,
            result: Some(result),
            error: None,
        }
    }

    pub fn err(id: u64, error: RpcError) -> Self {
        Self {
            jsonrpc: JSONRPC.into(),
            id,
            result: None,
            error: Some(error),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Notification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Value,
}

impl Notification {
    pub fn new(method: &str, params: Value) -> Self {
        Self {
            jsonrpc: JSONRPC.into(),
            method: method.into(),
            params,
        }
    }
}

/// Any line the daemon sends: responses carry an `id`, notifications do not.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum ServerMessage {
    Response(Response),
    Notification(Notification),
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ErrorKind {
    ParseError,
    MethodNotFound,
    InvalidParams,
    Internal,
    NotFound,
    NotARepo,
    BranchExists,
    DirtyWorktree,
    AgentUnavailable,
    IncompatibleVersion,
    Timeout,
    Git,
    PluginError,
    /// The plugin asks for permissions the caller did not accept.
    PermissionsChanged,
    /// The plugin is missing required settings.
    NeedsSetup,
    /// A kind from a newer peer; keeps older clients decoding its errors.
    #[serde(other)]
    Unknown,
}

impl ErrorKind {
    pub fn code(self) -> i64 {
        match self {
            Self::ParseError => -32700,
            Self::MethodNotFound => -32601,
            Self::InvalidParams => -32602,
            Self::Internal => -32603,
            Self::NotFound => -32001,
            Self::NotARepo => -32002,
            Self::BranchExists => -32003,
            Self::DirtyWorktree => -32004,
            Self::AgentUnavailable => -32005,
            Self::IncompatibleVersion => -32006,
            Self::Timeout => -32007,
            Self::Git => -32008,
            Self::PluginError => -32009,
            Self::PermissionsChanged => -32010,
            Self::NeedsSetup => -32011,
            Self::Unknown => -32099,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ErrorData {
    pub kind: ErrorKind,
}

impl Default for ErrorData {
    fn default() -> Self {
        Self {
            kind: ErrorKind::Unknown,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RpcError {
    pub code: i64,
    pub message: String,
    #[serde(default)]
    pub data: ErrorData,
}

impl RpcError {
    pub fn new(kind: ErrorKind, message: impl Into<String>) -> Self {
        Self {
            code: kind.code(),
            message: message.into(),
            data: ErrorData { kind },
        }
    }

    pub fn kind(&self) -> ErrorKind {
        self.data.kind
    }
}

impl fmt::Display for RpcError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:?}: {}", self.data.kind, self.message)
    }
}

impl std::error::Error for RpcError {}
