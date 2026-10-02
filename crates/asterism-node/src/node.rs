use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use asterism_proto::client::{Client, ClientError};
use asterism_proto::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{
    method, ClientKind, Event, HelloParams, HelloResult, Session, SessionAttachResult, SessionIdParams,
    SessionListParams, SessionStatus,
};
use asterism_proto::PROTO_VERSION;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::Notify;

use crate::daemon;

const BACKOFF_START: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(5);

pub type OutputSink = Box<dyn Fn(String) + Send + Sync>;

pub trait NodeSink: Send + Sync + 'static {
    fn status(&self, status: &NodeStatus);
    fn event(&self, event: &Event);
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum NodeStatus {
    Connecting,
    Connected { hello: HelloResult },
    UpdateAvailable { hello: HelloResult, bundled_version: String },
    Incompatible { message: String },
    Disconnected { reason: String },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CallError {
    pub kind: String,
    pub message: String,
}

impl CallError {
    fn not_connected() -> Self {
        Self { kind: "not_connected".into(), message: "the node is not connected".into() }
    }

    fn connection(message: impl Into<String>) -> Self {
        Self { kind: "connection".into(), message: message.into() }
    }
}

impl From<ClientError> for CallError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::Rpc(err) => Self { kind: kind_name(err.kind()), message: err.message },
            other => Self::connection(other.to_string()),
        }
    }
}

fn kind_name(kind: ErrorKind) -> String {
    serde_json::to_value(kind).ok().and_then(|v| v.as_str().map(String::from)).unwrap_or_else(|| "unknown".into())
}

pub struct LocalNodeConfig {
    pub paths: Paths,
    pub daemon_bin: PathBuf,
    pub path_env: Option<String>,
    pub bundled_version: String,
}

enum Outcome {
    Retry { reason: String, was_connected: bool },
    Incompatible(String),
    Replaced,
}

fn retry(reason: impl ToString) -> Outcome {
    Outcome::Retry { reason: reason.to_string(), was_connected: false }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct LocalNode {
    config: LocalNodeConfig,
    sink: Arc<dyn NodeSink>,
    client: Mutex<Option<Arc<Client>>>,
    status: Mutex<NodeStatus>,
    outputs: Mutex<HashMap<i64, OutputSink>>,
    restart: Notify,
    restarted_for_update: AtomicBool,
}

impl LocalNode {
    pub fn new(config: LocalNodeConfig, sink: Arc<dyn NodeSink>) -> Arc<Self> {
        Arc::new(Self {
            config,
            sink,
            client: Mutex::new(None),
            status: Mutex::new(NodeStatus::Connecting),
            outputs: Mutex::new(HashMap::new()),
            restart: Notify::new(),
            restarted_for_update: AtomicBool::new(false),
        })
    }

    pub fn status(&self) -> NodeStatus {
        lock(&self.status).clone()
    }

    fn set_status(&self, status: NodeStatus) {
        *lock(&self.status) = status.clone();
        self.sink.status(&status);
    }

    fn client(&self) -> Result<Arc<Client>, CallError> {
        lock(&self.client).clone().ok_or_else(CallError::not_connected)
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, CallError> {
        Ok(self.client()?.call(method, params).await?)
    }

    pub async fn attach(&self, session_id: i64, on_output: OutputSink) -> Result<SessionAttachResult, CallError> {
        let client = self.client()?;
        lock(&self.outputs).insert(session_id, on_output);
        let attached = client.call(method::SESSION_ATTACH, SessionIdParams { session_id }).await;
        if attached.is_err() {
            lock(&self.outputs).remove(&session_id);
        }
        Ok(attached?)
    }

    pub async fn detach(&self, session_id: i64) -> Result<(), CallError> {
        lock(&self.outputs).remove(&session_id);
        Ok(self.client()?.call(method::SESSION_DETACH, SessionIdParams { session_id }).await?)
    }

    /// Stops whatever daemon owns the socket; the connection loop then starts the bundled one.
    pub async fn restart_daemon(&self) -> Result<(), CallError> {
        let client = lock(&self.client).clone();
        match client {
            Some(client) => client.call::<_, ()>(method::SHUTDOWN, ()).await?,
            None => shutdown_raw(&self.config.paths).await.map_err(|e| CallError::connection(e.to_string()))?,
        }
        self.restart.notify_one();
        Ok(())
    }

    pub async fn run(self: Arc<Self>) {
        let mut backoff = BACKOFF_START;
        loop {
            self.set_status(NodeStatus::Connecting);
            match self.connect_and_serve().await {
                Outcome::Replaced => continue,
                Outcome::Incompatible(message) => {
                    self.set_status(NodeStatus::Incompatible { message });
                    self.restart.notified().await;
                    backoff = BACKOFF_START;
                    continue;
                }
                Outcome::Retry { reason, was_connected } => {
                    self.set_status(NodeStatus::Disconnected { reason });
                    if was_connected {
                        backoff = BACKOFF_START;
                    }
                }
            }
            tokio::time::sleep(backoff).await;
            backoff = (backoff * 2).min(BACKOFF_MAX);
        }
    }

    async fn connect_and_serve(&self) -> Outcome {
        let config = &self.config;
        let stream = match daemon::connect_or_spawn(&config.paths, &config.daemon_bin, config.path_env.as_deref()).await {
            Ok(stream) => stream,
            Err(e) => return retry(format!("could not start the daemon: {e}")),
        };
        let (reader, writer) = stream.into_split();
        let client = Arc::new(Client::new(reader, writer));
        let Some(mut events) = client.take_events() else { return retry("event stream unavailable") };
        let hello_params = HelloParams { proto_version: PROTO_VERSION, client_kind: ClientKind::App };
        let hello: HelloResult = match client.call(method::HELLO, hello_params).await {
            Ok(hello) => hello,
            Err(ClientError::Rpc(e)) if e.kind() == ErrorKind::IncompatibleVersion => {
                return Outcome::Incompatible(e.message);
            }
            Err(e) => return retry(e),
        };

        if hello.daemon_version == config.bundled_version {
            self.set_status(NodeStatus::Connected { hello });
        } else if !self.restarted_for_update.swap(true, Ordering::SeqCst) && !has_running_sessions(&client).await {
            let _ = client.call::<_, ()>(method::SHUTDOWN, ()).await;
            // The connection closes when the old daemon exits, which also releases its lock.
            while events.recv().await.is_some() {}
            return Outcome::Replaced;
        } else {
            self.set_status(NodeStatus::UpdateAvailable { hello, bundled_version: config.bundled_version.clone() });
        }

        if let Err(e) = client.call::<_, ()>(method::SUBSCRIBE, ()).await {
            return retry(e);
        }
        *lock(&self.client) = Some(client);
        while let Some(event) = events.recv().await {
            match event {
                Event::SessionOutput { session_id, data } => {
                    if let Some(sink) = lock(&self.outputs).get(&session_id) {
                        sink(data);
                    }
                }
                other => self.sink.event(&other),
            }
        }
        *lock(&self.client) = None;
        lock(&self.outputs).clear();
        Outcome::Retry { reason: "connection to the daemon closed".into(), was_connected: true }
    }
}

async fn has_running_sessions(client: &Client) -> bool {
    match client.call::<_, Vec<Session>>(method::SESSION_LIST, SessionListParams::default()).await {
        Ok(sessions) => sessions.iter().any(|s| s.status != SessionStatus::Exited),
        Err(_) => true,
    }
}

/// Raw JSON so a daemon speaking another protocol version still understands it.
async fn shutdown_raw(paths: &Paths) -> std::io::Result<()> {
    let stream = UnixStream::connect(paths.socket()).await?;
    let (reader, mut writer) = stream.into_split();
    writer.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"shutdown\",\"params\":null}\n").await?;
    BufReader::new(reader).read_line(&mut String::new()).await?;
    Ok(())
}
