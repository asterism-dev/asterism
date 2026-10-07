use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::Duration;

use asterism_proto::client::{Client, ClientError};
use asterism_proto::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{
    method, ClientKind, Event, HelloParams, HelloResult, Session, SessionAttachResult,
    SessionIdParams, SessionListParams, SessionStatus,
};
use asterism_proto::PROTO_VERSION;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{Notify, OnceCell};

use crate::{daemon, login_env};

const BACKOFF_START: Duration = Duration::from_millis(250);
const BACKOFF_MAX: Duration = Duration::from_secs(5);
const RAW_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(2);
const REPLACE_EXIT_TIMEOUT: Duration = Duration::from_secs(5);

pub type OutputSink = Box<dyn Fn(String) + Send + Sync>;

pub trait NodeSink: Send + Sync + 'static {
    fn status(&self, status: &NodeStatus);
    fn event(&self, event: &Event);
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum NodeStatus {
    Connecting,
    Connected {
        hello: HelloResult,
    },
    UpdateAvailable {
        hello: HelloResult,
        bundled_version: String,
        bundled_build: String,
    },
    Incompatible {
        message: String,
    },
    Disconnected {
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct CallError {
    pub kind: String,
    pub message: String,
}

impl CallError {
    fn not_connected() -> Self {
        Self {
            kind: "not_connected".into(),
            message: "the node is not connected".into(),
        }
    }

    fn connection(message: impl Into<String>) -> Self {
        Self {
            kind: "connection".into(),
            message: message.into(),
        }
    }
}

impl From<ClientError> for CallError {
    fn from(e: ClientError) -> Self {
        match e {
            ClientError::Rpc(err) => Self {
                kind: kind_name(err.kind()),
                message: err.message,
            },
            other => Self::connection(other.to_string()),
        }
    }
}

fn kind_name(kind: ErrorKind) -> String {
    serde_json::to_value(kind)
        .ok()
        .and_then(|v| v.as_str().map(String::from))
        .unwrap_or_else(|| "unknown".into())
}

pub enum PathEnv {
    Inherit,
    Fixed(String),
    /// Read from the user's login shell, only when the daemon has to be started.
    LoginShell,
}

pub struct LocalNodeConfig {
    pub paths: Paths,
    pub daemon_bin: PathBuf,
    pub path_env: PathEnv,
    pub bundled_version: String,
    pub bundled_build: String,
}

enum Outcome {
    Retry { reason: String, was_connected: bool },
    Incompatible(String),
    Replaced,
}

fn retry(reason: impl ToString) -> Outcome {
    Outcome::Retry {
        reason: reason.to_string(),
        was_connected: false,
    }
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct LocalNode {
    config: LocalNodeConfig,
    sink: Arc<dyn NodeSink>,
    client: Mutex<Option<Arc<Client>>>,
    status: Mutex<NodeStatus>,
    outputs: Mutex<HashMap<i64, (u64, OutputSink)>>,
    attach_token: AtomicU64,
    login_path: OnceCell<Option<String>>,
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
            attach_token: AtomicU64::new(0),
            login_path: OnceCell::new(),
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
        lock(&self.client)
            .clone()
            .ok_or_else(CallError::not_connected)
    }

    pub async fn call(&self, method: &str, params: Value) -> Result<Value, CallError> {
        Ok(self.client()?.call(method, params).await?)
    }

    pub async fn attach(
        &self,
        session_id: i64,
        on_output: OutputSink,
    ) -> Result<SessionAttachResult, CallError> {
        let client = self.client()?;
        let token = self.attach_token.fetch_add(1, Ordering::Relaxed);
        lock(&self.outputs).insert(session_id, (token, on_output));
        let attached = client
            .call(method::SESSION_ATTACH, SessionIdParams { session_id })
            .await;
        if attached.is_err() {
            // A newer overlapping attach may own the entry by now; only drop our own.
            let mut outputs = lock(&self.outputs);
            if outputs.get(&session_id).is_some_and(|(t, _)| *t == token) {
                outputs.remove(&session_id);
            }
        }
        Ok(attached?)
    }

    pub async fn detach(&self, session_id: i64) -> Result<(), CallError> {
        lock(&self.outputs).remove(&session_id);
        Ok(self
            .client()?
            .call(method::SESSION_DETACH, SessionIdParams { session_id })
            .await?)
    }

    /// Stops whatever daemon owns the socket; the connection loop then starts the bundled one.
    pub async fn restart_daemon(&self) -> Result<(), CallError> {
        let client = lock(&self.client).clone();
        if let Some(client) = client {
            return Ok(client.call::<_, ()>(method::SHUTDOWN, ()).await?);
        }
        let stopped =
            tokio::time::timeout(RAW_SHUTDOWN_TIMEOUT, shutdown_raw(&self.config.paths)).await;
        // A daemon that is gone or unresponsive is as good as stopped; the loop respawns either way.
        let result = match stopped {
            Ok(Err(ShutdownError::Io(e))) => Err(CallError::connection(e.to_string())),
            _ => Ok(()),
        };
        self.restart.notify_one();
        result
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
                Outcome::Retry {
                    reason,
                    was_connected,
                } => {
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

    async fn spawn_path(&self) -> Option<String> {
        match &self.config.path_env {
            PathEnv::Inherit => None,
            PathEnv::Fixed(path) => Some(path.clone()),
            PathEnv::LoginShell => self
                .login_path
                .get_or_init(|| async {
                    let login = tokio::task::spawn_blocking(login_env::login_shell_path).await.ok().flatten();
                    login.or_else(|| {
                        eprintln!("asterism: could not read PATH from the login shell; using a fallback PATH");
                        let inherited = std::env::var("PATH").unwrap_or_default();
                        Some(login_env::fallback_path(&inherited, std::env::var("HOME").ok().as_deref()))
                    })
                })
                .await
                .clone(),
        }
    }

    async fn connect_and_serve(&self) -> Outcome {
        let config = &self.config;
        let stream = match daemon::connect(&config.paths).await {
            Ok(stream) => stream,
            Err(_) => {
                let path = self.spawn_path().await;
                match daemon::spawn_and_connect(&config.paths, &config.daemon_bin, path.as_deref())
                    .await
                {
                    Ok(stream) => stream,
                    Err(e) => return retry(format!("could not start the daemon: {e}")),
                }
            }
        };
        let (reader, writer) = stream.into_split();
        let client = Arc::new(Client::new(reader, writer));
        let Some(mut events) = client.take_events() else {
            return retry("event stream unavailable");
        };
        let hello_params = HelloParams {
            proto_version: PROTO_VERSION,
            client_kind: ClientKind::App,
        };
        let hello: HelloResult = match client.call(method::HELLO, hello_params).await {
            Ok(hello) => hello,
            Err(ClientError::Rpc(e)) if e.kind() == ErrorKind::IncompatibleVersion => {
                return Outcome::Incompatible(e.message);
            }
            Err(e) => return retry(e),
        };

        let outdated = daemon_outdated(
            &hello.daemon_version,
            &hello.daemon_build,
            &config.bundled_version,
            &config.bundled_build,
        );
        let silent_restart = outdated
            && !self.restarted_for_update.load(Ordering::SeqCst)
            && !has_running_sessions(&client).await;
        if silent_restart {
            self.restarted_for_update.store(true, Ordering::SeqCst);
            if client.call::<_, ()>(method::SHUTDOWN, ()).await.is_ok() {
                // The connection closes when the old daemon exits, which also releases its lock.
                let closed = tokio::time::timeout(REPLACE_EXIT_TIMEOUT, async {
                    while events.recv().await.is_some() {}
                });
                return match closed.await {
                    Ok(()) => Outcome::Replaced,
                    Err(_) => retry("the old daemon did not exit after shutdown"),
                };
            }
        }

        if let Err(e) = client.call::<_, ()>(method::SUBSCRIBE, ()).await {
            return retry(e);
        }
        *lock(&self.client) = Some(client);
        self.set_status(if !outdated {
            NodeStatus::Connected { hello }
        } else {
            NodeStatus::UpdateAvailable {
                hello,
                bundled_version: config.bundled_version.clone(),
                bundled_build: config.bundled_build.clone(),
            }
        });
        while let Some(event) = events.recv().await {
            match event {
                Event::SessionOutput { session_id, data } => {
                    if let Some((_, sink)) = lock(&self.outputs).get(&session_id) {
                        sink(data);
                    }
                }
                other => self.sink.event(&other),
            }
        }
        *lock(&self.client) = None;
        lock(&self.outputs).clear();
        Outcome::Retry {
            reason: "connection to the daemon closed".into(),
            was_connected: true,
        }
    }
}

fn version_tuple(version: &str) -> Option<Vec<u64>> {
    version.split('.').map(|part| part.parse().ok()).collect()
}

/// A newer daemon is kept; at the same version a different build counts as outdated.
fn daemon_outdated(daemon: &str, daemon_build: &str, bundled: &str, bundled_build: &str) -> bool {
    match (version_tuple(daemon), version_tuple(bundled)) {
        (Some(d), Some(b)) if d != b => d < b,
        (Some(_), Some(_)) => daemon_build != bundled_build,
        _ => daemon != bundled || daemon_build != bundled_build,
    }
}

async fn has_running_sessions(client: &Client) -> bool {
    match client
        .call::<_, Vec<Session>>(method::SESSION_LIST, SessionListParams::default())
        .await
    {
        Ok(sessions) => sessions.iter().any(|s| s.status != SessionStatus::Exited),
        Err(_) => true,
    }
}

enum ShutdownError {
    Unreachable,
    Io(std::io::Error),
}

/// Raw JSON so a daemon speaking another protocol version still understands it.
async fn shutdown_raw(paths: &Paths) -> Result<(), ShutdownError> {
    let stream = UnixStream::connect(paths.socket())
        .await
        .map_err(|_| ShutdownError::Unreachable)?;
    let (reader, mut writer) = stream.into_split();
    writer
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"shutdown\",\"params\":null}\n")
        .await
        .map_err(ShutdownError::Io)?;
    BufReader::new(reader)
        .read_line(&mut String::new())
        .await
        .map_err(ShutdownError::Io)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::daemon_outdated;

    #[test]
    fn only_an_older_daemon_is_outdated() {
        assert!(daemon_outdated("0.1.0", "b", "0.2.0", "b"));
        assert!(daemon_outdated("0.9.0", "b", "0.10.0", "b"));
        assert!(!daemon_outdated("0.2.0", "b", "0.1.0", "b"));
        assert!(!daemon_outdated("0.1.0", "b", "0.1.0", "b"));
        assert!(daemon_outdated("0.1.0-dev", "b", "0.1.0", "b"));
        assert!(!daemon_outdated("0.1.0-dev", "b", "0.1.0-dev", "b"));
    }

    #[test]
    fn same_version_from_another_build_is_outdated() {
        assert!(daemon_outdated("0.2.0", "6cf7ac0", "0.2.0", "f81e486"));
        assert!(daemon_outdated("0.2.0", "", "0.2.0", "f81e486"));
        assert!(!daemon_outdated("0.3.0", "6cf7ac0", "0.2.0", "f81e486"));
        assert!(daemon_outdated("0.1.0-dev", "a", "0.1.0-dev", "b"));
    }
}
