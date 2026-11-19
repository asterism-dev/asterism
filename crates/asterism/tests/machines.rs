mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use asterism_core::daemon::{Daemon, DaemonOptions};
use asterism_core::machines::install::{self, InstallError};
use asterism_core::paths::Paths;
use asterism_core::store::Store;
use asterism_proto::client::{Client, ClientError};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{method, *};
use asterism_proto::PROTO_VERSION;
use common::{host_platform, FakeSsh};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tempfile::TempDir;
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::mpsc::UnboundedReceiver;

const DEADLINE: Duration = Duration::from_secs(30);

struct D {
    _home: TempDir,
    client: Client,
}

impl D {
    async fn call<P: Serialize, T: DeserializeOwned>(&self, name: &str, params: P) -> T {
        self.client.call(name, params).await.unwrap()
    }

    fn events(&self) -> UnboundedReceiver<Event> {
        self.client.take_events().unwrap()
    }
}

async fn start_with_fake_ssh() -> (D, FakeSsh) {
    start_with_role(Role::Node).await
}

async fn start_with_role(role: Role) -> (D, FakeSsh) {
    start(role, Duration::from_secs(30)).await
}

fn options(fake: &FakeSsh, role: Role, open_deadline: Duration) -> DaemonOptions {
    DaemonOptions {
        ssh_bin: fake.bin(),
        machine_bins_dir: Some(fake.bins_dir()),
        machine_health_interval: Duration::from_millis(200),
        machine_health_timeout: Duration::from_secs(2),
        machine_backoff_max: Duration::from_millis(300),
        machine_open_deadline: open_deadline,
        role,
        ..common::daemon_options()
    }
}

async fn hello(client: &Client) {
    let _: HelloResult = client
        .call(
            method::HELLO,
            HelloParams {
                proto_version: PROTO_VERSION,
                client_kind: ClientKind::Cli,
            },
        )
        .await
        .unwrap();
}

async fn subscribed_client(socket: &Path) -> Client {
    for _ in 0..100 {
        if UnixStream::connect(socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let client = Client::connect_unix(socket).await.unwrap();
    hello(&client).await;
    let _: Value = client.call(method::SUBSCRIBE, Value::Null).await.unwrap();
    client
}

async fn start(role: Role, open_deadline: Duration) -> (D, FakeSsh) {
    let fake = FakeSsh::new();
    let home = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: home.path().to_path_buf(),
    };
    tokio::spawn(asterism_core::run_with_options(
        paths.clone(),
        options(&fake, role, open_deadline),
    ));
    let client = subscribed_client(&paths.socket()).await;
    (
        D {
            _home: home,
            client,
        },
        fake,
    )
}

/// An in-process node, so tests can reach `Machines` directly.
async fn hub(call_timeout: Duration) -> (Arc<Daemon>, D, FakeSsh) {
    let fake = FakeSsh::new();
    let home = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: home.path().to_path_buf(),
    };
    let options = DaemonOptions {
        machine_call_timeout: call_timeout,
        ..options(&fake, Role::Node, DEADLINE)
    };
    let daemon = Daemon::with_options(paths.clone(), options).unwrap();
    let listener = UnixListener::bind(paths.socket()).unwrap();
    tokio::spawn(asterism_core::rpc::serve(daemon.clone(), listener));
    let client = subscribed_client(&paths.socket()).await;
    (
        daemon,
        D {
            _home: home,
            client,
        },
        fake,
    )
}

fn hub_store(d: &D) -> Store {
    let paths = Paths {
        home: d._home.path().to_path_buf(),
    };
    Store::open(&paths.db()).unwrap()
}

async fn add_machine(d: &D, name: &str) -> Machine {
    d.call(
        method::MACHINE_ADD,
        MachineAddParams {
            name: name.into(),
            destination: name.into(),
        },
    )
    .await
}

async fn machine(d: &D, id: i64) -> Machine {
    let machines: Vec<Machine> = d.call(method::MACHINE_LIST, Value::Null).await;
    machines.into_iter().find(|m| m.id == id).unwrap()
}

async fn eventually_state(d: &D, id: i64, state: MachineState) -> bool {
    // Generous: each test uploads ~60 MB of debug binaries and macOS vets every new executable.
    let deadline = Instant::now() + Duration::from_secs(30);
    while Instant::now() < deadline {
        if machine(d, id).await.status.state == state {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

fn kill_machine_daemon(fake: &FakeSsh) {
    fake.stop_daemon();
}

/// Events received within `window` after the `machine.removed` event for `id`.
async fn events_after_removed(
    mut events: UnboundedReceiver<Event>,
    id: i64,
    window: Duration,
) -> Vec<Event> {
    loop {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
        if matches!(event, Event::MachineRemoved { machine_id } if machine_id == id) {
            break;
        }
    }
    let mut after = Vec::new();
    let deadline = tokio::time::Instant::now() + window;
    while let Ok(Some(event)) = tokio::time::timeout_at(deadline, events.recv()).await {
        after.push(event);
    }
    after
}

/// Starts a daemon in the machine's home from outside `bin/<key>/`, as an older install would.
fn start_foreign_daemon(fake: &FakeSsh, role: &str) -> std::process::Child {
    let child = std::process::Command::new(env!("CARGO_BIN_EXE_asterismd"))
        .env_clear()
        .env("HOME", fake.home())
        .env("ASTERISM_HOME", fake.machine_home())
        .env("ASTERISM_ROLE", role)
        .env("PATH", std::env::var_os("PATH").unwrap_or_default())
        .spawn()
        .unwrap();
    let socket = fake.machine_home().join("asterismd.sock");
    for _ in 0..250 {
        if std::os::unix::net::UnixStream::connect(&socket).is_ok() {
            break;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    child
}

#[tokio::test]
async fn probe_reports_host_platform() {
    let fake = FakeSsh::new();
    let platform = install::probe(&fake.ssh("vm"), DEADLINE).await.unwrap();
    assert_eq!(platform.to_string(), host_platform());
}

#[tokio::test]
async fn install_uploads_binaries_once_into_a_home_with_spaces() {
    let fake = FakeSsh::new();
    assert!(fake.home().to_string_lossy().contains(' '));
    let ssh = fake.ssh("vm");
    let platform = install::probe(&ssh, DEADLINE).await.unwrap();
    let first = install::ensure_installed(&ssh, &platform, &fake.bins_dir(), DEADLINE)
        .await
        .unwrap();
    let dir = fake.home().join(".asterism-machine/bin").join(&first.key);
    assert!(dir.join("asterismd").is_file() && dir.join("asterism").is_file());
    let mtime = std::fs::metadata(dir.join("asterismd"))
        .unwrap()
        .modified()
        .unwrap();
    let second = install::ensure_installed(&ssh, &platform, &fake.bins_dir(), DEADLINE)
        .await
        .unwrap();
    assert_eq!(first.key, second.key);
    assert_eq!(
        mtime,
        std::fs::metadata(dir.join("asterismd"))
            .unwrap()
            .modified()
            .unwrap()
    );
}

#[tokio::test]
async fn changed_cli_is_uploaded_again() {
    let fake = FakeSsh::new();
    let ssh = fake.ssh("vm");
    let platform = install::probe(&ssh, DEADLINE).await.unwrap();
    let first = install::ensure_installed(&ssh, &platform, &fake.bins_dir(), DEADLINE)
        .await
        .unwrap();
    let cli = fake.bins_dir().join(host_platform()).join("asterism");
    let mut changed = std::fs::read(&cli).unwrap();
    changed.push(0);
    std::fs::remove_file(&cli).unwrap();
    std::fs::write(&cli, &changed).unwrap();
    let second = install::ensure_installed(&ssh, &platform, &fake.bins_dir(), DEADLINE)
        .await
        .unwrap();
    assert_ne!(first.key, second.key);
    let dir = fake.home().join(".asterism-machine/bin").join(&second.key);
    assert_eq!(std::fs::read(dir.join("asterism")).unwrap(), changed);
}

#[tokio::test]
async fn install_fails_without_binaries_for_the_platform() {
    let fake = FakeSsh::new();
    let empty = tempfile::tempdir().unwrap();
    let ssh = fake.ssh("vm");
    let platform = install::probe(&ssh, DEADLINE).await.unwrap();
    let err = install::ensure_installed(&ssh, &platform, empty.path(), DEADLINE)
        .await
        .unwrap_err();
    assert!(matches!(err, InstallError::MissingBinaries(_)));
}

#[tokio::test]
async fn ssh_failure_reports_the_last_stderr_line() {
    let fake = FakeSsh::new();
    fake.fail_with("Permission denied (publickey).", 255);
    let err = install::probe(&fake.ssh("vm"), DEADLINE).await.unwrap_err();
    assert!(matches!(
        err,
        InstallError::Ssh { status: Some(255), ref line } if line == "Permission denied (publickey)."
    ));
}

#[tokio::test]
async fn interrupted_install_is_uploaded_again() {
    let fake = FakeSsh::new();
    let ssh = fake.ssh("vm");
    let platform = install::probe(&ssh, DEADLINE).await.unwrap();
    let first = install::ensure_installed(&ssh, &platform, &fake.bins_dir(), DEADLINE)
        .await
        .unwrap();
    let dir = fake.home().join(".asterism-machine/bin").join(&first.key);
    std::fs::remove_file(dir.join("ok")).unwrap();
    std::fs::write(dir.join("asterismd"), b"truncated").unwrap();
    install::ensure_installed(&ssh, &platform, &fake.bins_dir(), DEADLINE)
        .await
        .unwrap();
    assert!(dir.join("ok").is_file());
    assert_eq!(
        std::fs::read(dir.join("asterismd")).unwrap(),
        std::fs::read(env!("CARGO_BIN_EXE_asterismd")).unwrap()
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn added_machine_becomes_ready() {
    let (d, fake) = start_with_fake_ssh().await;
    let m: Machine = d
        .call(
            method::MACHINE_ADD,
            MachineAddParams {
                name: "vm".into(),
                destination: "vm".into(),
            },
        )
        .await;
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
    assert_eq!(
        machine(&d, m.id).await.status.platform.as_deref(),
        Some(host_platform().as_str())
    );
    assert!(fake
        .home()
        .join(".asterism-machine/asterismd.sock")
        .exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn killed_machine_daemon_reconnects() {
    let (d, fake) = start_with_fake_ssh().await;
    let m = add_machine(&d, "vm").await;
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
    kill_machine_daemon(&fake);
    assert!(eventually_state(&d, m.id, MachineState::Offline).await);
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
}

#[tokio::test(flavor = "multi_thread")]
async fn auth_failure_blocks_without_retry() {
    let (d, fake) = start_with_fake_ssh().await;
    fake.fail_with("ubuntu@vm: Permission denied (publickey).", 255);
    let m = add_machine(&d, "vm").await;
    assert!(eventually_state(&d, m.id, MachineState::Blocked).await);
    assert!(machine(&d, m.id)
        .await
        .status
        .error
        .unwrap()
        .contains("Permission denied"));
    let calls = fake.invocations();
    tokio::time::sleep(Duration::from_millis(500)).await;
    assert_eq!(fake.invocations(), calls);
}

#[tokio::test(flavor = "multi_thread")]
async fn retry_reconnects_a_blocked_machine() {
    let (d, fake) = start_with_fake_ssh().await;
    fake.fail_with("Host key verification failed.", 255);
    let m = add_machine(&d, "vm").await;
    assert!(eventually_state(&d, m.id, MachineState::Blocked).await);
    fake.recover();
    d.call::<_, Value>(method::MACHINE_RETRY, MachineIdParams { machine_id: m.id })
        .await;
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
}

#[tokio::test(flavor = "multi_thread")]
async fn unreachable_machine_does_not_delay_other_requests() {
    let (d, fake) = start_with_fake_ssh().await;
    fake.hang();
    add_machine(&d, "vm").await;
    let started = Instant::now();
    d.call::<_, Vec<Project>>(method::PROJECT_LIST, Value::Null)
        .await;
    assert!(started.elapsed() < Duration::from_millis(200));
}

#[tokio::test(flavor = "multi_thread")]
async fn hanging_machine_goes_offline_and_test_fails() {
    let (d, fake) = start(Role::Node, Duration::from_millis(300)).await;
    fake.hang();
    let mut events = d.events();
    let m = add_machine(&d, "vm").await;
    let error = loop {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap();
        if let Event::MachineChanged(x) = event {
            if x.id == m.id && x.status.state == MachineState::Offline {
                break x.status.error.unwrap();
            }
        }
    };
    assert!(error.contains("did not answer"), "{error}");
    let err = tokio::time::timeout(
        Duration::from_secs(5),
        d.client.call::<_, MachineTestResult>(
            method::MACHINE_TEST,
            MachineTestParams {
                destination: "vm".into(),
            },
        ),
    )
    .await
    .unwrap()
    .unwrap_err();
    assert!(err.to_string().contains("did not answer"), "{err}");
}

#[tokio::test(flavor = "multi_thread")]
async fn removing_a_connecting_machine_stops_its_events() {
    let (d, fake) = start_with_fake_ssh().await;
    fake.hang();
    let events = d.events();
    let m = add_machine(&d, "vm").await;
    d.call::<_, Value>(method::MACHINE_REMOVE, MachineIdParams { machine_id: m.id })
        .await;
    let after = events_after_removed(events, m.id, Duration::from_millis(300)).await;
    assert!(after
        .iter()
        .all(|e| !matches!(e, Event::MachineChanged(x) if x.id == m.id)));
    let machines: Vec<Machine> = d.call(method::MACHINE_LIST, Value::Null).await;
    assert!(machines.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn destinations_that_look_like_ssh_options_are_rejected() {
    let (d, fake) = start_with_fake_ssh().await;
    for destination in ["-oProxyCommand=x", "", "a b", "vm\n"] {
        let err = d
            .client
            .call::<_, Machine>(
                method::MACHINE_ADD,
                MachineAddParams {
                    name: "vm".into(),
                    destination: destination.into(),
                },
            )
            .await
            .unwrap_err();
        assert!(
            matches!(err, ClientError::Rpc(ref e) if e.kind() == ErrorKind::InvalidParams),
            "{destination:?}: {err}"
        );
    }
    let err = d
        .client
        .call::<_, MachineTestResult>(
            method::MACHINE_TEST,
            MachineTestParams {
                destination: "-oProxyCommand=x".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(err, ClientError::Rpc(ref e) if e.kind() == ErrorKind::InvalidParams));
    assert_eq!(fake.invocations(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn test_connects_once_without_storing() {
    let (d, _fake) = start_with_fake_ssh().await;
    let result: MachineTestResult = d
        .call(
            method::MACHINE_TEST,
            MachineTestParams {
                destination: "vm".into(),
            },
        )
        .await;
    assert_eq!(result.platform, host_platform());
    let machines: Vec<Machine> = d.call(method::MACHINE_LIST, Value::Null).await;
    assert!(machines.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_daemon_from_an_older_install_is_replaced() {
    let (d, fake) = start_with_fake_ssh().await;
    let mut old = start_foreign_daemon(&fake, "machine");
    let m = add_machine(&d, "vm").await;
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
    assert!(old.wait().unwrap().success());
    assert!(!machine(&d, m.id).await.status.update_available);
}

#[tokio::test(flavor = "multi_thread")]
async fn node_daemon_on_the_machine_is_blocked() {
    let (d, fake) = start_with_fake_ssh().await;
    let mut node = start_foreign_daemon(&fake, "node");
    let m = add_machine(&d, "vm").await;
    assert!(eventually_state(&d, m.id, MachineState::Blocked).await);
    assert_eq!(
        machine(&d, m.id).await.status.error.as_deref(),
        Some("not an asterism machine daemon")
    );
    fake.stop_daemon();
    node.wait().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_command_permission_denied_is_offline_not_blocked() {
    let (d, fake) = start_with_fake_ssh().await;
    fake.fail_with("mkdir: /x: Permission denied", 1);
    let m = add_machine(&d, "vm").await;
    assert!(eventually_state(&d, m.id, MachineState::Offline).await);
    assert!(machine(&d, m.id)
        .await
        .status
        .error
        .unwrap()
        .contains("Permission denied"));
}

#[tokio::test(flavor = "multi_thread")]
async fn machine_role_daemon_refuses_to_manage_machines() {
    let (d, fake) = start_with_role(Role::Machine).await;
    let add = d
        .client
        .call::<_, Machine>(
            method::MACHINE_ADD,
            MachineAddParams {
                name: "vm".into(),
                destination: "vm".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(add, ClientError::Rpc(ref e) if e.kind() == ErrorKind::Forbidden));
    let test = d
        .client
        .call::<_, MachineTestResult>(
            method::MACHINE_TEST,
            MachineTestParams {
                destination: "vm".into(),
            },
        )
        .await
        .unwrap_err();
    assert!(matches!(test, ClientError::Rpc(ref e) if e.kind() == ErrorKind::Forbidden));
    let retry = d
        .client
        .call::<_, Value>(method::MACHINE_RETRY, MachineIdParams { machine_id: 1 })
        .await
        .unwrap_err();
    assert!(matches!(retry, ClientError::Rpc(ref e) if e.kind() == ErrorKind::Forbidden));
    assert_eq!(fake.invocations(), 0);
}

#[tokio::test(flavor = "multi_thread")]
async fn machine_with_active_tasks_cannot_be_removed() {
    let (d, _fake) = start_with_fake_ssh().await;
    let m = add_machine(&d, "vm").await;
    let repo = tempfile::tempdir().unwrap();
    common::init_repo(repo.path());
    let project: Project = d
        .call(
            method::PROJECT_ADD,
            ProjectAddParams {
                path: repo.path().display().to_string(),
                workspace_id: None,
            },
        )
        .await;
    let created: TaskCreateResult = d
        .call(
            method::TASK_CREATE,
            TaskCreateParams {
                project_id: project.id,
                title: "t".into(),
                ..Default::default()
            },
        )
        .await;
    let task_id = created.task.id;
    let paths = Paths {
        home: d._home.path().to_path_buf(),
    };
    asterism_core::store::Store::open(&paths.db())
        .unwrap()
        .set_task_remote(task_id, m.id, 1)
        .unwrap();

    let err = d
        .client
        .call::<_, Value>(method::MACHINE_REMOVE, MachineIdParams { machine_id: m.id })
        .await
        .unwrap_err();
    match err {
        ClientError::Rpc(e) => {
            assert_eq!(e.kind(), ErrorKind::InvalidParams);
            assert!(e.message.contains("tasks"), "{}", e.message);
        }
        other => panic!("{other:?}"),
    }

    let _: Task = d
        .call(
            method::TASK_ARCHIVE,
            TaskArchiveParams {
                task_id,
                force: false,
            },
        )
        .await;
    let _: Value = d
        .call(method::MACHINE_REMOVE, MachineIdParams { machine_id: m.id })
        .await;
}

/// A `file://` origin under an owner, as `project.ensure` needs.
fn bare_origin(dir: &Path) -> String {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    common::init_repo(&work);
    let bare = dir.join("remotes/acme/demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    common::run_git(
        dir,
        &[
            "clone",
            "-q",
            "--bare",
            &work.display().to_string(),
            &bare.display().to_string(),
        ],
    );
    format!("file://{}", bare.display())
}

/// A task and a running shell created over the machine daemon's own socket; returns their ids.
async fn remote_shell(fake: &FakeSsh, origin: &str) -> (Client, i64, i64) {
    let client = Client::connect_unix(&fake.machine_home().join("asterismd.sock"))
        .await
        .unwrap();
    hello(&client).await;
    let project: Project = client
        .call(
            method::PROJECT_ENSURE,
            ProjectEnsureParams { url: origin.into() },
        )
        .await
        .unwrap();
    let created: TaskCreateResult = client
        .call(
            method::TASK_CREATE,
            TaskCreateParams {
                project_id: project.id,
                title: "remote".into(),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let session: Session = client
        .call(
            method::SESSION_START,
            SessionStartParams {
                task_id: created.task.id,
                kind: SessionKind::Shell,
                prompt: None,
            },
        )
        .await
        .unwrap();
    (client, created.task.id, session.id)
}

async fn local_project(d: &D, repo: &Path) -> i64 {
    common::init_repo(repo);
    let project: Project = d
        .call(
            method::PROJECT_ADD,
            ProjectAddParams {
                path: repo.display().to_string(),
                workspace_id: None,
            },
        )
        .await;
    project.id
}

async fn hub_task(d: &D, project_id: i64, title: &str) -> i64 {
    let created: TaskCreateResult = d
        .call(
            method::TASK_CREATE,
            TaskCreateParams {
                project_id,
                title: title.into(),
                ..Default::default()
            },
        )
        .await;
    created.task.id
}

fn session_status(d: &D, id: i64) -> SessionStatus {
    hub_store(d).session(id).unwrap().unwrap().session.status
}

#[tokio::test(flavor = "multi_thread")]
async fn call_to_offline_machine_fails_fast() {
    let (daemon, d, fake) = hub(DEADLINE).await;
    fake.hang();
    let m = daemon.machines().add("vm", "vm").unwrap();
    let started = Instant::now();
    let err = daemon
        .machines()
        .call::<_, Vec<Session>>(m.id, method::SESSION_LIST, SessionListParams::default())
        .await
        .unwrap_err();
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(err.kind, ErrorKind::MachineOffline);
    assert_eq!(err.message, "machine vm is offline");
    hello(&d.client).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn hung_machine_call_times_out() {
    let (daemon, d, fake) = hub(Duration::from_millis(500)).await;
    let m = daemon.machines().add("vm", "vm").unwrap();
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
    fake.hang();
    fake.pause_link();
    let started = Instant::now();
    let result = daemon
        .machines()
        .call::<_, Vec<Session>>(m.id, method::SESSION_LIST, SessionListParams::default())
        .await;
    fake.resume_link();
    let err = result.unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout, "{err}");
    assert!(started.elapsed() < Duration::from_secs(2));
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_status_reaches_local_mirror_with_colliding_ids() {
    let (d, fake) = start_with_fake_ssh().await;
    let m = add_machine(&d, "vm").await;
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
    let dir = tempfile::tempdir().unwrap();
    let (remote, remote_task, remote_session) = remote_shell(&fake, &bare_origin(dir.path())).await;
    let repo = tempfile::tempdir().unwrap();
    let project = local_project(&d, repo.path()).await;
    let local_task = hub_task(&d, project, "local").await;
    let local: Session = d
        .call(
            method::SESSION_START,
            SessionStartParams {
                task_id: local_task,
                kind: SessionKind::Shell,
                prompt: None,
            },
        )
        .await;
    assert_eq!((local.id, remote_session), (1, 1));
    let task = hub_task(&d, project, "mirror").await;
    let store = hub_store(&d);
    store.set_task_remote(task, m.id, remote_task).unwrap();
    let mirror = store
        .insert_remote_session(
            task,
            remote_session,
            &SessionKind::Shell,
            SessionStatus::Working,
        )
        .unwrap();
    let mut events = d.events();

    let _: Value = remote
        .call(
            method::SESSION_KILL,
            SessionIdParams {
                session_id: remote_session,
            },
        )
        .await
        .unwrap();
    loop {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap();
        match event {
            Event::SessionStatusChanged { session_id, status } if session_id == mirror => {
                if status == SessionStatus::Exited {
                    break;
                }
            }
            Event::SessionStatusChanged { session_id, status } => {
                assert!(
                    session_id != local.id || status != SessionStatus::Exited,
                    "local session {session_id} got {status:?}"
                );
            }
            _ => {}
        }
    }
    assert_eq!(session_status(&d, mirror), SessionStatus::Exited);
    assert_ne!(session_status(&d, local.id), SessionStatus::Exited);
}

#[tokio::test(flavor = "multi_thread")]
async fn reconcile_marks_vanished_sessions_exited() {
    let (d, fake) = start_with_fake_ssh().await;
    let m = add_machine(&d, "vm").await;
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
    let dir = tempfile::tempdir().unwrap();
    let (_remote, remote_task, remote_session) =
        remote_shell(&fake, &bare_origin(dir.path())).await;
    let repo = tempfile::tempdir().unwrap();
    let project = local_project(&d, repo.path()).await;
    let task = hub_task(&d, project, "mirror").await;
    let store = hub_store(&d);
    store.set_task_remote(task, m.id, remote_task).unwrap();
    let mirror = store
        .insert_remote_session(
            task,
            remote_session,
            &SessionKind::Shell,
            SessionStatus::Working,
        )
        .unwrap();
    // No remote row at all: only reconcile can end it.
    let vanished = store
        .insert_remote_session(
            task,
            remote_session + 100,
            &SessionKind::Shell,
            SessionStatus::Idle,
        )
        .unwrap();

    fake.stop_daemon();
    assert!(
        common::eventually_within(DEADLINE, || {
            session_status(&d, mirror) == SessionStatus::Exited
                && session_status(&d, vanished) == SessionStatus::Exited
        })
        .await
    );
}

/// A daemon in the machine's home from an older install, with one shell that exits if `kill`.
async fn foreign_daemon_with_shell(fake: &FakeSsh, kill: bool) -> std::process::Child {
    let old = start_foreign_daemon(fake, "machine");
    let dir = tempfile::tempdir().unwrap();
    let (client, _, session) = remote_shell(fake, &bare_origin(dir.path())).await;
    if kill {
        let _: Value = client
            .call(
                method::SESSION_KILL,
                SessionIdParams {
                    session_id: session,
                },
            )
            .await
            .unwrap();
        let mut exited = false;
        for _ in 0..250 {
            let sessions: Vec<Session> = client
                .call(method::SESSION_LIST, SessionListParams::default())
                .await
                .unwrap();
            if sessions.iter().all(|s| s.status == SessionStatus::Exited) {
                exited = true;
                break;
            }
            tokio::time::sleep(Duration::from_millis(20)).await;
        }
        assert!(exited);
    }
    old
}

#[tokio::test(flavor = "multi_thread")]
async fn idle_upgrade_ignores_exited_sessions() {
    let (d, fake) = start_with_fake_ssh().await;
    let mut old = foreign_daemon_with_shell(&fake, true).await;
    let m = add_machine(&d, "vm").await;
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
    let exited = common::eventually_within(DEADLINE, || old.try_wait().unwrap().is_some()).await;
    assert!(exited, "the old daemon was not replaced");
    assert!(old.wait().unwrap().success());
    assert!(!machine(&d, m.id).await.status.update_available);
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_present_reports_update_available() {
    let (d, fake) = start_with_fake_ssh().await;
    let mut old = foreign_daemon_with_shell(&fake, false).await;
    let m = add_machine(&d, "vm").await;
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
    assert!(machine(&d, m.id).await.status.update_available);
    assert!(old.try_wait().unwrap().is_none());
    fake.stop_daemon();
    old.wait().unwrap();
}

#[tokio::test(flavor = "multi_thread")]
async fn attach_output_streams_remote_output() {
    let (daemon, d, fake) = hub(DEADLINE).await;
    let m = daemon.machines().add("vm", "vm").unwrap();
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
    let dir = tempfile::tempdir().unwrap();
    let (remote, _, session) = remote_shell(&fake, &bare_origin(dir.path())).await;
    let (_, mut output) = daemon
        .machines()
        .attach_output(m.id, session)
        .await
        .unwrap();
    let _: Value = remote
        .call(
            method::SESSION_SEND,
            SessionSendParams {
                session_id: session,
                text: "echo mirrored-$((40+2))".into(),
                submit: true,
            },
        )
        .await
        .unwrap();
    let mut seen = Vec::new();
    while !String::from_utf8_lossy(&seen).contains("mirrored-42") {
        let chunk = tokio::time::timeout(Duration::from_secs(10), output.recv())
            .await
            .unwrap()
            .unwrap();
        seen.extend(chunk);
    }
}

/// A ready machine and a hub project cloned from a fresh `file://` origin; returns the project id and origin URL.
async fn machine_and_project(d: &D, dir: &Path) -> (Machine, i64, String) {
    let m = add_machine(d, "vm").await;
    assert!(eventually_state(d, m.id, MachineState::Ready).await);
    let origin = bare_origin(dir);
    let repo = dir.join("hub/demo");
    std::fs::create_dir_all(repo.parent().unwrap()).unwrap();
    common::run_git(dir, &["clone", "-q", &origin, &repo.display().to_string()]);
    let project: Project = d
        .call(
            method::PROJECT_ADD,
            ProjectAddParams {
                path: repo.display().to_string(),
                workspace_id: None,
            },
        )
        .await;
    (m, project.id, origin)
}

async fn machine_client(fake: &FakeSsh) -> Client {
    let client = Client::connect_unix(&fake.machine_home().join("asterismd.sock"))
        .await
        .unwrap();
    hello(&client).await;
    client
}

async fn machine_tasks(fake: &FakeSsh) -> Vec<Task> {
    machine_client(fake)
        .await
        .call(method::TASK_LIST, TaskListParams::default())
        .await
        .unwrap()
}

async fn create_error(d: &D, params: TaskCreateParams) -> asterism_proto::rpc::RpcError {
    match d
        .client
        .call::<_, TaskCreateResult>(method::TASK_CREATE, params)
        .await
        .unwrap_err()
    {
        ClientError::Rpc(e) => e,
        other => panic!("{other:?}"),
    }
}

fn origin_branches(dir: &Path, origin: &str) -> String {
    common::run_git(dir, &["ls-remote", "--heads", origin])
}

#[tokio::test(flavor = "multi_thread")]
async fn task_created_on_machine_is_pushed() {
    let (d, fake) = start_with_fake_ssh().await;
    let dir = tempfile::tempdir().unwrap();
    let (m, project_id, origin) = machine_and_project(&d, dir.path()).await;

    let created: TaskCreateResult = d
        .call(
            method::TASK_CREATE,
            TaskCreateParams {
                project_id,
                title: "Remote work".into(),
                machine_id: Some(m.id),
                ..Default::default()
            },
        )
        .await;

    let task = created.task;
    assert_eq!(task.machine_id, Some(m.id));
    assert_eq!(task.branch, format!("asterism/{}-remote-work", task.id));
    assert!(
        origin_branches(dir.path(), &origin).contains(&format!("refs/heads/{}", task.branch)),
        "{}",
        origin_branches(dir.path(), &origin)
    );
    let remote = machine_tasks(&fake).await;
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].branch, task.branch);
    assert_eq!(task.worktree_path, remote[0].worktree_path);
    assert!(task
        .worktree_path
        .starts_with(&fake.machine_home().display().to_string()));
    assert_eq!(
        hub_store(&d).task_remote(task.id).unwrap(),
        Some((m.id, remote[0].id))
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn checkout_existing_branch_on_machine() {
    let (d, fake) = start_with_fake_ssh().await;
    let dir = tempfile::tempdir().unwrap();
    let (m, project_id, _origin) = machine_and_project(&d, dir.path()).await;
    let work = dir.path().join("work");
    common::run_git(&work, &["checkout", "-q", "-b", "feature/x"]);
    common::run_git(
        &work,
        &[
            "push",
            "-q",
            &dir.path()
                .join("remotes/acme/demo.git")
                .display()
                .to_string(),
            "feature/x",
        ],
    );

    let created: TaskCreateResult = d
        .call(
            method::TASK_CREATE,
            TaskCreateParams {
                project_id,
                title: "Checkout".into(),
                checkout: Some("feature/x".into()),
                machine_id: Some(m.id),
                ..Default::default()
            },
        )
        .await;

    assert_eq!(created.task.branch, "feature/x");
    let remote = machine_tasks(&fake).await;
    assert_eq!(remote.len(), 1);
    assert_eq!(remote[0].branch, "feature/x");
    let head = common::run_git(
        Path::new(&remote[0].worktree_path),
        &["rev-parse", "--abbrev-ref", "HEAD"],
    );
    assert_eq!(head.trim(), "feature/x");
}

#[tokio::test(flavor = "multi_thread")]
async fn project_without_origin_is_rejected() {
    let (d, fake) = start_with_fake_ssh().await;
    let dir = tempfile::tempdir().unwrap();
    let (m, project_id, _origin) = machine_and_project(&d, dir.path()).await;
    common::run_git(
        &dir.path().join("hub/demo"),
        &["remote", "remove", "origin"],
    );

    let e = create_error(
        &d,
        TaskCreateParams {
            project_id,
            title: "t".into(),
            machine_id: Some(m.id),
            ..Default::default()
        },
    )
    .await;

    assert_eq!(e.kind(), ErrorKind::InvalidParams);
    assert_eq!(e.message, "project has no origin remote");
    assert!(hub_store(&d).tasks(None, true).unwrap().is_empty());
    assert!(machine_tasks(&fake).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn unreachable_origin_leaves_no_task() {
    let (d, fake) = start_with_fake_ssh().await;
    let dir = tempfile::tempdir().unwrap();
    let (m, project_id, _origin) = machine_and_project(&d, dir.path()).await;
    let missing = format!("file://{}/remotes/acme/missing.git", dir.path().display());
    common::run_git(
        &dir.path().join("hub/demo"),
        &["remote", "set-url", "origin", &missing],
    );

    let e = create_error(
        &d,
        TaskCreateParams {
            project_id,
            title: "t".into(),
            machine_id: Some(m.id),
            ..Default::default()
        },
    )
    .await;

    assert_ne!(e.kind(), ErrorKind::MachineOffline);
    assert!(e.message.contains("missing.git"), "{}", e.message);
    assert!(hub_store(&d).tasks(None, true).unwrap().is_empty());
    assert!(machine_tasks(&fake).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_on_machine_is_rejected() {
    let (d, fake) = start_with_fake_ssh().await;
    let dir = tempfile::tempdir().unwrap();
    let (m, project_id, _origin) = machine_and_project(&d, dir.path()).await;

    let e = create_error(
        &d,
        TaskCreateParams {
            project_id,
            title: "t".into(),
            agent: Some("claude".into()),
            machine_id: Some(m.id),
            ..Default::default()
        },
    )
    .await;

    assert_eq!(e.kind(), ErrorKind::InvalidParams);
    assert_eq!(e.message, "agents on machines are not supported yet");
    assert!(hub_store(&d).tasks(None, true).unwrap().is_empty());
    assert!(machine_tasks(&fake).await.is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn slow_remote_create_within_clone_deadline_succeeds() {
    let (_daemon, d, fake) = hub(Duration::from_millis(300)).await;
    let dir = tempfile::tempdir().unwrap();
    let (m, project_id, _origin) = machine_and_project(&d, dir.path()).await;

    fake.pause_link();
    let resume = tokio::spawn(async move {
        tokio::time::sleep(Duration::from_secs(1)).await;
        fake.resume_link();
        fake
    });
    let created = d
        .client
        .call::<_, TaskCreateResult>(
            method::TASK_CREATE,
            TaskCreateParams {
                project_id,
                title: "slow".into(),
                machine_id: Some(m.id),
                ..Default::default()
            },
        )
        .await;
    let fake = resume.await.unwrap();

    let task = created.unwrap().task;
    assert_eq!(task.machine_id, Some(m.id));
    assert_eq!(machine_tasks(&fake).await.len(), 1);
}

/// A task created on the machine; the hub's copy of its worktree path is broken so any local git/file/PTY use fails.
async fn remote_task(d: &D, fake: &FakeSsh, dir: &Path) -> (Machine, Task, PathBuf) {
    let (m, project_id, _origin) = machine_and_project(d, dir).await;
    let created: TaskCreateResult = d
        .call(
            method::TASK_CREATE,
            TaskCreateParams {
                project_id,
                title: "Remote".into(),
                machine_id: Some(m.id),
                ..Default::default()
            },
        )
        .await;
    let task = created.task;
    let worktree = PathBuf::from(&machine_tasks(fake).await[0].worktree_path);
    hub_store(d)
        .set_task_location(task.id, &task.slug, &task.branch, "/nonexistent/hub-copy")
        .unwrap();
    (m, task, worktree)
}

async fn start_remote_shell(d: &D, task_id: i64) -> Session {
    d.call(
        method::SESSION_START,
        SessionStartParams {
            task_id,
            kind: SessionKind::Shell,
            prompt: None,
        },
    )
    .await
}

async fn read_text(d: &D, session_id: i64) -> String {
    let read: SessionReadResult = d
        .call(
            method::SESSION_READ,
            SessionReadParams {
                session_id,
                lines: 50,
            },
        )
        .await;
    read.text
}

fn rpc_error<T: std::fmt::Debug>(result: Result<T, ClientError>) -> asterism_proto::rpc::RpcError {
    match result.unwrap_err() {
        ClientError::Rpc(e) => e,
        other => panic!("{other:?}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_diff_and_files_are_forwarded() {
    let (d, fake) = start_with_fake_ssh().await;
    let dir = tempfile::tempdir().unwrap();
    let (_m, task, worktree) = remote_task(&d, &fake, dir.path()).await;
    std::fs::write(worktree.join("README.md"), "hello\nfrom-machine\n").unwrap();
    let id = task.id;

    let diff: TaskDiffResult = d
        .call(method::TASK_DIFF, TaskIdParams { task_id: id })
        .await;
    assert!(diff.patch.contains("+from-machine"), "{}", diff.patch);
    let stat: DiffStat = d
        .call(method::TASK_DIFF_STAT, TaskIdParams { task_id: id })
        .await;
    assert_eq!((stat.added, stat.removed), (1, 0));
    let entries: Vec<DirEntry> = d
        .call(
            method::TASK_LIST_DIR,
            TaskListDirParams {
                task_id: id,
                path: String::new(),
            },
        )
        .await;
    assert!(entries.iter().any(|e| e.name == "README.md" && e.changed));
    let file: TaskFileResult = d
        .call(
            method::TASK_FILE,
            TaskFileParams {
                task_id: id,
                path: "README.md".into(),
                known_mtime: None,
            },
        )
        .await;
    assert_eq!(file.content.as_deref(), Some("hello\nfrom-machine\n"));
    let review: ReviewResult = d
        .call(
            method::REVIEW_GET,
            ReviewGetParams {
                task_id: id,
                source: ReviewSource::Local,
            },
        )
        .await;
    assert!(review.patch.contains("+from-machine"), "{}", review.patch);
    let commits: ReviewCommitsResult = d
        .call(
            method::REVIEW_COMMITS,
            ReviewCommitsParams {
                task_id: id,
                source: ReviewSource::Local,
            },
        )
        .await;
    assert!(commits.uncommitted);
    let uncommitted: TaskDiffResult = d
        .call(
            method::REVIEW_COMMIT_DIFF,
            ReviewCommitDiffParams {
                task_id: id,
                source: ReviewSource::Local,
                sha: None,
            },
        )
        .await;
    assert!(uncommitted.patch.contains("+from-machine"));
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_shell_round_trip() {
    let (d, fake) = start_with_fake_ssh().await;
    let dir = tempfile::tempdir().unwrap();
    let (_m, task, _worktree) = remote_task(&d, &fake, dir.path()).await;
    let mut events = d.events();

    let session = start_remote_shell(&d, task.id).await;
    assert_eq!(session.task_id, task.id);
    let sessions: Vec<Session> = d
        .call(
            method::SESSION_LIST,
            SessionListParams {
                task_id: Some(task.id),
            },
        )
        .await;
    assert_eq!(
        sessions.iter().map(|s| s.id).collect::<Vec<_>>(),
        [session.id]
    );
    let remote_id = hub_store(&d).session_remote(session.id).unwrap().unwrap();
    assert_eq!(
        machine_client(&fake)
            .await
            .call::<_, Vec<Session>>(method::SESSION_LIST, SessionListParams::default())
            .await
            .unwrap()[0]
            .id,
        remote_id
    );

    let attached: SessionAttachResult = d
        .call(
            method::SESSION_ATTACH,
            SessionIdParams {
                session_id: session.id,
            },
        )
        .await;
    assert!(attached.rows > 0);
    let waited: SessionWaitResult = d
        .call(
            method::SESSION_WAIT,
            SessionWaitParams {
                session_id: session.id,
                until: SessionStatus::Idle,
                timeout_ms: Some(10_000),
            },
        )
        .await;
    assert_eq!(waited.status, SessionStatus::Idle);
    let _: Value = d
        .call(
            method::SESSION_SEND,
            SessionSendParams {
                session_id: session.id,
                text: "echo hi-from-$((1+1))-machine".into(),
                submit: true,
            },
        )
        .await;
    let mut seen = String::new();
    while !seen.contains("hi-from-2-machine") {
        let event = tokio::time::timeout(Duration::from_secs(10), events.recv())
            .await
            .unwrap()
            .unwrap();
        if let Event::SessionOutput { session_id, data } = event {
            assert_eq!(session_id, session.id);
            seen.push_str(&String::from_utf8_lossy(
                &base64::Engine::decode(&base64::engine::general_purpose::STANDARD, data).unwrap(),
            ));
        }
    }
    assert!(read_text(&d, session.id)
        .await
        .contains("hi-from-2-machine"));
    let _: Value = d
        .call(
            method::SESSION_RESIZE,
            SessionResizeParams {
                session_id: session.id,
                rows: 30,
                cols: 100,
            },
        )
        .await;
    let subagents: Vec<Subagent> = d
        .call(
            method::SESSION_SUBAGENTS,
            SessionSubagentsParams {
                session_id: session.id,
            },
        )
        .await;
    assert!(subagents.is_empty());
    let _: Value = d
        .call(
            method::SESSION_DETACH,
            SessionIdParams {
                session_id: session.id,
            },
        )
        .await;

    let _: Value = d
        .call(
            method::SESSION_KILL,
            SessionIdParams {
                session_id: session.id,
            },
        )
        .await;
    assert!(
        common::eventually_within(DEADLINE, || session_status(&d, session.id)
            == SessionStatus::Exited)
        .await
    );

    let _: Value = d
        .call(
            method::SESSION_REMOVE,
            SessionIdParams {
                session_id: session.id,
            },
        )
        .await;
    assert!(hub_store(&d).session(session.id).unwrap().is_none());
    assert!(machine_client(&fake)
        .await
        .call::<_, Vec<Session>>(method::SESSION_LIST, SessionListParams::default())
        .await
        .unwrap()
        .is_empty());
}

#[tokio::test(flavor = "multi_thread")]
async fn removed_on_machine_drops_the_mirror() {
    let (d, fake) = start_with_fake_ssh().await;
    let dir = tempfile::tempdir().unwrap();
    let (_m, task, _worktree) = remote_task(&d, &fake, dir.path()).await;
    let session = start_remote_shell(&d, task.id).await;
    let remote_id = hub_store(&d).session_remote(session.id).unwrap().unwrap();

    let _: Value = machine_client(&fake)
        .await
        .call(
            method::SESSION_REMOVE,
            SessionIdParams {
                session_id: remote_id,
            },
        )
        .await
        .unwrap();

    assert!(
        common::eventually_within(DEADLINE, || hub_store(&d)
            .session(session.id)
            .unwrap()
            .is_none())
        .await
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn remote_session_calls_fail_fast_when_offline() {
    let (d, fake) = start_with_fake_ssh().await;
    let dir = tempfile::tempdir().unwrap();
    let (m, task, _worktree) = remote_task(&d, &fake, dir.path()).await;
    let session = start_remote_shell(&d, task.id).await;

    fake.hang();
    fake.drop_link();
    let deadline = Instant::now() + DEADLINE;
    while machine(&d, m.id).await.status.state == MachineState::Ready {
        assert!(Instant::now() < deadline);
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let started = Instant::now();
    let e = rpc_error(
        d.client
            .call::<_, Value>(
                method::SESSION_SEND,
                SessionSendParams {
                    session_id: session.id,
                    text: "echo never-$((1+1))-sent".into(),
                    submit: true,
                },
            )
            .await,
    );
    assert!(started.elapsed() < Duration::from_secs(1));
    assert_eq!(e.kind(), ErrorKind::MachineOffline);

    fake.recover();
    assert!(eventually_state(&d, m.id, MachineState::Ready).await);
    tokio::time::sleep(Duration::from_millis(500)).await;
    // A loaded host can drop the fresh link on a health timeout; only the text matters here.
    let deadline = Instant::now() + DEADLINE;
    let text = loop {
        let read = d
            .client
            .call::<_, SessionReadResult>(
                method::SESSION_READ,
                SessionReadParams {
                    session_id: session.id,
                    lines: 50,
                },
            )
            .await;
        match read {
            Ok(read) => break read.text,
            Err(e) if Instant::now() < deadline => {
                eprintln!("retrying read: {e}");
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            Err(e) => panic!("{e}"),
        }
    };
    assert!(!text.contains("never-2-sent"), "{text}");
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_session_on_machine_is_rejected() {
    let (d, fake) = start_with_fake_ssh().await;
    let dir = tempfile::tempdir().unwrap();
    let (_m, task, _worktree) = remote_task(&d, &fake, dir.path()).await;

    let e = rpc_error(
        d.client
            .call::<_, Session>(
                method::SESSION_START,
                SessionStartParams {
                    task_id: task.id,
                    kind: SessionKind::Agent {
                        name: "claude".into(),
                    },
                    prompt: None,
                },
            )
            .await,
    );

    assert_eq!(e.kind(), ErrorKind::InvalidParams);
    assert!(hub_store(&d).sessions(Some(task.id)).unwrap().is_empty());
}
