use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use asterism_proto::rpc::{ErrorKind, Request, Response, RpcError};
use asterism_proto::types::{method, *};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::{broadcast, mpsc};
use tokio::task::{JoinHandle, JoinSet};

use crate::daemon::Daemon;
use crate::error::{Error, Result};
use crate::lock;
use crate::plugins::process::HostFn;

const MAX_OUTPUT_FRAME: usize = 64 * 1024;
// Bounded so a client that stops reading backs up into the broadcast channels, which drop on lag.
const OUT_QUEUE: usize = 256;
// Lets the shutdown response reach the client before the server stops.
const SHUTDOWN_GRACE: Duration = Duration::from_millis(50);
const ACCEPT_BACKOFF: Duration = Duration::from_millis(100);

/// One client connection; background forwarders are keyed so re-attaching replaces them.
struct Conn {
    out: mpsc::Sender<String>,
    forwarders: Mutex<HashMap<String, JoinHandle<()>>>,
}

impl Conn {
    // Awaiting (not try_send) so a briefly full queue delays a response instead of losing it.
    async fn send(&self, response: &Response) {
        if let Ok(line) = serde_json::to_string(response) {
            let _ = self.out.send(line).await;
        }
    }

    fn replace_forwarder(&self, key: String, handle: JoinHandle<()>) {
        if let Some(old) = lock(&self.forwarders).insert(key, handle) {
            old.abort();
        }
    }

    fn stop_forwarder(&self, key: &str) {
        if let Some(handle) = lock(&self.forwarders).remove(key) {
            handle.abort();
        }
    }

    fn stop_all(&self) {
        for (_, handle) in lock(&self.forwarders).drain() {
            handle.abort();
        }
    }
}

pub async fn serve(daemon: Arc<Daemon>, listener: UnixListener) {
    loop {
        match listener.accept().await {
            Ok((stream, _)) => {
                tokio::spawn(handle_conn(daemon.clone(), stream));
            }
            Err(e) => {
                eprintln!("asterismd: accept failed: {e}");
                // Persistent errors such as EMFILE would otherwise spin a core.
                tokio::time::sleep(ACCEPT_BACKOFF).await;
            }
        }
    }
}

async fn handle_conn(daemon: Arc<Daemon>, stream: UnixStream) {
    let (reader, mut writer) = stream.into_split();
    let (out, mut out_rx) = mpsc::channel::<String>(OUT_QUEUE);
    tokio::spawn(async move {
        while let Some(mut line) = out_rx.recv().await {
            line.push('\n');
            if writer.write_all(line.as_bytes()).await.is_err() {
                break;
            }
        }
    });
    let conn = Arc::new(Conn { out, forwarders: Mutex::new(HashMap::new()) });
    // Dropping the set on disconnect aborts in-flight requests such as an unbounded `wait`.
    let mut requests = JoinSet::new();
    let mut lines = BufReader::new(reader).lines();
    while let Ok(Some(line)) = lines.next_line().await {
        while requests.try_join_next().is_some() {}
        requests.spawn(handle_line(daemon.clone(), conn.clone(), line));
    }
    requests.shutdown().await;
    conn.stop_all();
}

async fn handle_line(daemon: Arc<Daemon>, conn: Arc<Conn>, line: String) {
    let request = match serde_json::from_str::<Request>(&line) {
        Ok(request) => request,
        Err(e) => {
            conn.send(&Response::err(0, RpcError::new(ErrorKind::ParseError, e.to_string()))).await;
            return;
        }
    };
    let id = request.id;
    if request.method == method::SESSION_ATTACH {
        attach(&daemon, &conn, id, request.params).await;
        return;
    }
    let response = match dispatch(&daemon, &conn, request).await {
        Ok(result) => Response::ok(id, result),
        Err(e) => Response::err(id, e.into()),
    };
    conn.send(&response).await;
}

/// The snapshot response is queued before the forwarder starts, so output never overtakes it.
async fn attach(daemon: &Daemon, conn: &Conn, id: u64, raw: Value) {
    let result = params::<SessionIdParams>(raw).and_then(|p| {
        let (snapshot, rx) = daemon.attach(p.session_id)?;
        let result = SessionAttachResult { snapshot: BASE64.encode(&snapshot.screen), rows: snapshot.rows, cols: snapshot.cols };
        Ok((p.session_id, to_value(result)?, rx))
    });
    match result {
        Ok((session_id, value, rx)) => {
            conn.send(&Response::ok(id, value)).await;
            let forwarder = tokio::spawn(forward_output(session_id, rx, conn.out.clone()));
            conn.replace_forwarder(format!("attach:{session_id}"), forwarder);
        }
        Err(e) => conn.send(&Response::err(id, e.into())).await,
    }
}

async fn blocking(f: impl FnOnce() -> Result<Value> + Send + 'static) -> Result<Value> {
    tokio::task::spawn_blocking(f)
        .await
        .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?
}

async fn dispatch(daemon: &Arc<Daemon>, conn: &Conn, request: Request) -> Result<Value> {
    let raw = request.params;
    match request.method.as_str() {
        method::SHUTDOWN => {
            let daemon = daemon.clone();
            tokio::spawn(async move {
                tokio::time::sleep(SHUTDOWN_GRACE).await;
                daemon.request_shutdown();
            });
            Ok(Value::Null)
        }
        method::SUBSCRIBE => {
            let forwarder = tokio::spawn(forward_events(daemon.subscribe(), conn.out.clone()));
            conn.replace_forwarder("subscribe".into(), forwarder);
            Ok(Value::Null)
        }
        method::SESSION_DETACH => {
            conn.stop_forwarder(&format!("attach:{}", params::<SessionIdParams>(raw)?.session_id));
            Ok(Value::Null)
        }
        other => dispatch_method(daemon, other, raw).await,
    }
}

/// Every method that does not need the client connection; plugins reach these through the host API.
pub async fn dispatch_method(daemon: &Arc<Daemon>, method_name: &str, raw: Value) -> Result<Value> {
    match method_name {
        method::HELLO => to_value(daemon.hello(params(raw)?)?),
        method::PROJECT_LIST => to_value(daemon.projects()?),
        method::PROJECT_ADD => to_value(daemon.add_project(&params::<ProjectAddParams>(raw)?.path)?),
        method::PROJECT_REMOVE => {
            daemon.remove_project(params::<ProjectIdParams>(raw)?.project_id)?;
            Ok(Value::Null)
        }
        method::PLUGIN_LIST => to_value(daemon.plugin_list()?),
        method::PLUGIN_LINK => to_value(daemon.plugin_link(&params::<PluginPathParams>(raw)?.path).await?),
        method::PLUGIN_UNLINK => {
            daemon.plugin_unlink(&params::<PluginNameParams>(raw)?.name).await?;
            Ok(Value::Null)
        }
        method::PLUGIN_RELOAD => {
            daemon.reload_plugins(params::<PluginReloadParams>(raw)?.name.as_deref()).await?;
            Ok(Value::Null)
        }
        method::PLUGIN_SETTINGS => to_value(daemon.plugin_settings(&params::<PluginNameParams>(raw)?.name)?),
        method::PLUGIN_SET_SETTINGS => {
            let p: PluginSetSettingsParams = params(raw)?;
            daemon.set_plugin_settings(&p.name, &p.values).await?;
            Ok(Value::Null)
        }
        method::NODE_CONFIG_GET => {
            let daemon = daemon.clone();
            blocking(move || to_value(daemon.node_config()?)).await
        }
        method::NODE_CONFIG_SET => {
            let daemon = daemon.clone();
            blocking(move || {
                daemon.set_node_config(&params::<NodeConfigSetParams>(raw)?.config)?;
                Ok(Value::Null)
            })
            .await
        }
        method::NODE_STATS => {
            let daemon = daemon.clone();
            let params = params::<NodeStatsParams>(raw)?;
            blocking(move || to_value(daemon.stats(params))).await
        }
        method::PROJECT_WORKTREES => {
            let daemon = daemon.clone();
            let id = params::<ProjectIdParams>(raw)?.project_id;
            blocking(move || to_value(daemon.project_worktrees(id)?)).await
        }
        method::PROJECT_WORKTREE_SIZES => {
            let daemon = daemon.clone();
            let id = params::<ProjectIdParams>(raw)?.project_id;
            blocking(move || to_value(daemon.project_worktree_sizes(id)?)).await
        }
        method::PROJECT_WORKTREE_REMOVE => {
            let daemon = daemon.clone();
            let p: ProjectWorktreeParams = params(raw)?;
            blocking(move || {
                daemon.remove_worktree(p.project_id, &p.path)?;
                Ok(Value::Null)
            })
            .await
        }
        method::PROJECT_WORKTREE_PRUNE => {
            let daemon = daemon.clone();
            let id = params::<ProjectIdParams>(raw)?.project_id;
            blocking(move || {
                daemon.prune_worktrees(id)?;
                Ok(Value::Null)
            })
            .await
        }
        method::FORGE_LIST => to_value(daemon.forges()),
        method::FORGE_STATUS => to_value(daemon.forge_status(&params::<ForgeParams>(raw)?.forge).await?),
        method::FORGE_REPOS => {
            let p: ForgeOwnerParams = params(raw)?;
            to_value(daemon.forge_repos(&p.forge, &p.owner).await?)
        }
        method::PROJECT_CLONE => {
            let p = params::<ProjectCloneParams>(raw)?;
            to_value(daemon.clone_project(&p.source, p.forge.as_deref()).await?)
        }
        method::PROJECT_CREATE => to_value(daemon.create_project(&params::<ProjectCreateParams>(raw)?).await?),
        method::AGENT_LIST => to_value(daemon.agent_infos()),
        method::AGENT_CONFIG_GET => {
            let daemon = daemon.clone();
            blocking(move || to_value(daemon.agent_config(&params::<AgentParams>(raw)?.agent)?)).await
        }
        method::AGENT_CONFIG_GET_RAW => {
            let daemon = daemon.clone();
            blocking(move || to_value(daemon.agent_config_raw(&params::<AgentParams>(raw)?.agent)?)).await
        }
        method::AGENT_CONFIG_SET => {
            let daemon = daemon.clone();
            blocking(move || {
                let p: AgentConfigSetParams = params(raw)?;
                daemon.set_agent_config(&p.agent, &p.config)?;
                Ok(Value::Null)
            })
            .await
        }
        method::SESSION_REMOVE => {
            daemon.remove_session(params::<SessionIdParams>(raw)?.session_id).await?;
            Ok(Value::Null)
        }
        method::TASK_LIST => to_value(daemon.tasks(params(raw)?)?),
        method::TASK_CREATE => to_value(daemon.create_task(params(raw)?).await?),
        method::TASK_ARCHIVE => to_value(daemon.archive_task(params::<TaskArchiveParams>(raw)?.task_id)?),
        method::TASK_RESTORE => {
            let daemon = daemon.clone();
            let task_id = params::<TaskIdParams>(raw)?.task_id;
            blocking(move || to_value(daemon.restore_task(task_id)?)).await
        }
        method::TASK_DELETE_CHECK => {
            let daemon = daemon.clone();
            let task_id = params::<TaskIdParams>(raw)?.task_id;
            blocking(move || to_value(daemon.delete_check(task_id)?)).await
        }
        method::TASK_DELETE => {
            let daemon = daemon.clone();
            let p: TaskDeleteParams = params(raw)?;
            blocking(move || to_value(daemon.delete_task(p.task_id, p.delete_branch)?)).await
        }
        method::TASK_DIFF => to_value(daemon.diff(params::<TaskIdParams>(raw)?.task_id)?),
        method::SESSION_LIST => to_value(daemon.sessions(params::<SessionListParams>(raw)?.task_id)?),
        method::SESSION_START => to_value(daemon.start_session(params(raw)?).await?),
        method::SESSION_KILL => {
            daemon.kill_session(params::<SessionIdParams>(raw)?.session_id)?;
            Ok(Value::Null)
        }
        method::SESSION_SEND => {
            daemon.send(params(raw)?).await?;
            Ok(Value::Null)
        }
        method::SESSION_RESIZE => {
            daemon.resize(params(raw)?)?;
            Ok(Value::Null)
        }
        method::SESSION_READ => to_value(daemon.read(params(raw)?)?),
        method::SESSION_WAIT => to_value(SessionWaitResult { status: daemon.wait(params(raw)?).await? }),
        method::SESSION_HOOK => {
            daemon.hook(params(raw)?)?;
            Ok(Value::Null)
        }
        other => Err(Error::new(ErrorKind::MethodNotFound, format!("unknown method {other}"))),
    }
}

const HOST_DENIED: &[&str] = &[method::SHUTDOWN, method::SUBSCRIBE, method::SESSION_ATTACH, method::SESSION_DETACH];

pub fn host_fn(daemon: Weak<Daemon>) -> HostFn {
    Arc::new(move |_plugin: String, method_name: String, params: Value| {
        let daemon = daemon.clone();
        Box::pin(async move {
            if method_name.starts_with("plugin.") || HOST_DENIED.contains(&method_name.as_str()) {
                return Err(RpcError::new(ErrorKind::MethodNotFound, format!("{method_name} is not available to plugins")));
            }
            let Some(daemon) = daemon.upgrade() else {
                return Err(RpcError::new(ErrorKind::Internal, "the daemon is shutting down"));
            };
            dispatch_method(&daemon, &method_name, params).await.map_err(Into::into)
        })
    })
}

async fn forward_events(mut rx: broadcast::Receiver<Event>, out: mpsc::Sender<String>) {
    loop {
        match rx.recv().await {
            Ok(event) => {
                if let Ok(line) = serde_json::to_string(&event.to_notification()) {
                    if out.send(line).await.is_err() {
                        break;
                    }
                }
            }
            // ponytail: lagged events are dropped; add a resync event if clients ever fall 1024 events behind.
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => break,
        }
    }
}

async fn forward_output(session_id: i64, mut rx: broadcast::Receiver<Vec<u8>>, out: mpsc::Sender<String>) {
    loop {
        let mut frame = match rx.recv().await {
            Ok(chunk) => chunk,
            // ponytail: lagging drops bytes and garbles the client screen until it re-attaches.
            Err(RecvError::Lagged(_)) => continue,
            Err(RecvError::Closed) => break,
        };
        while frame.len() < MAX_OUTPUT_FRAME {
            match rx.try_recv() {
                Ok(more) => frame.extend_from_slice(&more),
                Err(_) => break,
            }
        }
        let event = Event::SessionOutput { session_id, data: BASE64.encode(&frame) };
        let Ok(line) = serde_json::to_string(&event.to_notification()) else { continue };
        if out.send(line).await.is_err() {
            break;
        }
    }
}

fn params<T: DeserializeOwned>(value: Value) -> Result<T> {
    let value = if value.is_null() { Value::Object(Default::default()) } else { value };
    serde_json::from_value(value).map_err(|e| Error::new(ErrorKind::InvalidParams, e.to_string()))
}

fn to_value<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))
}
