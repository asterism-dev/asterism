mod common;

use asterism_core::daemon::{issue_name, issue_prompt, Daemon};
use asterism_core::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::*;
use serde_json::json;

fn daemon(home: &std::path::Path, token: bool, mode: &str) -> std::sync::Arc<Daemon> {
    let paths = Paths { home: home.join("h") };
    paths.ensure_dirs().unwrap();
    common::link_fixture(&paths);
    if token {
        common::set_fixture_token(&paths);
    }
    let options = asterism_core::daemon::DaemonOptions {
        plugin_env: vec![("FIXTURE_MODE".into(), mode.into())],
        ..common::daemon_options()
    };
    Daemon::with_options(paths, options).unwrap()
}

fn project(daemon: &Daemon) -> (tempfile::TempDir, i64) {
    let repo = tempfile::tempdir().unwrap();
    common::init_repo(repo.path());
    let id = daemon.add_project(&repo.path().display().to_string()).unwrap().id;
    (repo, id)
}

#[tokio::test]
async fn sources_are_listed_with_availability() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), true, "");
    let (_repo, project_id) = project(&d);
    let all = d.task_sources(None).await.unwrap();
    let echo = all.iter().find(|s| s.id == "echo-issues").unwrap();
    assert_eq!((echo.plugin.as_str(), echo.display_name.as_str(), echo.available), ("echo", "Echo issues", true));
    let scoped = d.task_sources(Some(project_id)).await.unwrap();
    assert!(scoped.iter().find(|s| s.id == "echo-issues").unwrap().available);

    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), true, "no-repo");
    let (_repo, project_id) = project(&d);
    let echo = d.task_sources(Some(project_id)).await.unwrap().into_iter().find(|s| s.id == "echo-issues").unwrap();
    assert_eq!((echo.available, echo.reason.as_deref()), (false, Some("project has no echo repository")));
}

#[tokio::test]
async fn needs_setup_is_its_own_kind_and_counts_as_available() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), false, "");
    let (_repo, project_id) = project(&d);
    let echo = d.task_sources(Some(project_id)).await.unwrap().into_iter().find(|s| s.id == "echo-issues").unwrap();
    assert!(echo.available);
    let err = d
        .task_source_search(&TaskSourceSearchParams { project_id, source: "echo-issues".into(), query: String::new(), assigned_to_me: true })
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NeedsSetup);
    assert!(err.message.contains("Token is not set"), "{}", err.message);
}

#[tokio::test]
async fn search_and_get_reach_the_plugin() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), true, "");
    let (_repo, project_id) = project(&d);
    let hits = d
        .task_source_search(&TaskSourceSearchParams { project_id, source: "echo-issues".into(), query: "login".into(), assigned_to_me: false })
        .await
        .unwrap();
    assert_eq!(hits.iter().map(|h| h.key.as_str()).collect::<Vec<_>>(), ["ECH-1"]);

    let got = d.task_source_get(&TaskSourceGetParams { project_id, source: "echo-issues".into(), key: "ECH-1".into() }).await.unwrap();
    assert_eq!(got.name, "ech-1-fix-login-timeout");
    assert_eq!(got.branch, "feature/ech-1-fix-login-timeout");
    assert_eq!(got.prompt, "# Fix login timeout\n\nhttps://echo.test/ECH-1\n\nUsers get logged out.");

    let plain = d.task_source_get(&TaskSourceGetParams { project_id, source: "echo-issues".into(), key: "ECH-2".into() }).await.unwrap();
    assert_eq!((plain.branch.as_str(), plain.prompt.as_str()), ("asterism/ech-2-add-dark-mode", "# Add dark mode\n\nhttps://echo.test/ECH-2"));

    let missing = d.task_source_get(&TaskSourceGetParams { project_id, source: "echo-issues".into(), key: "NOPE".into() }).await.unwrap_err();
    assert_eq!(missing.kind, ErrorKind::NotFound);
    let unknown = d
        .task_source_search(&TaskSourceSearchParams { project_id, source: "nope".into(), query: String::new(), assigned_to_me: false })
        .await
        .unwrap_err();
    assert_eq!(unknown.kind, ErrorKind::NotFound);
}

#[test]
fn issue_names_join_key_and_title_slugs() {
    assert_eq!(issue_name("TRA-1343", "Migrate trading infrastructure unit conversions onto bbase"), "tra-1343-migrate-trading-infrastructure-unit-conversions-onto-bbase");
    assert_eq!(issue_name("#42", "Fix login timeout"), "42-fix-login-timeout");
    assert_eq!(issue_name("ECH-4", "!!!"), "ech-4");
    assert_eq!(issue_name("%%", "%%"), "");
    let long = issue_name("K-1", &"word ".repeat(40));
    assert!(long.len() <= 80 && !long.ends_with('-'), "{long}");
}

#[test]
fn prompts_cut_long_descriptions_on_a_char_boundary() {
    let issue = |description: String| Issue { key: "K-1".into(), title: "T".into(), url: "u".into(), description, branch: None };
    assert_eq!(issue_prompt(&issue("  ".into())), "# T\n\nu");
    let long = format!("{}é tail", "x".repeat(32 * 1024 - 1));
    let prompt = issue_prompt(&issue(long));
    assert!(prompt.ends_with(&"x".repeat(10)), "cut before the multibyte char");
    assert_eq!(prompt.len(), "# T\n\nu\n\n".len() + 32 * 1024 - 1);
}

#[tokio::test]
async fn plugins_cannot_call_task_sources() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), true, "");
    let err = d.plugin_call("echo", "echo.host", json!({"method": "task_source.list"}), Some(d.call_timeout())).await.unwrap_err();
    assert!(err.message.contains("not available to plugins"), "{}", err.message);
}
