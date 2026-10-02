mod common;

use std::sync::Arc;
use std::time::Duration;

use asterism_node::{LocalNode, LocalNodeConfig, NodeStatus, PathEnv};
use asterism_proto::paths::Paths;
use asterism_proto::types::{method, Event, Project, Session, SessionAttachResult, TaskCreateResult};
use base64::Engine;
use common::{daemon_bin, init_repo, shared, stop_daemon, Recorder};
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::task::JoinHandle;

const VERSION: &str = env!("CARGO_PKG_VERSION");

struct TestNode {
    node: Arc<LocalNode>,
    sink: Arc<Recorder>,
    paths: Paths,
    run: JoinHandle<()>,
    stop_daemon_on_drop: bool,
}

impl TestNode {
    fn start(home: &TempDir, bundled_version: &str, path_env: PathEnv) -> Self {
        let paths = Paths { home: home.path().join("h") };
        let sink = Arc::new(Recorder::default());
        let config = LocalNodeConfig {
            paths: paths.clone(),
            daemon_bin: daemon_bin(),
            path_env,
            bundled_version: bundled_version.into(),
        };
        let node = LocalNode::new(config, sink.clone());
        let run = tokio::spawn(node.clone().run());
        Self { node, sink, paths, run, stop_daemon_on_drop: true }
    }

    async fn connected_pid(&self) -> u32 {
        match self.sink.wait_status(|s| matches!(s, NodeStatus::Connected { .. })).await {
            NodeStatus::Connected { hello } => hello.pid,
            _ => unreachable!(),
        }
    }

    /// Stops the connection loop but leaves the daemon running for the next node.
    fn abandon(mut self) {
        self.stop_daemon_on_drop = false;
    }
}

impl Drop for TestNode {
    fn drop(&mut self) {
        self.run.abort();
        if self.stop_daemon_on_drop {
            stop_daemon(&self.paths);
        }
    }
}

async fn shell_session(node: &LocalNode, script: &str) -> Session {
    let repo = tempfile::tempdir().unwrap().keep();
    init_repo(&repo);
    let project: Project = serde_json::from_value(
        node.call(method::PROJECT_ADD, json!({"path": repo.display().to_string()})).await.unwrap(),
    )
    .unwrap();
    let created: TaskCreateResult = serde_json::from_value(
        node.call(method::TASK_CREATE, json!({"project_id": project.id, "title": "node test"})).await.unwrap(),
    )
    .unwrap();
    let kind = json!({"type": "command", "argv": ["sh", "-c", script]});
    serde_json::from_value(
        node.call(method::SESSION_START, json!({"task_id": created.task.id, "kind": kind})).await.unwrap(),
    )
    .unwrap()
}

async fn read_screen(node: &LocalNode, session_id: i64, needle: &str) -> String {
    for _ in 0..100 {
        let read = node.call(method::SESSION_READ, json!({"session_id": session_id, "lines": 20})).await.unwrap();
        let text = read["text"].as_str().unwrap_or_default().to_string();
        if text.contains(needle) {
            return text;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("{needle:?} never appeared on session {session_id}");
}

fn decode(data: &str) -> String {
    String::from_utf8_lossy(&base64::engine::general_purpose::STANDARD.decode(data).unwrap()).into_owned()
}

#[tokio::test(flavor = "multi_thread")]
async fn autostarts_the_bundled_daemon_and_connects() {
    let home = tempfile::tempdir().unwrap();
    let test = TestNode::start(&home, VERSION, PathEnv::Inherit);
    test.connected_pid().await;
    assert!(matches!(test.node.status(), NodeStatus::Connected { .. }));
    assert!(test.paths.socket().exists());
}

#[tokio::test(flavor = "multi_thread")]
async fn calls_events_and_attached_output_flow_through() {
    let home = tempfile::tempdir().unwrap();
    let test = TestNode::start(&home, VERSION, PathEnv::Inherit);
    test.connected_pid().await;
    let session = shell_session(&test.node, "echo ready; cat").await;
    read_screen(&test.node, session.id, "ready").await;

    let output = shared(String::new());
    let sink = output.clone();
    let attached: SessionAttachResult =
        test.node.attach(session.id, Box::new(move |data| sink.lock().unwrap().push_str(&decode(&data)))).await.unwrap();
    assert!(decode(&attached.snapshot).contains("ready"));

    test.node.call(method::SESSION_SEND, json!({"session_id": session.id, "text": "ping\n"})).await.unwrap();
    for _ in 0..100 {
        if output.lock().unwrap().contains("ping") {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(output.lock().unwrap().contains("ping"));
    assert!(test.sink.events.lock().unwrap().iter().any(|e| matches!(e, Event::TaskChanged(_))));

    test.node.detach(session.id).await.unwrap();
    let err = test.node.call("no.such.method", Value::Null).await.unwrap_err();
    assert_eq!(err.kind, "method_not_found");
}

#[tokio::test(flavor = "multi_thread")]
async fn sessions_get_the_login_path() {
    let home = tempfile::tempdir().unwrap();
    let path = format!("/asterism-test-bin:{}", std::env::var("PATH").unwrap());
    let test = TestNode::start(&home, VERSION, PathEnv::Fixed(path));
    test.connected_pid().await;
    let session = shell_session(&test.node, "echo \"path=$PATH\"; sleep 30").await;
    read_screen(&test.node, session.id, "/asterism-test-bin").await;
}

#[tokio::test(flavor = "multi_thread")]
async fn reconnects_after_the_daemon_stops() {
    let home = tempfile::tempdir().unwrap();
    let test = TestNode::start(&home, VERSION, PathEnv::Inherit);
    let first = test.connected_pid().await;
    test.sink.clear();
    test.node.call(method::SHUTDOWN, Value::Null).await.unwrap();
    let second = test.connected_pid().await;
    assert_ne!(first, second);
}

#[tokio::test(flavor = "multi_thread")]
async fn version_mismatch_restarts_an_idle_daemon_once() {
    let home = tempfile::tempdir().unwrap();
    let first = TestNode::start(&home, VERSION, PathEnv::Inherit);
    let old_pid = first.connected_pid().await;
    first.abandon();

    let test = TestNode::start(&home, "999.0.0", PathEnv::Inherit);
    match test.sink.wait_status(|s| matches!(s, NodeStatus::UpdateAvailable { .. })).await {
        NodeStatus::UpdateAvailable { hello, bundled_version } => {
            assert_ne!(hello.pid, old_pid, "idle daemon should have been replaced");
            assert_eq!(bundled_version, "999.0.0");
        }
        _ => unreachable!(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn running_sessions_block_the_silent_update() {
    let home = tempfile::tempdir().unwrap();
    let first = TestNode::start(&home, VERSION, PathEnv::Inherit);
    let old_pid = first.connected_pid().await;
    shell_session(&first.node, "sleep 30").await;
    first.abandon();

    let test = TestNode::start(&home, "999.0.0", PathEnv::Inherit);
    match test.sink.wait_status(|s| matches!(s, NodeStatus::UpdateAvailable { .. })).await {
        NodeStatus::UpdateAvailable { hello, .. } => assert_eq!(hello.pid, old_pid),
        _ => unreachable!(),
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn incompatible_daemon_is_reported_and_replaced_on_restart() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().join("h") };
    paths.ensure_dirs().unwrap();
    let listener = tokio::net::UnixListener::bind(paths.socket()).unwrap();
    let socket = paths.socket();
    let fake = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let request: Value = serde_json::from_str(&line).unwrap();
                let id = request["id"].clone();
                if request["method"] == "shutdown" {
                    writer.write_all(format!("{}\n", json!({"jsonrpc": "2.0", "id": id, "result": null})).as_bytes()).await.unwrap();
                    std::fs::remove_file(&socket).unwrap();
                    return;
                }
                let error = json!({"code": -32006, "message": "daemon speaks protocol 0", "data": {"kind": "incompatible_version"}});
                writer.write_all(format!("{}\n", json!({"jsonrpc": "2.0", "id": id, "error": error})).as_bytes()).await.unwrap();
            }
        }
    });

    let test = TestNode::start(&home, VERSION, PathEnv::Inherit);
    match test.sink.wait_status(|s| matches!(s, NodeStatus::Incompatible { .. })).await {
        NodeStatus::Incompatible { message } => assert!(message.contains("protocol 0")),
        _ => unreachable!(),
    }
    test.node.restart_daemon().await.unwrap();
    tokio::time::timeout(Duration::from_secs(5), fake).await.unwrap().unwrap();
    test.connected_pid().await;
}

struct CallOnConnected {
    node: std::sync::OnceLock<Arc<LocalNode>>,
    result: std::sync::Mutex<Option<Result<Value, asterism_node::CallError>>>,
}

impl asterism_node::NodeSink for CallOnConnected {
    fn status(&self, status: &NodeStatus) {
        let (Some(node), NodeStatus::Connected { .. }) = (self.node.get().cloned(), status) else { return };
        let handle = tokio::runtime::Handle::current();
        // A separate thread so the call can run while the connection loop is inside this callback.
        let result = std::thread::spawn(move || handle.block_on(node.call(method::PROJECT_LIST, Value::Null)))
            .join()
            .unwrap();
        *self.result.lock().unwrap() = Some(result);
    }
    fn event(&self, _: &Event) {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn connected_status_is_published_only_once_calls_work() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().join("h") };
    let sink = Arc::new(CallOnConnected { node: Default::default(), result: Default::default() });
    let config = LocalNodeConfig { paths: paths.clone(), daemon_bin: daemon_bin(), path_env: PathEnv::Inherit, bundled_version: VERSION.into() };
    let node = LocalNode::new(config, sink.clone());
    let _ = sink.node.set(node.clone());
    let run = tokio::spawn(node.clone().run());
    for _ in 0..300 {
        if sink.result.lock().unwrap().is_some() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    run.abort();
    stop_daemon(&paths);
    let result = sink.result.lock().unwrap().take();
    assert!(matches!(result, Some(Ok(_))), "call from the Connected callback failed: {result:?}");
}

#[tokio::test(flavor = "multi_thread")]
async fn incompatible_wait_wakes_when_the_daemon_is_already_gone() {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().join("h") };
    paths.ensure_dirs().unwrap();
    let listener = tokio::net::UnixListener::bind(paths.socket()).unwrap();
    let fake = tokio::spawn(async move {
        loop {
            let (stream, _) = listener.accept().await.unwrap();
            let (reader, mut writer) = stream.into_split();
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                let request: Value = serde_json::from_str(&line).unwrap();
                let error = json!({"code": -32006, "message": "daemon speaks protocol 0", "data": {"kind": "incompatible_version"}});
                writer.write_all(format!("{}\n", json!({"jsonrpc": "2.0", "id": request["id"], "error": error})).as_bytes()).await.unwrap();
            }
        }
    });

    let test = TestNode::start(&home, VERSION, PathEnv::Inherit);
    test.sink.wait_status(|s| matches!(s, NodeStatus::Incompatible { .. })).await;
    fake.abort();
    let _ = fake.await;
    std::fs::remove_file(paths.socket()).unwrap();

    test.node.restart_daemon().await.unwrap();
    test.connected_pid().await;
}
