use std::collections::hash_map::RandomState;
use std::collections::HashMap;
use std::hash::{BuildHasher, Hasher};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use asterism_proto::client::{Client, ClientError};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{
    method, ClientKind, Event, HelloParams, HelloResult, Machine, MachineState, Role, Session,
    SessionListParams, SessionStatus,
};
use asterism_proto::{PROTO_VERSION, PROTO_VERSION_PREVIOUS};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::process::Child;
use tokio::sync::{broadcast, mpsc, oneshot, Notify};
use tokio::task::{JoinHandle, JoinSet};

use super::install::{self, InstallError, Ssh};
use crate::daemon::Daemon;
use crate::error::Error;
use crate::lock;

const ATTACH: &str = r#"ASTERISM_HOME="$HOME/.asterism-machine" ASTERISM_ROLE=machine exec "$HOME/.asterism-machine/bin/$1/asterism" attach"#;
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const STDERR_GRACE: Duration = Duration::from_secs(1);
const AUTH_ERRORS: &[&str] = &[
    "Permission denied",
    "Host key verification failed",
    "REMOTE HOST IDENTIFICATION HAS CHANGED",
];
const SSH_FAILED: i32 = 255;
const NOT_A_MACHINE: &str = "not an asterism machine daemon";
const INCOMPATIBLE: &str = "incompatible version";

/// Methods with the protocol version that added them; extend on each protocol bump.
const ADDED_IN: &[(&str, u32)] = &[];

fn link_supports(protocol: u32, method_name: &str) -> bool {
    ADDED_IN
        .iter()
        .all(|&(added, since)| added != method_name || protocol >= since)
}

pub(crate) struct Settings {
    pub ssh_bin: PathBuf,
    pub bins_dir: Option<PathBuf>,
    pub health_interval: Duration,
    pub health_timeout: Duration,
    pub backoff_max: Duration,
    pub open_deadline: Duration,
    pub call_timeout: Duration,
    pub events: broadcast::Sender<Event>,
    pub daemon: Weak<Daemon>,
    /// Per destination, so `machine.test` never uploads over a running install.
    pub installs: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

#[derive(Debug)]
pub(crate) enum Failure {
    /// Needs the user to act; no automatic retry.
    Blocked(String),
    Offline(String),
}

impl Failure {
    pub fn message(self) -> String {
        match self {
            Failure::Blocked(m) | Failure::Offline(m) => m,
        }
    }
}

fn classify(status: Option<i32>, line: String) -> Failure {
    if status == Some(SSH_FAILED) && AUTH_ERRORS.iter().any(|e| line.contains(e)) {
        Failure::Blocked(line)
    } else {
        Failure::Offline(line)
    }
}

fn install_failure(e: InstallError) -> Failure {
    match e {
        InstallError::Ssh { status, line } => classify(status, line),
        e => Failure::Blocked(e.to_string()),
    }
}

struct State {
    machine: Machine,
    client: Option<Arc<Client>>,
    closed: bool,
    /// The caller of a running Update now, answered once the machine is ready again or the update failed.
    update: Option<oneshot::Sender<Result<(), String>>>,
}

impl State {
    fn finish_update(&mut self, result: Result<(), String>) {
        if let Some(reply) = self.update.take() {
            let _ = reply.send(result);
        }
    }
}

/// What the registry and the connection task share for one machine.
pub(crate) struct Shared {
    pub(super) settings: Arc<Settings>,
    state: Mutex<State>,
    retry: Notify,
    update_requested: Notify,
    /// `attach_output` subscribers per remote session id; kept across reconnects.
    pub(super) outputs: Mutex<HashMap<i64, Vec<mpsc::Sender<Vec<u8>>>>>,
}

impl Shared {
    pub fn new(settings: Arc<Settings>, machine: Machine) -> Self {
        Self {
            settings,
            state: Mutex::new(State {
                machine,
                client: None,
                closed: false,
                update: None,
            }),
            retry: Notify::new(),
            update_requested: Notify::new(),
            outputs: Mutex::default(),
        }
    }

    pub fn machine(&self) -> Machine {
        lock(&self.state).machine.clone()
    }

    pub fn client(&self) -> Option<Arc<Client>> {
        lock(&self.state).client.clone()
    }

    /// After this no more `machine.changed` events go out, even from a task that is still unwinding.
    pub fn close(&self) {
        let mut state = lock(&self.state);
        state.closed = true;
        state.client = None;
        state.update = None;
    }

    pub fn retry(&self) {
        let state = lock(&self.state).machine.status.state;
        if matches!(state, MachineState::Offline | MachineState::Blocked) {
            self.retry.notify_one();
        }
    }

    /// Has the supervisor restart the machine daemon on the current build; returns once it is ready on it.
    pub async fn request_update(&self) -> Result<(), Error> {
        let done = {
            let mut state = lock(&self.state);
            let status = &state.machine.status;
            match status.state {
                MachineState::Ready if !status.update_available => return Ok(()),
                MachineState::Ready => {}
                MachineState::Blocked if status.error.as_deref() == Some(INCOMPATIBLE) => {}
                _ => {
                    return Err(Error::new(
                        ErrorKind::InvalidParams,
                        format!("machine {} is not ready", state.machine.name),
                    ))
                }
            }
            if state.update.is_some() {
                return Err(Error::new(
                    ErrorKind::InvalidParams,
                    format!("machine {} is already updating", state.machine.name),
                ));
            }
            let (reply, done) = oneshot::channel();
            state.update = Some(reply);
            done
        };
        self.update_requested.notify_one();
        match done.await {
            Ok(result) => result.map_err(|e| Error::new(ErrorKind::Internal, e)),
            Err(_) => Err(Error::new(
                ErrorKind::Internal,
                "the machine was removed during the update",
            )),
        }
    }

    fn updating(&self) -> bool {
        lock(&self.state).update.is_some()
    }

    fn finish_update(&self, result: Result<(), String>) {
        lock(&self.state).finish_update(result);
    }

    pub fn publish(&self) {
        let state = lock(&self.state);
        if !state.closed {
            let _ = self
                .settings
                .events
                .send(Event::MachineChanged(state.machine.clone()));
        }
    }

    fn update(&self, f: impl FnOnce(&mut State)) {
        let mut state = lock(&self.state);
        if state.closed {
            return;
        }
        let before = state.machine.status.clone();
        f(&mut state);
        if state.machine.status != before {
            let _ = self
                .settings
                .events
                .send(Event::MachineChanged(state.machine.clone()));
        }
    }

    fn set(&self, new: MachineState, error: Option<String>) {
        self.update(|state| {
            state.machine.status.state = new;
            state.machine.status.error = error;
            state.machine.status.protocol = None;
            state.client = None;
        });
    }
}

pub(crate) async fn run(shared: Arc<Shared>) {
    let initial = INITIAL_BACKOFF.min(shared.settings.backoff_max);
    let mut backoff = initial;
    loop {
        let failure = connect(&shared, &mut backoff).await;
        let (Failure::Blocked(reason) | Failure::Offline(reason)) = &failure;
        shared.finish_update(Err(reason.clone()));
        match failure {
            Failure::Blocked(reason) => {
                shared.set(MachineState::Blocked, Some(reason));
                wait_blocked(&shared).await;
                backoff = initial;
            }
            Failure::Offline(reason) => {
                shared.set(MachineState::Offline, Some(reason));
                tokio::select! {
                    _ = tokio::time::sleep(jitter(backoff)) => {}
                    _ = shared.retry.notified() => {}
                }
                backoff = (backoff * 2).min(shared.settings.backoff_max);
            }
        }
    }
}

/// Until a retry, or an Update now that stopped the blocked machine daemon.
async fn wait_blocked(shared: &Shared) {
    loop {
        tokio::select! {
            _ = shared.retry.notified() => return,
            _ = shared.update_requested.notified() => {}
        }
        if shared.updating() {
            match stop_blocked(shared).await {
                Ok(()) => return,
                Err(failure) => shared.finish_update(Err(failure.message())),
            }
        }
    }
}

async fn stop_blocked(shared: &Shared) -> Result<(), Failure> {
    let key = install_current(shared).await?;
    // The daemon serves `shutdown` without a successful `hello`, so even an incompatible one stops.
    let link = Link::spawn(&shared.settings, &shared.machine().destination, &key)?;
    link.shutdown(&shared.settings).await;
    Ok(())
}

async fn install_current(shared: &Shared) -> Result<String, Failure> {
    let settings = &shared.settings;
    let destination = shared.machine().destination;
    let ssh = Ssh::new(settings.ssh_bin.clone(), destination.clone());
    let platform = install::probe(&ssh, settings.open_deadline)
        .await
        .map_err(install_failure)?;
    ensure_installed(settings, &ssh, &destination, &platform).await
}

/// Runs until the connection is lost; resets `backoff` once the machine daemon answers.
async fn connect(shared: &Arc<Shared>, backoff: &mut Duration) -> Failure {
    shared.set(MachineState::Connecting, None);
    let settings = &shared.settings;
    let destination = shared.machine().destination;
    let ssh = Ssh::new(settings.ssh_bin.clone(), destination.clone());
    let platform = match install::probe(&ssh, settings.open_deadline).await {
        Ok(platform) => platform,
        Err(e) => return install_failure(e),
    };
    shared.update(|state| state.machine.status.platform = Some(platform.to_string()));
    let mut key = match ensure_installed(settings, &ssh, &destination, &platform).await {
        Ok(key) => key,
        Err(failure) => return failure,
    };
    let mut upgraded = false;
    loop {
        let (link, hello) = match Link::open(settings, &destination, &key).await {
            Ok(opened) => opened,
            Err(failure) => return failure,
        };
        if hello.role != Role::Machine {
            return Failure::Blocked(NOT_A_MACHINE.into());
        }
        let mut update_available = link.protocol != PROTO_VERSION;
        if !runs_from(&hello.exe, &key) {
            let sessions = tokio::time::timeout(
                settings.health_timeout,
                link.client.call::<_, Vec<Session>>(
                    method::SESSION_LIST,
                    SessionListParams { task_id: None },
                ),
            )
            .await;
            match sessions {
                Ok(Ok(sessions))
                    if sessions.iter().all(|s| s.status == SessionStatus::Exited) && !upgraded =>
                {
                    link.shutdown(settings).await;
                    upgraded = true;
                    continue;
                }
                Ok(Ok(_)) => update_available = true,
                Ok(Err(e)) => return link.failure(e).await,
                Err(_) => return Failure::Offline("the machine daemon did not answer".into()),
            }
        }
        if upgraded && runs_from(&hello.exe, &key) {
            // Best effort: a leftover install only costs disk space until the next restart.
            let _ = install::remove_other_installs(&ssh, &key, settings.open_deadline).await;
        }
        *backoff = INITIAL_BACKOFF.min(settings.backoff_max);
        let client = link.client.clone();
        let Some(events) = client.take_events() else {
            return Failure::Offline("the machine's event stream is gone".into());
        };
        let Some(requests) = client.take_requests() else {
            return Failure::Offline("the machine's request stream is gone".into());
        };
        // Calls the machine forwards before `peer::serve` starts wait in the client's request queue.
        if link_supports(link.protocol, method::UPLINK_REGISTER) {
            match tokio::time::timeout(
                settings.health_timeout,
                client.call::<_, Value>(method::UPLINK_REGISTER, Value::Null),
            )
            .await
            {
                Ok(Ok(_)) => {}
                // An older daemon kept for its sessions does not forward, so it needs no uplink.
                Ok(Err(ClientError::Rpc(e))) if e.kind() == ErrorKind::MethodNotFound => {}
                Ok(Err(e)) => return link.failure(e).await,
                Err(_) => return Failure::Offline("the machine daemon did not answer".into()),
            }
        }
        match tokio::time::timeout(
            settings.health_timeout,
            client.call::<_, Value>(method::SUBSCRIBE, Value::Null),
        )
        .await
        {
            Ok(Ok(_)) => {}
            Ok(Err(e)) => return link.failure(e).await,
            Err(_) => return Failure::Offline("the machine daemon did not answer".into()),
        }
        // Dropped with this connection, which aborts the pump.
        let mut pump = JoinSet::new();
        pump.spawn(super::link::pump(shared.clone(), client.clone(), events));
        shared.update(|state| {
            state.machine.status.state = MachineState::Ready;
            state.machine.status.error = None;
            state.machine.status.update_available = update_available;
            state.machine.status.agents = hello.agents;
            state.machine.status.protocol = Some(link.protocol);
            state.client = Some(client.clone());
            state.finish_update(if update_available {
                Err("the machine daemon still runs an older build".into())
            } else {
                Ok(())
            });
        });
        // After Ready, so a queued forwarded call that calls back down the pipe finds the client.
        pump.spawn(super::peer::serve(shared.clone(), client, requests));
        match link.supervise(shared).await {
            Supervised::Lost(failure) => return failure,
            Supervised::Restarting(installed) => {
                shared.set(MachineState::Connecting, None);
                key = installed;
                upgraded = true;
            }
        }
    }
}

enum Supervised {
    Lost(Failure),
    /// Update now stopped the machine daemon; reconnect onto install `key`.
    Restarting(String),
}

/// Probes, installs and says `hello` once over a throwaway connection; returns the platform.
pub(crate) async fn test(settings: &Settings, destination: &str) -> Result<String, Failure> {
    let ssh = Ssh::new(settings.ssh_bin.clone(), destination.to_string());
    let platform = install::probe(&ssh, settings.open_deadline)
        .await
        .map_err(install_failure)?;
    let key = ensure_installed(settings, &ssh, destination, &platform).await?;
    let (_link, hello) = Link::open(settings, destination, &key).await?;
    if hello.role != Role::Machine {
        return Err(Failure::Blocked(NOT_A_MACHINE.into()));
    }
    Ok(platform.to_string())
}

async fn ensure_installed(
    settings: &Settings,
    ssh: &Ssh,
    destination: &str,
    platform: &install::Platform,
) -> Result<String, Failure> {
    let Some(bins_dir) = &settings.bins_dir else {
        return Err(Failure::Blocked(
            "machine binaries are not bundled with this build".into(),
        ));
    };
    let guard = lock(&settings.installs)
        .entry(destination.to_string())
        .or_default()
        .clone();
    let _installing = guard.lock().await;
    install::ensure_installed(ssh, platform, bins_dir, settings.open_deadline)
        .await
        .map(|installed| installed.key)
        .map_err(install_failure)
}

fn runs_from(exe: &str, key: &str) -> bool {
    Path::new(exe)
        .parent()
        .is_some_and(|dir| dir.ends_with(Path::new("bin").join(key)))
}

/// One ssh process running `asterism attach` on the machine, with a client over its stdio.
struct Link {
    child: Child,
    client: Arc<Client>,
    stderr: JoinHandle<Option<String>>,
    protocol: u32,
}

impl Link {
    fn spawn(settings: &Settings, destination: &str, key: &str) -> Result<Self, Failure> {
        let mut child = Ssh::new(settings.ssh_bin.clone(), destination.to_string())
            .command(ATTACH, &[key])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| Failure::Offline(format!("starting ssh: {e}")))?;
        let (Some(stdin), Some(stdout), Some(stderr)) =
            (child.stdin.take(), child.stdout.take(), child.stderr.take())
        else {
            return Err(Failure::Offline("ssh has no stdio".into()));
        };
        Ok(Link {
            child,
            client: Arc::new(Client::new(stdout, stdin)),
            stderr: tokio::spawn(last_line(stderr)),
            protocol: PROTO_VERSION,
        })
    }

    async fn open(
        settings: &Settings,
        destination: &str,
        key: &str,
    ) -> Result<(Self, HelloResult), Failure> {
        let mut link = Self::spawn(settings, destination, key)?;
        let opened = tokio::time::timeout(settings.open_deadline, async {
            match hello(&link.client, PROTO_VERSION).await {
                Err(ClientError::Rpc(e)) if e.kind() == ErrorKind::IncompatibleVersion => {
                    link.protocol = PROTO_VERSION_PREVIOUS;
                    hello(&link.client, PROTO_VERSION_PREVIOUS).await
                }
                result => result,
            }
        })
        .await;
        match opened {
            Ok(Ok(hello)) => Ok((link, hello)),
            Ok(Err(ClientError::Rpc(e))) if e.kind() == ErrorKind::IncompatibleVersion => {
                Err(Failure::Blocked(INCOMPATIBLE.into()))
            }
            Ok(Err(e)) => Err(link.failure(e).await),
            Err(_) => Err(Failure::Offline("the machine daemon did not answer".into())),
        }
    }

    async fn supervise(mut self, shared: &Shared) -> Supervised {
        let settings = &shared.settings;
        loop {
            let healthy = tokio::select! {
                _ = self.child.wait() => None,
                _ = shared.update_requested.notified() => Some(None),
                _ = tokio::time::sleep(settings.health_interval) => {
                    Some(Some(tokio::time::timeout(settings.health_timeout, hello(&self.client, self.protocol)).await))
                }
            };
            match healthy {
                Some(None) if shared.updating() => match install_current(shared).await {
                    Ok(key) => {
                        self.shutdown(settings).await;
                        return Supervised::Restarting(key);
                    }
                    // The old daemon keeps running.
                    Err(failure) => shared.finish_update(Err(failure.message())),
                },
                Some(None) | Some(Some(Ok(Ok(_)))) => {}
                Some(Some(Ok(Err(e)))) => return Supervised::Lost(self.failure(e).await),
                Some(Some(Err(_))) => {
                    return Supervised::Lost(Failure::Offline("health check timed out".into()))
                }
                None => return Supervised::Lost(self.failure(ClientError::Closed).await),
            }
        }
    }

    /// Returns once `attach` has exited, which it does when the daemon closes its socket.
    async fn shutdown(mut self, settings: &Settings) {
        let _ = tokio::time::timeout(
            settings.health_timeout,
            self.client.call::<_, Value>(method::SHUTDOWN, Value::Null),
        )
        .await;
        let _ = tokio::time::timeout(settings.open_deadline, self.child.wait()).await;
    }

    /// Prefers ssh's own last stderr line, which says why the connection went away.
    async fn failure(mut self, error: ClientError) -> Failure {
        if let ClientError::Rpc(e) = error {
            return Failure::Offline(e.to_string());
        }
        let status = tokio::time::timeout(STDERR_GRACE, self.child.wait())
            .await
            .ok()
            .and_then(Result::ok)
            .and_then(|status| status.code());
        let line = tokio::time::timeout(STDERR_GRACE, self.stderr)
            .await
            .ok()
            .and_then(Result::ok)
            .flatten();
        classify(status, line.unwrap_or_else(|| error.to_string()))
    }
}

async fn hello(client: &Client, protocol: u32) -> Result<HelloResult, ClientError> {
    client
        .call(
            method::HELLO,
            HelloParams {
                proto_version: protocol,
                client_kind: ClientKind::Cli,
            },
        )
        .await
}

async fn last_line(stderr: impl AsyncRead + Unpin) -> Option<String> {
    let mut lines = BufReader::new(stderr).lines();
    let mut last = None;
    while let Ok(Some(line)) = lines.next_line().await {
        if !line.trim().is_empty() {
            last = Some(line);
        }
    }
    last
}

/// ±20 %, so machines that dropped together do not reconnect in lockstep.
fn jitter(base: Duration) -> Duration {
    let random = RandomState::new().build_hasher().finish();
    base.mul_f64(0.8 + (random % 1000) as f64 / 1000.0 * 0.4)
}
