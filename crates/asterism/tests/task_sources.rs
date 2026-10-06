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

    let empty = d.task_source_get(&TaskSourceGetParams { project_id, source: "echo-issues".into(), key: "%%".into() }).await.unwrap();
    assert_eq!((empty.name.as_str(), empty.branch.as_str()), ("", ""));

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

fn issue(key: &str, title: &str, branch: Option<&str>) -> TaskIssue {
    TaskIssue { source: "echo-issues".into(), key: key.into(), title: title.into(), url: format!("https://echo.test/{key}"), branch: branch.map(String::from) }
}

async fn create(d: &std::sync::Arc<Daemon>, project_id: i64, title: &str, issue: TaskIssue) -> asterism_core::error::Result<Task> {
    let params = TaskCreateParams { project_id, title: title.into(), prompt: Some("p".into()), agent: None, issue: Some(issue) };
    d.create_task(params).await.map(|r| r.task)
}

#[tokio::test]
async fn issue_tasks_take_their_name_and_branch_from_the_issue() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), true, "");
    let (repo, project_id) = project(&d);
    let task = create(&d, project_id, "", issue("ECH-1", "Fix login timeout", Some("feature/ech-1-fix-login-timeout"))).await.unwrap();
    assert_eq!(task.title, "ech-1-fix-login-timeout");
    assert_eq!(task.slug, "ech-1-fix-login-timeout");
    assert_eq!(task.branch, "feature/ech-1-fix-login-timeout");
    assert!(task.worktree_path.ends_with("/ech-1-fix-login-timeout"), "{}", task.worktree_path);
    assert_eq!(task.issue, Some(IssueRef { source: "echo-issues".into(), key: "ECH-1".into(), url: "https://echo.test/ECH-1".into() }));
    assert!(asterism_core::git::branch_exists(repo.path(), "feature/ech-1-fix-login-timeout"));
    assert_eq!(d.task(task.id).unwrap().issue, task.issue);

    let plain = create(&d, project_id, "", issue("#42", "Fix login", None)).await.unwrap();
    assert_eq!((plain.title.as_str(), plain.branch.as_str()), ("42-fix-login", "asterism/42-fix-login"));

    let titled = create(&d, project_id, "My own title", issue("#43", "Other", None)).await.unwrap();
    assert_eq!((titled.title.as_str(), titled.branch.as_str()), ("My own title", "asterism/43-other"));
}

#[tokio::test]
async fn a_second_task_for_the_same_issue_gets_the_task_id_appended() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), true, "");
    let (_repo, project_id) = project(&d);
    let first = create(&d, project_id, "", issue("ECH-2", "Add dark mode", None)).await.unwrap();
    let second = create(&d, project_id, "", issue("ECH-2", "Add dark mode", None)).await.unwrap();
    assert_eq!(first.branch, "asterism/ech-2-add-dark-mode");
    assert_eq!(second.title, format!("ech-2-add-dark-mode-{}", second.id));
    assert_eq!(second.branch, format!("asterism/ech-2-add-dark-mode-{}", second.id));
    assert!(second.worktree_path.ends_with(&format!("/ech-2-add-dark-mode-{}", second.id)));
}

#[tokio::test]
async fn odd_names_fall_back_and_invalid_branches_are_rejected() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), true, "");
    let (_repo, project_id) = project(&d);
    let symbols = create(&d, project_id, "", issue("ECH-4", "!!!", None)).await.unwrap();
    assert_eq!(symbols.branch, "asterism/ech-4");
    let empty = create(&d, project_id, "", issue("%%", "%%", None)).await.unwrap();
    assert_eq!((empty.title.clone(), empty.branch.clone()), (empty.id.to_string(), format!("asterism/{}", empty.id)));
    for bad in ["bad..branch", "-x", "a@{1}"] {
        let err = create(&d, project_id, "", issue("ECH-3", "Bad branch", Some(bad))).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidParams, "{bad}");
    }
    let tasks = d.tasks(TaskListParams { project_id: Some(project_id), include_archived: true }).unwrap();
    assert_eq!(tasks.len(), 2, "rejected branches leave no task behind");
}
