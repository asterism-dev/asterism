mod common;

use std::path::{Path, PathBuf};
use std::sync::Arc;

use asterism_core::daemon::{Daemon, DaemonOptions};
use asterism_core::paths::Paths;
use asterism_proto::types::*;
use serde_json::{json, Value};
use tempfile::TempDir;

struct Env {
    _home: TempDir,
    repo: TempDir,
    prs: PathBuf,
    log: PathBuf,
    daemon: Arc<Daemon>,
}

fn setup(origin: Option<&str>) -> Env {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    common::init_repo(repo.path());
    if let Some(url) = origin {
        common::run_git(repo.path(), &["remote", "add", "origin", url]);
    }
    let paths = Paths { home: home.path().join("h") };
    paths.ensure_dirs().unwrap();
    common::link_fixture(&paths);
    common::set_fixture_token(&paths);
    let prs = home.path().join("prs.json");
    let log = home.path().join("fixture.log");
    let options = DaemonOptions {
        plugin_env: vec![("FIXTURE_PRS".into(), prs.display().to_string()), ("FIXTURE_LOG".into(), log.display().to_string())],
        ..common::daemon_options()
    };
    let daemon = Daemon::with_options(paths, options).unwrap();
    Env { _home: home, repo, prs, log, daemon }
}

fn pr(number: u64, state: &str) -> Value {
    json!({"number": number, "url": format!("https://echo.test/pr/{number}"), "title": "T", "state": state,
           "review": "none", "checks": {"state": "success", "failing": []}})
}

fn write(path: &Path, value: Value) {
    std::fs::write(path, value.to_string()).unwrap();
}

async fn task(env: &Env, title: &str) -> Task {
    let project = env.daemon.add_project(&env.repo.path().display().to_string()).unwrap();
    let params = TaskCreateParams { project_id: project.id, title: title.into(), prompt: None, agent: None, issue: None };
    env.daemon.create_task(params).await.unwrap().task
}

fn asked(env: &Env) -> Vec<String> {
    std::fs::read_to_string(&env.log).unwrap_or_default().lines().filter_map(|l| l.strip_prefix("prs ")).map(String::from).collect()
}

fn drain(rx: &mut tokio::sync::broadcast::Receiver<Event>) -> Vec<(i64, Option<u64>)> {
    let mut out = Vec::new();
    while let Ok(event) = rx.try_recv() {
        if let Event::PrChanged { task_id, pr } = event {
            out.push((task_id, pr.map(|p| p.number)));
        }
    }
    out
}

#[tokio::test]
async fn prs_are_matched_to_tasks_and_changes_are_emitted_once() {
    let env = setup(Some("https://echo.test/acme/demo.git"));
    let a = task(&env, "Fix login").await;
    let b = task(&env, "Add docs").await;
    write(&env.prs, json!({ (a.branch.clone()): pr(7, "open") }));
    let mut rx = env.daemon.subscribe();

    let list = env.daemon.refresh_prs(a.project_id).await.unwrap();
    assert_eq!(list.prs.iter().map(|p| (p.task_id, p.pr.number)).collect::<Vec<_>>(), vec![(a.id, 7)]);
    assert!(list.errors.is_empty());
    assert_eq!(drain(&mut rx), vec![(a.id, Some(7))]);
    env.daemon.refresh_prs(a.project_id).await.unwrap();
    assert!(drain(&mut rx).is_empty(), "an unchanged poll is silent");
    assert_eq!(env.daemon.pr_list(None).prs.len(), 1);
    assert_eq!(asked(&env).last().unwrap(), &format!("{},{}", a.branch, b.branch));

    write(&env.prs, json!({ (a.branch.clone()): pr(7, "merged"), (b.branch.clone()): pr(8, "open") }));
    env.daemon.refresh_prs(a.project_id).await.unwrap();
    env.daemon.refresh_prs(a.project_id).await.unwrap();
    assert_eq!(asked(&env).last().unwrap(), &b.branch, "merged branches are not asked again");
    let list = env.daemon.pr_list(Some(a.project_id));
    assert_eq!(list.prs.iter().find(|p| p.task_id == a.id).unwrap().pr.state, PrState::Merged);

    env.daemon.archive_task(b.id).unwrap();
    assert!(drain(&mut rx).contains(&(b.id, None)));
    assert!(env.daemon.pr_list(None).prs.iter().all(|p| p.task_id != b.id));
}

#[tokio::test]
async fn errors_are_kept_per_project_until_a_poll_succeeds() {
    let env = setup(Some("git@echo.test:acme/demo.git"));
    let a = task(&env, "Fix login").await;
    write(&env.prs, json!({ (a.branch.clone()): pr(7, "open") }));
    env.daemon.refresh_prs(a.project_id).await.unwrap();
    write(&env.prs, json!({"error": "API rate limit exceeded"}));
    let list = env.daemon.refresh_prs(a.project_id).await.unwrap();
    assert_eq!(list.errors, vec![PrProjectError { project_id: a.project_id, message: "API rate limit exceeded".into() }]);
    assert_eq!(list.prs.len(), 1, "the last known status stays");
    write(&env.prs, json!({ (a.branch.clone()): pr(7, "open") }));
    assert!(env.daemon.refresh_prs(a.project_id).await.unwrap().errors.is_empty());
}

#[tokio::test]
async fn projects_without_a_matching_forge_are_not_polled() {
    for origin in [None, Some("https://elsewhere.test/acme/demo.git")] {
        let env = setup(origin);
        let a = task(&env, "Fix login").await;
        write(&env.prs, json!({ (a.branch.clone()): pr(7, "open") }));
        let list = env.daemon.refresh_prs(a.project_id).await.unwrap();
        assert!(list.prs.is_empty() && list.errors.is_empty());
        assert!(asked(&env).is_empty());
    }
}
