use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use asterism_core::plugins::process::{Backend, BackendConfig, HostFn};
use asterism_proto::rpc::{ErrorKind, RpcError};
use serde_json::{json, Map, Value};

fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins/echo")
}

fn host() -> HostFn {
    Arc::new(|plugin: String, method: String, _params: Value| {
        Box::pin(async move {
            match method.as_str() {
                "whoami" => Ok(json!(plugin)),
                "project.list" => Ok(json!([{"id": 7}])),
                other => Err(RpcError::new(
                    ErrorKind::MethodNotFound,
                    format!("unknown method {other}"),
                )),
            }
        })
    })
}

struct Fixture {
    dir: tempfile::TempDir,
    backend: Arc<Backend>,
}

impl Fixture {
    fn new(mode: &str, caps: &str, timeout: Duration, idle: Duration) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let env = vec![
            ("PATH".to_string(), std::env::var("PATH").unwrap()),
            ("FIXTURE_MODE".to_string(), mode.to_string()),
            ("FIXTURE_CAPS".to_string(), caps.to_string()),
            ("FIXTURE_LOG".to_string(), log.display().to_string()),
        ];
        let backend_py = fixture_dir().join("backend.py").display().to_string();
        // Logs the spawn before Python boots, so a fast kill cannot hide it.
        let argv = [
            "/bin/sh",
            "-c",
            "echo spawn $$ >> \"$FIXTURE_LOG\"; exec \"$0\"",
            backend_py.as_str(),
        ]
        .map(String::from)
        .to_vec();
        let config = BackendConfig {
            plugin: "echo".into(),
            argv,
            dir: fixture_dir(),
            data_dir: dir.path().join("data"),
            env,
            capabilities: ["command", "forge"].map(String::from).into(),
            call_timeout: timeout,
            idle,
        };
        let backend = Backend::new(
            config,
            host(),
            Map::from_iter([("region".to_string(), json!("eu"))]),
        );
        Self { dir, backend }
    }

    fn standard() -> Self {
        Self::new(
            "",
            "command,forge",
            Duration::from_secs(5),
            Duration::from_secs(60),
        )
    }

    fn log(&self) -> String {
        std::fs::read_to_string(self.dir.path().join("log")).unwrap_or_default()
    }

    fn starts(&self) -> usize {
        self.log()
            .lines()
            .filter(|l| l.starts_with("start "))
            .count()
    }

    fn spawns(&self) -> usize {
        self.log()
            .lines()
            .filter(|l| l.starts_with("spawn "))
            .count()
    }

    async fn call(&self, method: &str, params: Value) -> asterism_core::error::Result<Value> {
        self.backend
            .call(method, params, Some(Duration::from_secs(5)))
            .await
    }
}

#[tokio::test]
async fn handshake_passes_settings_and_calls_work() {
    let f = Fixture::standard();
    let status = f.call("forge.status", json!({})).await.unwrap();
    assert_eq!(status["account"], "me");
    assert!(f.log().contains(r#"init {"region": "eu"}"#), "{}", f.log());
}

#[tokio::test]
async fn capability_mismatch_marks_the_backend_failing() {
    let f = Fixture::new("", "forge", Duration::from_secs(5), Duration::from_secs(60));
    let err = f.call("forge.status", json!({})).await.unwrap_err();
    assert_eq!(err.kind, ErrorKind::PluginError);
    assert!(f.backend.failing().unwrap().contains("capabilities"));
    assert!(f.call("forge.status", json!({})).await.is_err());
    assert_eq!(f.starts(), 1);
}

#[tokio::test]
async fn slow_calls_time_out_without_killing_the_backend() {
    let f = Fixture::new(
        "",
        "command,forge",
        Duration::from_secs(5),
        Duration::from_secs(60),
    );
    let err = f
        .backend
        .call(
            "echo.sleep",
            json!({"ms": 2000}),
            Some(Duration::from_millis(200)),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout);
    assert!(f
        .backend
        .call("forge.status", json!({}), Some(Duration::from_secs(5)))
        .await
        .is_ok());
    assert_eq!(f.starts(), 1);
}

#[tokio::test]
async fn crashes_restart_until_the_backend_is_failing() {
    let f = Fixture::standard();
    assert_eq!(
        f.call("echo.crash", json!({})).await.unwrap_err().kind,
        ErrorKind::PluginError
    );
    assert!(f.call("forge.status", json!({})).await.is_ok());
    assert_eq!(f.starts(), 2);
    f.call("echo.crash", json!({})).await.unwrap_err();
    f.call("echo.crash", json!({})).await.unwrap_err();
    // The reader task records the crash right after failing the call.
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert!(f.backend.failing().unwrap().contains("crashed 3 times"));
    assert!(f.call("forge.status", json!({})).await.is_err());
    assert_eq!(f.starts(), 3);
}

#[tokio::test]
async fn idle_backends_stop_and_restart_on_demand() {
    let f = Fixture::new(
        "",
        "command,forge",
        Duration::from_secs(5),
        Duration::from_millis(300),
    );
    f.call("forge.status", json!({})).await.unwrap();
    tokio::time::sleep(Duration::from_millis(900)).await;
    assert!(!f.backend.is_running().await);
    f.call("forge.status", json!({})).await.unwrap();
    assert_eq!(f.starts(), 2);
}

#[tokio::test]
async fn host_requests_reach_the_host_fn() {
    let f = Fixture::standard();
    assert_eq!(
        f.call("echo.host", json!({"method": "project.list"}))
            .await
            .unwrap(),
        json!([{"id": 7}])
    );
    assert_eq!(
        f.call("echo.host", json!({"method": "whoami"}))
            .await
            .unwrap(),
        json!("echo")
    );
    let err = f
        .call("echo.host", json!({"method": "nope"}))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::PluginError);
    assert!(
        err.message.starts_with("echo: unknown method nope"),
        "{}",
        err.message
    );
}

#[tokio::test]
async fn garbage_output_is_ignored() {
    let f = Fixture::standard();
    assert_eq!(
        f.call("echo.garbage", json!({})).await.unwrap(),
        json!("after garbage")
    );
}

#[tokio::test]
async fn concurrent_first_calls_start_one_process() {
    let f = Fixture::standard();
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..5 {
        let backend = f.backend.clone();
        set.spawn(async move {
            backend
                .call("forge.status", json!({}), Some(Duration::from_secs(5)))
                .await
        });
    }
    while let Some(result) = set.join_next().await {
        result.unwrap().unwrap();
    }
    assert_eq!(f.starts(), 1);
}

#[tokio::test]
async fn hanging_initialize_times_out_and_retries() {
    let f = Fixture::new(
        "hang-init",
        "command,forge",
        Duration::from_millis(1000),
        Duration::from_secs(60),
    );
    for _ in 0..2 {
        let started = Instant::now();
        assert_eq!(
            f.call("forge.status", json!({})).await.unwrap_err().kind,
            ErrorKind::Timeout
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }
    assert_eq!(f.spawns(), 2);
}

#[tokio::test]
async fn settings_updates_reach_the_backend_or_restart_it() {
    let f = Fixture::standard();
    f.call("forge.status", json!({})).await.unwrap();
    f.backend
        .update_settings(Map::from_iter([("token".to_string(), json!("t"))]))
        .await;
    assert!(
        f.log().contains(r#"settings {"token": "t"}"#),
        "{}",
        f.log()
    );

    let quiet = Fixture::new(
        "no-settings",
        "command,forge",
        Duration::from_secs(5),
        Duration::from_secs(60),
    );
    quiet.call("forge.status", json!({})).await.unwrap();
    quiet.backend.update_settings(Map::new()).await;
    assert!(!quiet.backend.is_running().await);
}

#[tokio::test]
async fn cancelling_an_untimed_call_kills_the_backend() {
    let f = Fixture::standard();
    f.call("forge.status", json!({})).await.unwrap();
    let backend = f.backend.clone();
    let call =
        tokio::spawn(async move { backend.call("echo.sleep", json!({"ms": 10000}), None).await });
    tokio::time::sleep(Duration::from_millis(300)).await;
    call.abort();
    let mut stopped = false;
    for _ in 0..50 {
        if !f.backend.is_running().await {
            stopped = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(stopped);
}

async fn wait_for(mut condition: impl FnMut() -> bool) {
    for _ in 0..100 {
        if condition() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    panic!("condition not met in time");
}

#[tokio::test]
async fn crashing_during_initialize_counts_once_per_attempt() {
    let f = Fixture::new(
        "crash-init",
        "command,forge",
        Duration::from_secs(5),
        Duration::from_secs(60),
    );
    for _ in 0..3 {
        f.call("forge.status", json!({})).await.unwrap_err();
    }
    wait_for(|| f.backend.failing().is_some()).await;
    assert_eq!(f.starts(), 3);
    f.call("forge.status", json!({})).await.unwrap_err();
    assert_eq!(f.starts(), 3);
}

#[tokio::test]
async fn concurrent_capability_mismatch_starts_one_process() {
    let f = Fixture::new("", "forge", Duration::from_secs(5), Duration::from_secs(60));
    let mut set = tokio::task::JoinSet::new();
    for _ in 0..5 {
        let backend = f.backend.clone();
        set.spawn(async move {
            backend
                .call("forge.status", json!({}), Some(Duration::from_secs(5)))
                .await
        });
    }
    while let Some(result) = set.join_next().await {
        result.unwrap().unwrap_err();
    }
    assert_eq!(f.starts(), 1);
}

#[tokio::test]
async fn cancelling_during_the_handshake_does_not_leave_a_process_behind() {
    let f = Fixture::new(
        "hang-init",
        "command,forge",
        Duration::from_secs(10),
        Duration::from_secs(60),
    );
    let backend = f.backend.clone();
    let call = tokio::spawn(async move {
        backend
            .call("forge.status", json!({}), Some(Duration::from_secs(10)))
            .await
    });
    wait_for(|| f.starts() == 1).await;
    call.abort();
    let backend = f.backend.clone();
    let second = tokio::spawn(async move {
        backend
            .call("forge.status", json!({}), Some(Duration::from_secs(10)))
            .await
    });
    wait_for(|| f.starts() == 2).await;
    second.abort();
    assert!(!f.backend.is_running().await);
}
