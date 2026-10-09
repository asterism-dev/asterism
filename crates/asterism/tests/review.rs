mod common;

use std::path::PathBuf;
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
    review: PathBuf,
    log: PathBuf,
    daemon: Arc<Daemon>,
}

fn setup() -> Env {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    common::init_repo(repo.path());
    common::run_git(
        repo.path(),
        &["remote", "add", "origin", "https://echo.test/acme/demo.git"],
    );
    let paths = Paths {
        home: home.path().join("h"),
    };
    paths.ensure_dirs().unwrap();
    common::link_fixture(&paths);
    common::set_fixture_token(&paths);
    let prs = home.path().join("prs.json");
    let review = home.path().join("review.json");
    let log = home.path().join("fixture.log");
    let options = DaemonOptions {
        plugin_env: vec![
            ("FIXTURE_PRS".into(), prs.display().to_string()),
            ("FIXTURE_REVIEW".into(), review.display().to_string()),
            ("FIXTURE_LOG".into(), log.display().to_string()),
        ],
        ..common::daemon_options()
    };
    let daemon = Daemon::with_options(paths, options).unwrap();
    Env {
        _home: home,
        repo,
        prs,
        review,
        log,
        daemon,
    }
}

async fn task(env: &Env) -> Task {
    let project = env
        .daemon
        .add_project(&env.repo.path().display().to_string())
        .unwrap();
    let params = TaskCreateParams {
        project_id: project.id,
        title: "t".into(),
        ..Default::default()
    };
    env.daemon.create_task(params).await.unwrap().task
}

/// Commits `content` to `a.rs` in the task worktree; returns the commit sha.
fn commit(task: &Task, content: &str) -> String {
    let wt = std::path::Path::new(&task.worktree_path);
    std::fs::write(wt.join("a.rs"), content).unwrap();
    common::run_git(wt, &["add", "."]);
    common::run_git(wt, &["commit", "-q", "-m", "c"]);
    common::run_git(wt, &["rev-parse", "HEAD"])
        .trim()
        .to_string()
}

/// Gives the task a PR #7 whose head is `head`.
async fn with_pr(env: &Env, task: &Task, head: &str) {
    std::fs::write(
        &env.prs,
        json!({ &task.branch: {"number": 7, "url": "https://echo.test/pr/7", "title": "T",
        "state": "open", "review": "none", "checks": {"state": "success", "failing": []}}})
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        &env.review,
        json!({"head_sha": head, "base_sha": "", "head_ref": "refs/pull/7/head",
        "threads": [], "conversation": [], "viewed_files": [], "pending_review": null})
        .to_string(),
    )
    .unwrap();
    env.daemon.refresh_prs(task.project_id).await.unwrap();
}

fn get(source: ReviewSource, task: &Task) -> ReviewGetParams {
    ReviewGetParams {
        task_id: task.id,
        source,
    }
}

#[tokio::test]
async fn local_mode_shows_the_worktree_diff_without_a_forge() {
    let env = setup();
    let task = task(&env).await;
    commit(&task, "one\n");
    let r = env
        .daemon
        .review_get(&get(ReviewSource::Local, &task))
        .await
        .unwrap();
    assert!(r.patch.contains("+one"));
    assert_eq!((r.pr, r.reviews_supported), (None, false));
    assert!(!std::fs::read_to_string(&env.log)
        .unwrap_or_default()
        .contains("review get"));
}

#[tokio::test]
async fn pr_mode_diffs_merge_base_to_head() {
    let env = setup();
    let task = task(&env).await;
    let head = commit(&task, "one\n");
    with_pr(&env, &task, &head).await;
    std::fs::write(
        std::path::Path::new(&task.worktree_path).join("a.rs"),
        "dirty\n",
    )
    .unwrap();
    let r = env
        .daemon
        .review_get(&get(ReviewSource::Pr, &task))
        .await
        .unwrap();
    assert_eq!((r.pr, r.reviews_supported), (Some(7), true));
    assert!(r.patch.contains("+one") && !r.patch.contains("dirty"));
    assert!(r.local_ahead, "uncommitted changes differ from the PR head");
}

#[tokio::test]
async fn pr_mode_reports_forge_errors_and_local_still_works() {
    let env = setup();
    let task = task(&env).await;
    let head = commit(&task, "one\n");
    with_pr(&env, &task, &head).await;
    std::fs::write(
        &env.review,
        json!({"error": "gh is not logged in"}).to_string(),
    )
    .unwrap();
    let err = env
        .daemon
        .review_get(&get(ReviewSource::Pr, &task))
        .await
        .unwrap_err();
    assert!(err.message.contains("not logged in"), "{}", err.message);
    assert!(env
        .daemon
        .review_get(&get(ReviewSource::Local, &task))
        .await
        .is_ok());
}

#[tokio::test]
async fn local_viewed_resets_when_the_file_changes() {
    let env = setup();
    let task = task(&env).await;
    commit(&task, "one\n");
    env.daemon
        .review_set_viewed(&ReviewViewedParams {
            task_id: task.id,
            source: ReviewSource::Local,
            path: "a.rs".into(),
            viewed: true,
        })
        .await
        .unwrap();
    let r = env
        .daemon
        .review_get(&get(ReviewSource::Local, &task))
        .await
        .unwrap();
    assert_eq!(r.viewed_files, vec!["a.rs".to_string()]);
    commit(&task, "two\n");
    let r = env
        .daemon
        .review_get(&get(ReviewSource::Local, &task))
        .await
        .unwrap();
    assert!(r.viewed_files.is_empty());
}

#[allow(dead_code)]
fn log_lines(env: &Env, prefix: &str) -> Vec<Value> {
    std::fs::read_to_string(&env.log)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix(prefix))
        .map(|l| serde_json::from_str(l.split_once(' ').map_or(l, |(_, j)| j)).unwrap())
        .collect()
}
