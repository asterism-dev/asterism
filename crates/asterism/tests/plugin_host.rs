mod common;

use asterism_core::daemon::Daemon;
use asterism_core::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use serde_json::{json, Value};

fn daemon(home: &std::path::Path, token: bool) -> std::sync::Arc<Daemon> {
    let paths = Paths {
        home: home.join("h"),
    };
    paths.ensure_dirs().unwrap();
    common::link_fixture(&paths);
    if token {
        common::set_fixture_token(&paths);
    }
    Daemon::with_options(paths, common::daemon_options()).unwrap()
}

async fn host_call(daemon: &Daemon, method: &str) -> asterism_core::error::Result<Value> {
    daemon
        .plugin_call(
            "echo",
            "echo.host",
            json!({"method": method}),
            Some(daemon.call_timeout()),
        )
        .await
}

#[tokio::test]
async fn plugins_reach_the_daemon_through_the_host_api() {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    common::init_repo(repo.path());
    let daemon = daemon(home.path(), true);
    let project = daemon
        .add_project(&repo.path().display().to_string())
        .unwrap();
    let listed = host_call(&daemon, "project.list").await.unwrap();
    assert_eq!(listed[0]["id"], project.id);
}

#[tokio::test]
async fn host_api_denies_plugin_management_and_streams() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path(), true);
    for method in [
        "plugin.reload",
        "store.add",
        "subscribe",
        "session.attach",
        "shutdown",
        "pr.list",
    ] {
        let err = host_call(&daemon, method).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::PluginError, "{method}");
        assert!(
            err.message.contains("not available to plugins"),
            "{method}: {}",
            err.message
        );
    }
}

#[tokio::test]
async fn missing_required_settings_block_calls() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path(), false);
    let err = daemon
        .plugin_call(
            "echo",
            "forge.status",
            json!({}),
            Some(daemon.call_timeout()),
        )
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NeedsSetup);
    assert!(err.message.contains("Token is not set"), "{}", err.message);
}

#[tokio::test]
async fn broken_links_file_does_not_stop_the_daemon() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: home.path().join("h"),
    };
    paths.ensure_dirs().unwrap();
    std::fs::create_dir_all(paths.plugins_dir()).unwrap();
    std::fs::write(paths.plugin_links(), "links = [").unwrap();
    let daemon = Daemon::with_options(paths, common::daemon_options()).unwrap();
    let set = daemon.plugin_set();
    let names: Vec<_> = set
        .registry
        .plugins()
        .iter()
        .map(|p| p.name.clone())
        .collect();
    assert_eq!(names, ["claude", "github", "linear"]);
    assert_eq!(
        daemon
            .plugin_call("nope", "x", json!({}), None)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::PluginError
    );
}
