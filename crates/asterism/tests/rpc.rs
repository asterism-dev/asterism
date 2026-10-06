mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use asterism_core::paths::Paths;
use asterism_proto::client::{Client, ClientError};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{method, *};
use asterism_proto::PROTO_VERSION;
use base64::Engine;
use common::init_repo;
use tempfile::TempDir;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

async fn start_daemon() -> (TempDir, PathBuf) {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().to_path_buf() };
    let socket = paths.socket();
    tokio::spawn(asterism_core::run(paths));
    for _ in 0..100 {
        if UnixStream::connect(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    (home, socket)
}

async fn client(socket: &Path) -> Client {
    let client = Client::connect_unix(socket).await.unwrap();
    let _: HelloResult = client
        .call(method::HELLO, HelloParams { proto_version: PROTO_VERSION, client_kind: ClientKind::Cli })
        .await
        .unwrap();
    client
}

async fn shell_session(client: &Client, repo: &Path) -> Session {
    let project: Project =
        client.call(method::PROJECT_ADD, ProjectAddParams { path: repo.display().to_string() }).await.unwrap();
    let created: TaskCreateResult = client
        .call(method::TASK_CREATE, TaskCreateParams { project_id: project.id, title: "rpc".into(), prompt: None, agent: None, base: None })
        .await
        .unwrap();
    let kind = SessionKind::Command { argv: vec!["sh".into(), "-c".into(), "echo ready; cat".into()] };
    client
        .call(method::SESSION_START, SessionStartParams { task_id: created.task.id, kind, prompt: None })
        .await
        .unwrap()
}

fn rpc_kind(err: ClientError) -> ErrorKind {
    match err {
        ClientError::Rpc(e) => e.kind(),
        other => panic!("expected an rpc error, got {other}"),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn hello_reports_node_and_rejects_other_protocol_versions() {
    let (_home, socket) = start_daemon().await;
    let client = Client::connect_unix(&socket).await.unwrap();
    let hello: HelloResult = client
        .call(method::HELLO, HelloParams { proto_version: PROTO_VERSION, client_kind: ClientKind::App })
        .await
        .unwrap();
    assert_eq!(hello.proto_version, PROTO_VERSION);
    assert!(hello.agents.iter().any(|a| a.name == "claude"));

    let err = client
        .call::<_, HelloResult>(method::HELLO, HelloParams { proto_version: 999, client_kind: ClientKind::App })
        .await
        .unwrap_err();
    assert_eq!(rpc_kind(err), ErrorKind::IncompatibleVersion);
}

#[tokio::test(flavor = "multi_thread")]
async fn attach_streams_output_after_the_snapshot() {
    let (_home, socket) = start_daemon().await;
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let client = client(&socket).await;
    let mut events = client.take_events().unwrap();
    let session = shell_session(&client, repo.path()).await;

    tokio::time::sleep(Duration::from_millis(300)).await;
    let attached: SessionAttachResult =
        client.call(method::SESSION_ATTACH, SessionIdParams { session_id: session.id }).await.unwrap();
    let snapshot = base64::engine::general_purpose::STANDARD.decode(attached.snapshot).unwrap();
    assert!(String::from_utf8_lossy(&snapshot).contains("ready"));

    let _: () = client
        .call(method::SESSION_SEND, SessionSendParams { session_id: session.id, text: "ping\n".into(), submit: false })
        .await
        .unwrap();
    let mut seen = String::new();
    while !seen.contains("ping") {
        let event = tokio::time::timeout(Duration::from_secs(5), events.recv()).await.unwrap().unwrap();
        if let Event::SessionOutput { session_id, data } = event {
            assert_eq!(session_id, session.id);
            seen.push_str(&String::from_utf8_lossy(&base64::engine::general_purpose::STANDARD.decode(data).unwrap()));
        }
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn subscribe_delivers_change_events() {
    let (_home, socket) = start_daemon().await;
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let client = client(&socket).await;
    let mut events = client.take_events().unwrap();
    let _: () = client.call(method::SUBSCRIBE, ()).await.unwrap();
    let _: Project =
        client.call(method::PROJECT_ADD, ProjectAddParams { path: repo.path().display().to_string() }).await.unwrap();
    let event = tokio::time::timeout(Duration::from_secs(5), events.recv()).await.unwrap().unwrap();
    assert!(matches!(event, Event::ProjectChanged(_)), "{event:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn bad_requests_get_structured_errors() {
    let (_home, socket) = start_daemon().await;
    let client = client(&socket).await;
    assert_eq!(rpc_kind(client.call::<_, ()>("nope", ()).await.unwrap_err()), ErrorKind::MethodNotFound);
    assert_eq!(
        rpc_kind(client.call::<_, ()>(method::SESSION_KILL, serde_json::json!({"wrong": 1})).await.unwrap_err()),
        ErrorKind::InvalidParams
    );

    let mut raw = UnixStream::connect(&socket).await.unwrap();
    raw.write_all(b"this is not json\n").await.unwrap();
    let mut line = String::new();
    BufReader::new(raw).read_line(&mut line).await.unwrap();
    assert!(line.contains("parse_error"), "{line}");
}

#[tokio::test(flavor = "multi_thread")]
async fn disconnecting_mid_attach_keeps_daemon_and_session_alive() {
    let (_home, socket) = start_daemon().await;
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let first = client(&socket).await;
    let session = shell_session(&first, repo.path()).await;
    let _: SessionAttachResult =
        first.call(method::SESSION_ATTACH, SessionIdParams { session_id: session.id }).await.unwrap();
    drop(first);

    let second = client(&socket).await;
    let _: () = second
        .call(method::SESSION_SEND, SessionSendParams { session_id: session.id, text: "still here\n".into(), submit: false })
        .await
        .unwrap();
    for _ in 0..100 {
        let read: SessionReadResult =
            second.call(method::SESSION_READ, SessionReadParams { session_id: session.id, lines: 10 }).await.unwrap();
        if read.text.contains("still here") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("session stopped responding after the first client disconnected");
}

#[tokio::test(flavor = "multi_thread")]
async fn home_is_private_and_second_daemon_refuses() {
    let parent = tempfile::tempdir().unwrap();
    let home = parent.path().join("home");
    std::fs::create_dir(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755)).unwrap();
    let paths = Paths { home: home.clone() };
    let socket = paths.socket();
    tokio::spawn(asterism_core::run(paths));
    for _ in 0..100 {
        if UnixStream::connect(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let mode = std::fs::metadata(&home).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o700);

    let err = asterism_core::run(Paths { home: home.clone() }).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
    client(&socket).await;
}

#[tokio::test(flavor = "multi_thread")]
async fn stalled_attached_client_does_not_block_others() {
    let (_home, socket) = start_daemon().await;
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let good = client(&socket).await;
    let project: Project =
        good.call(method::PROJECT_ADD, ProjectAddParams { path: repo.path().display().to_string() }).await.unwrap();
    let created: TaskCreateResult = good
        .call(method::TASK_CREATE, TaskCreateParams { project_id: project.id, title: "yes".into(), prompt: None, agent: None, base: None })
        .await
        .unwrap();
    let kind = SessionKind::Command { argv: vec!["yes".into()] };
    let session: Session = good
        .call(method::SESSION_START, SessionStartParams { task_id: created.task.id, kind, prompt: None })
        .await
        .unwrap();

    let mut raw = UnixStream::connect(&socket).await.unwrap();
    let attach = serde_json::json!({"jsonrpc":"2.0","id":1,"method":method::SESSION_ATTACH,"params":{"session_id":session.id}});
    raw.write_all(format!("{attach}\n").as_bytes()).await.unwrap();
    tokio::time::sleep(Duration::from_secs(1)).await;

    let sessions: Vec<Session> = tokio::time::timeout(
        Duration::from_secs(2),
        good.call(method::SESSION_LIST, SessionListParams { task_id: None }),
    )
    .await
    .unwrap()
    .unwrap();
    assert_eq!(sessions.len(), 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn stale_socket_file_is_replaced() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().to_path_buf() };
    std::fs::write(paths.socket(), "").unwrap();
    let socket = paths.socket();
    tokio::spawn(asterism_core::run(paths));
    let mut connected = false;
    for _ in 0..100 {
        if UnixStream::connect(&socket).await.is_ok() {
            connected = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(connected);
}

#[tokio::test(flavor = "multi_thread")]
async fn shutdown_stops_the_server() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().to_path_buf() };
    let socket = paths.socket();
    let server = tokio::spawn(asterism_core::run(paths));
    for _ in 0..100 {
        if UnixStream::connect(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let client = client(&socket).await;
    let _: () = client.call(method::SHUTDOWN, ()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), server).await.unwrap().unwrap().unwrap();
    assert!(!socket.exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn concurrent_daemons_start_exactly_once() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().to_path_buf() };
    let socket = paths.socket();
    let mut a = tokio::spawn(asterism_core::run(paths.clone()));
    let mut b = tokio::spawn(asterism_core::run(paths));
    let (loser, winner) = tokio::time::timeout(Duration::from_secs(5), async {
        tokio::select! {
            res = &mut a => (res, b),
            res = &mut b => (res, a),
        }
    })
    .await
    .unwrap();
    assert_eq!(loser.unwrap().unwrap_err().kind(), std::io::ErrorKind::AddrInUse);
    for _ in 0..100 {
        if UnixStream::connect(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!winner.is_finished());
    let _: () = client(&socket).await.call(method::SHUTDOWN, ()).await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), winner).await.unwrap().unwrap().unwrap();
}

#[tokio::test]
async fn held_daemon_lock_refuses_to_start() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().to_path_buf() };
    paths.ensure_dirs().unwrap();
    let held = std::fs::File::create(paths.lock()).unwrap();
    held.try_lock().unwrap();
    let err = asterism_core::run(paths.clone()).await.unwrap_err();
    assert_eq!(err.kind(), std::io::ErrorKind::AddrInUse);
    assert!(!paths.socket().exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn agent_config_and_session_remove_are_routed() {
    let (_home, socket) = start_daemon().await;
    let client = client(&socket).await;
    let config: AgentConfig =
        client.call(method::AGENT_CONFIG_GET, AgentParams { agent: "claude".into() }).await.unwrap();
    assert_eq!(config, AgentConfig::default());
    let err = client
        .call::<_, ()>(
            method::AGENT_CONFIG_SET,
            AgentConfigSetParams {
                agent: "claude".into(),
                config: AgentConfig { mcp: Some(serde_json::json!([])), ..Default::default() },
            },
        )
        .await
        .unwrap_err();
    assert_eq!(rpc_kind(err), ErrorKind::InvalidParams);
    let raw: AgentConfigRaw =
        client.call(method::AGENT_CONFIG_GET_RAW, AgentParams { agent: "claude".into() }).await.unwrap();
    assert_eq!(raw, AgentConfigRaw::default());
    let err = client.call::<_, ()>(method::SESSION_REMOVE, SessionIdParams { session_id: 42 }).await.unwrap_err();
    assert_eq!(rpc_kind(err), ErrorKind::NotFound);
}

#[tokio::test(flavor = "multi_thread")]
async fn project_and_path_methods_are_routed() {
    let (_home, socket) = start_daemon().await;
    let client = client(&socket).await;
    let info: NodeConfigInfo = client.call(method::NODE_CONFIG_GET, serde_json::Value::Null).await.unwrap();
    assert_eq!(info.config.paths, info.defaults);
    let err = client
        .call::<_, ()>(method::NODE_CONFIG_SET, NodeConfigSetParams { config: NodeConfig { paths: PathSettings { repos: "rel".into(), worktrees: "rel".into() } } })
        .await
        .unwrap_err();
    assert_eq!(rpc_kind(err), ErrorKind::InvalidParams);
    let err = client.call::<_, Project>(method::PROJECT_CLONE, ProjectCloneParams { source: "nope".into() }).await.unwrap_err();
    assert_eq!(rpc_kind(err), ErrorKind::InvalidParams);
    let err = client.call::<_, Vec<GithubRepo>>(method::GITHUB_REPOS, GithubOwnerParams { owner: "-x".into() }).await.unwrap_err();
    assert_eq!(rpc_kind(err), ErrorKind::InvalidParams);
    let err = client.call::<_, ProjectCreateResult>(method::PROJECT_CREATE, ProjectCreateParams { name: "../x".into(), github: None }).await.unwrap_err();
    assert_eq!(rpc_kind(err), ErrorKind::InvalidParams);
}

#[tokio::test]
async fn project_branches_and_update_over_rpc() {
    let (_home, socket) = start_daemon().await;
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let client = client(&socket).await;
    let project: Project =
        client.call(method::PROJECT_ADD, ProjectAddParams { path: repo.path().display().to_string() }).await.unwrap();
    let branches: ProjectBranches =
        client.call(method::PROJECT_BRANCHES, ProjectIdParams { project_id: project.id }).await.unwrap();
    assert_eq!(branches.default.as_deref(), Some("main"));
    let updated: Project = client
        .call(method::PROJECT_UPDATE, ProjectUpdateParams { project_id: project.id, default_base: Some("main".into()) })
        .await
        .unwrap();
    assert_eq!(updated.default_base.as_deref(), Some("main"));
}
