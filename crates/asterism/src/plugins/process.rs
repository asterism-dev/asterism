use std::collections::{BTreeSet, HashMap, VecDeque};
use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use asterism_plugin::protocol::{method, InitializeParams, InitializeResult, SettingsChangedParams, HOST_PREFIX, PROTOCOL};
use asterism_proto::rpc::{ErrorKind, Request, Response, RpcError};
use serde_json::{Map, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::process::{Child, ChildStdout, Command};
use tokio::sync::{mpsc, oneshot};

use crate::error::{Error, Result};
use crate::lock;

pub const CALL_TIMEOUT: Duration = Duration::from_secs(20);
pub const IDLE_TIMEOUT: Duration = Duration::from_secs(60);
const CRASH_LIMIT: usize = 3;
const CRASH_WINDOW: Duration = Duration::from_secs(60);

pub type HostFn = Arc<
    dyn Fn(String, Value) -> Pin<Box<dyn Future<Output = std::result::Result<Value, RpcError>> + Send>> + Send + Sync,
>;

pub struct BackendConfig {
    pub plugin: String,
    pub argv: Vec<String>,
    pub dir: PathBuf,
    pub data_dir: PathBuf,
    /// The complete environment; nothing else is inherited.
    pub env: Vec<(String, String)>,
    pub capabilities: BTreeSet<String>,
    pub call_timeout: Duration,
    pub idle: Duration,
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<std::result::Result<Value, RpcError>>>>>;

struct Running {
    generation: u64,
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    child: Child,
    /// Cleared by the reader at EOF, before it fails the waiters.
    alive: Arc<AtomicBool>,
    /// Set when asterism kills the process, so the exit is not counted as a crash.
    stopped: Arc<AtomicBool>,
}

impl Running {
    fn kill(&mut self) {
        self.stopped.store(true, Ordering::SeqCst);
        let _ = self.child.start_kill();
    }
}

type Handles = (mpsc::UnboundedSender<String>, Pending, u64, Arc<AtomicBool>);

struct Reader {
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    generation: u64,
    alive: Arc<AtomicBool>,
    stopped: Arc<AtomicBool>,
}

pub struct Backend {
    config: BackendConfig,
    host: HostFn,
    settings: Mutex<Map<String, Value>>,
    running: tokio::sync::Mutex<Option<Running>>,
    generation: AtomicU64,
    next_id: AtomicU64,
    in_flight: AtomicUsize,
    last_used: Mutex<Instant>,
    crashes: Mutex<VecDeque<Instant>>,
    failing: Mutex<Option<String>>,
}

/// Bookkeeping for one call; an abandoned call without a timeout takes the process down with it.
struct CallGuard {
    backend: Arc<Backend>,
    generation: Option<u64>,
    kill_on_cancel: bool,
    done: bool,
}

impl Drop for CallGuard {
    fn drop(&mut self) {
        self.backend.in_flight.fetch_sub(1, Ordering::SeqCst);
        *lock(&self.backend.last_used) = Instant::now();
        let Ok(runtime) = tokio::runtime::Handle::try_current() else { return };
        let backend = self.backend.clone();
        match (self.done, self.kill_on_cancel, self.generation) {
            (false, true, Some(generation)) => {
                runtime.spawn(async move { backend.stop_generation(generation).await });
            }
            _ => {
                let idle = backend.config.idle;
                runtime.spawn(async move {
                    tokio::time::sleep(idle).await;
                    let quiet = backend.in_flight.load(Ordering::SeqCst) == 0 && lock(&backend.last_used).elapsed() >= idle;
                    if quiet {
                        backend.stop().await;
                    }
                });
            }
        }
    }
}

impl Backend {
    pub fn new(config: BackendConfig, host: HostFn, settings: Map<String, Value>) -> Arc<Self> {
        Arc::new(Self {
            config,
            host,
            settings: Mutex::new(settings),
            running: tokio::sync::Mutex::new(None),
            generation: AtomicU64::new(0),
            next_id: AtomicU64::new(1),
            in_flight: AtomicUsize::new(0),
            last_used: Mutex::new(Instant::now()),
            crashes: Mutex::new(VecDeque::new()),
            failing: Mutex::new(None),
        })
    }

    pub fn failing(&self) -> Option<String> {
        lock(&self.failing).clone()
    }

    pub async fn is_running(&self) -> bool {
        self.running.lock().await.is_some()
    }

    fn error(&self, message: impl std::fmt::Display) -> Error {
        Error::new(ErrorKind::PluginError, format!("plugin {}: {message}", self.config.plugin))
    }

    pub async fn call(self: &Arc<Self>, method: &str, params: Value, timeout: Option<Duration>) -> Result<Value> {
        if let Some(reason) = self.failing() {
            return Err(self.error(reason));
        }
        self.in_flight.fetch_add(1, Ordering::SeqCst);
        let mut guard = CallGuard { backend: self.clone(), generation: None, kill_on_cancel: timeout.is_none(), done: false };
        let (out, pending, generation, alive) = self.ensure_running().await?;
        guard.generation = Some(generation);
        let result = self.request(&out, &pending, &alive, method, params, timeout).await;
        guard.done = true;
        result
    }

    pub async fn stop(&self) {
        if let Some(mut running) = self.running.lock().await.take() {
            running.kill();
        }
    }

    async fn stop_generation(&self, generation: u64) {
        let mut running = self.running.lock().await;
        if running.as_ref().is_some_and(|r| r.generation == generation) {
            if let Some(mut r) = running.take() {
                r.kill();
            }
        }
    }

    /// A backend that cannot take the new settings is stopped and restarts with them on its next call.
    pub async fn update_settings(self: &Arc<Self>, settings: Map<String, Value>) {
        *lock(&self.settings) = settings.clone();
        if !self.is_running().await {
            return;
        }
        let params = serde_json::to_value(SettingsChangedParams { settings }).unwrap_or_default();
        if self.call(method::SETTINGS_CHANGED, params, Some(self.config.call_timeout)).await.is_err() {
            self.stop().await;
        }
    }

    /// The lock is held through the handshake so no request overtakes `initialize`.
    async fn ensure_running(self: &Arc<Self>) -> Result<Handles> {
        let mut running = self.running.lock().await;
        if let Some(r) = running.as_ref().filter(|r| r.alive.load(Ordering::SeqCst)) {
            return Ok((r.out.clone(), r.pending.clone(), r.generation, r.alive.clone()));
        }
        let started = self.spawn()?;
        let handles = (started.out.clone(), started.pending.clone(), started.generation, started.alive.clone());
        *running = Some(started);
        if let Err(e) = self.handshake(&handles.0, &handles.1, &handles.3).await {
            if let Some(mut r) = running.take() {
                r.kill();
            }
            if self.failing().is_none() {
                self.record_crash();
            }
            return Err(e);
        }
        Ok(handles)
    }

    fn spawn(self: &Arc<Self>) -> Result<Running> {
        let generation = self.generation.fetch_add(1, Ordering::SeqCst) + 1;
        let (program, args) = self.config.argv.split_first().ok_or_else(|| self.error("empty backend command"))?;
        let mut child = Command::new(program)
            .args(args)
            .current_dir(&self.config.dir)
            .env_clear()
            .envs(self.config.env.iter().map(|(k, v)| (k, v)))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true)
            .spawn()
            .map_err(|e| self.error(format!("cannot start {program}: {e}")))?;
        let (out, mut out_rx) = mpsc::unbounded_channel::<String>();
        let pending = Pending::default();
        if let Some(mut stdin) = child.stdin.take() {
            tokio::spawn(async move {
                while let Some(mut line) = out_rx.recv().await {
                    line.push('\n');
                    if stdin.write_all(line.as_bytes()).await.is_err() {
                        break;
                    }
                }
            });
        }
        if let Some(stderr) = child.stderr.take() {
            let name = self.config.plugin.clone();
            tokio::spawn(async move {
                let mut lines = BufReader::new(stderr).lines();
                while let Ok(Some(line)) = lines.next_line().await {
                    eprintln!("plugin {name}: {line}");
                }
            });
        }
        let (alive, stopped) = (Arc::new(AtomicBool::new(true)), Arc::new(AtomicBool::new(false)));
        if let Some(stdout) = child.stdout.take() {
            let reader = Reader { out: out.clone(), pending: pending.clone(), generation, alive: alive.clone(), stopped: stopped.clone() };
            tokio::spawn(self.clone().read_loop(stdout, reader));
        }
        Ok(Running { generation, out, pending, child, alive, stopped })
    }

    async fn handshake(&self, out: &mpsc::UnboundedSender<String>, pending: &Pending, alive: &AtomicBool) -> Result<()> {
        std::fs::create_dir_all(&self.config.data_dir)?;
        let params = InitializeParams {
            protocol: PROTOCOL,
            plugin_dir: self.config.dir.display().to_string(),
            data_dir: self.config.data_dir.display().to_string(),
            asterism_version: env!("CARGO_PKG_VERSION").into(),
            settings: lock(&self.settings).clone(),
        };
        let params = serde_json::to_value(params).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
        let reply = self.request(out, pending, alive, method::INITIALIZE, params, Some(self.config.call_timeout)).await?;
        let result: InitializeResult =
            serde_json::from_value(reply).map_err(|e| self.error(format!("invalid initialize reply: {e}")))?;
        let reported: BTreeSet<String> = result.capabilities.into_iter().collect();
        if reported != self.config.capabilities {
            let reason = format!("backend reports capabilities {reported:?} but plugin.toml declares {:?}", self.config.capabilities);
            *lock(&self.failing) = Some(reason.clone());
            return Err(self.error(reason));
        }
        Ok(())
    }

    async fn request(
        &self,
        out: &mpsc::UnboundedSender<String>,
        pending: &Pending,
        alive: &AtomicBool,
        method: &str,
        params: Value,
        timeout: Option<Duration>,
    ) -> Result<Value> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(pending).insert(id, tx);
        // Checked after inserting: a reader that already drained the waiters has cleared `alive` first.
        let line = serde_json::to_string(&Request::new(id, method, params)).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
        if !alive.load(Ordering::SeqCst) || out.send(line).is_err() {
            lock(pending).remove(&id);
            return Err(self.error("exited"));
        }
        let reply = match timeout {
            Some(limit) => match tokio::time::timeout(limit, rx).await {
                Ok(reply) => reply,
                Err(_) => {
                    lock(pending).remove(&id);
                    let message = format!("plugin {} did not answer {method} within {limit:?}", self.config.plugin);
                    return Err(Error::new(ErrorKind::Timeout, message));
                }
            },
            None => rx.await,
        };
        match reply {
            Ok(Ok(value)) => Ok(value),
            Ok(Err(e)) => Err(match e.kind() {
                ErrorKind::InvalidParams | ErrorKind::Git | ErrorKind::NotFound | ErrorKind::Timeout => Error::new(e.kind(), e.message),
                _ => Error::new(ErrorKind::PluginError, format!("{}: {}", self.config.plugin, e.message)),
            }),
            Err(_) => Err(self.error("exited")),
        }
    }

    async fn read_loop(self: Arc<Self>, stdout: ChildStdout, reader: Reader) {
        let Reader { out, pending, generation, alive, stopped } = reader;
        let mut lines = BufReader::new(stdout).lines();
        while let Ok(Some(line)) = lines.next_line().await {
            let Ok(message) = serde_json::from_str::<Value>(&line) else {
                eprintln!("plugin {}: ignoring non-JSON output: {}", self.config.plugin, crate::git::truncate(&line));
                continue;
            };
            if message.get("method").is_some() {
                if let Ok(request) = serde_json::from_value::<Request>(message) {
                    self.serve_host(request, out.clone());
                }
            } else if let Ok(response) = serde_json::from_value::<Response>(message) {
                if let Some(tx) = lock(&pending).remove(&response.id) {
                    let _ = tx.send(match response.error {
                        Some(e) => Err(e),
                        None => Ok(response.result.unwrap_or(Value::Null)),
                    });
                }
            }
        }
        alive.store(false, Ordering::SeqCst);
        // Waiters learn first, so a handshake holding `running` never blocks this task.
        let exited = RpcError::new(ErrorKind::PluginError, format!("plugin {} exited", self.config.plugin));
        for (_, tx) in lock(&pending).drain() {
            let _ = tx.send(Err(exited.clone()));
        }
        if !stopped.load(Ordering::SeqCst) {
            self.record_crash();
        }
        let mut running = self.running.lock().await;
        if running.as_ref().is_some_and(|r| r.generation == generation) {
            *running = None;
        }
    }

    fn serve_host(&self, request: Request, out: mpsc::UnboundedSender<String>) {
        let host = self.host.clone();
        tokio::spawn(async move {
            let id = request.id;
            let result = match request.method.strip_prefix(HOST_PREFIX) {
                Some(method) => host(method.to_string(), request.params).await,
                None => Err(RpcError::new(ErrorKind::MethodNotFound, format!("unknown method {}", request.method))),
            };
            let response = match result {
                Ok(value) => Response::ok(id, value),
                Err(e) => Response::err(id, e),
            };
            if let Ok(line) = serde_json::to_string(&response) {
                let _ = out.send(line);
            }
        });
    }

    fn record_crash(&self) {
        let now = Instant::now();
        let mut crashes = lock(&self.crashes);
        crashes.push_back(now);
        while crashes.front().is_some_and(|t| now.duration_since(*t) > CRASH_WINDOW) {
            crashes.pop_front();
        }
        if crashes.len() >= CRASH_LIMIT {
            *lock(&self.failing) = Some(format!("crashed {CRASH_LIMIT} times within a minute; reload the plugin to retry"));
        }
    }
}
