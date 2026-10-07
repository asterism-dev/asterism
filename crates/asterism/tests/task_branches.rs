mod common;

use std::sync::Arc;

use asterism_core::daemon::Daemon;
use asterism_core::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::*;
use common::{init_repo, run_git};
use tempfile::TempDir;

struct Env {
    _home: TempDir,
    origin: TempDir,
    repo: TempDir,
    daemon: Arc<Daemon>,
    project_id: i64,
}

/// Origin has `main` and `feature/pr`; locally `feature/pr` is unknown until fetched.
fn setup() -> Env {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().join("h") };
    paths.ensure_dirs().unwrap();
    let origin = tempfile::tempdir().unwrap();
    run_git(origin.path(), &["init", "-q", "--bare", "-b", "main"]);
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    run_git(repo.path(), &["remote", "add", "origin", &origin.path().display().to_string()]);
    run_git(repo.path(), &["push", "-q", "origin", "main", "main:feature/pr"]);
    run_git(repo.path(), &["update-ref", "-d", "refs/remotes/origin/feature/pr"]);
    let daemon = Daemon::with_options(paths, common::daemon_options()).unwrap();
    let project_id = daemon.add_project(&repo.path().display().to_string()).unwrap().id;
    Env { _home: home, origin, repo, daemon, project_id }
}

fn params(env: &Env, title: &str) -> TaskCreateParams {
    TaskCreateParams { project_id: env.project_id, title: title.into(), ..Default::default() }
}

fn task_count(env: &Env) -> usize {
    env.daemon.tasks(TaskListParams { project_id: Some(env.project_id), include_archived: true }).unwrap().len()
}

#[tokio::test]
async fn explicit_branches_name_the_branch_and_worktree() {
    let env = setup();
    let wanted = TaskCreateParams { branch: Some("asterism/fix-login-x8d4t".into()), push: true, ..params(&env, "fix-login") };
    let created = env.daemon.create_task(wanted.clone()).await.unwrap();
    assert_eq!(created.task.title, "fix-login");
    assert_eq!(created.task.branch, "asterism/fix-login-x8d4t");
    assert!(created.task.worktree_path.ends_with("/fix-login-x8d4t"), "{}", created.task.worktree_path);
    assert_eq!(created.warning, None);
    run_git(env.origin.path(), &["rev-parse", "--verify", "refs/heads/asterism/fix-login-x8d4t"]);
    let again = env.daemon.create_task(wanted).await.unwrap_err();
    assert_eq!(again.kind, ErrorKind::BranchExists);
    assert!(again.message.contains("asterism/fix-login-x8d4t"), "{}", again.message);
    assert_eq!(task_count(&env), 1);
}

#[tokio::test]
async fn invalid_explicit_branches_leave_no_task() {
    let env = setup();
    for bad in ["bad..name", "-x", "a@{1}"] {
        let err = env.daemon.create_task(TaskCreateParams { branch: Some(bad.into()), ..params(&env, "t") }).await.unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidParams, "{bad}");
    }
    assert_eq!(task_count(&env), 0);
}

#[tokio::test]
async fn a_failed_push_is_only_a_warning() {
    let env = setup();
    run_git(env.repo.path(), &["remote", "set-url", "origin", "/nonexistent/origin.git"]);
    let created = env.daemon.create_task(TaskCreateParams { branch: Some("asterism/offline-ab12c".into()), push: true, ..params(&env, "offline") }).await.unwrap();
    assert!(created.warning.unwrap().contains("Couldn't push `asterism/offline-ab12c`"));
    assert!(asterism_core::git::branch_exists(env.repo.path(), "asterism/offline-ab12c"));
}

#[tokio::test]
async fn checkout_tracks_a_branch_that_only_exists_on_origin() {
    let env = setup();
    let created = env.daemon.create_task(TaskCreateParams { checkout: Some("feature/pr".into()), ..params(&env, "add-search") }).await.unwrap();
    assert_eq!(created.task.branch, "feature/pr");
    assert!(created.task.worktree_path.ends_with("/pr"), "{}", created.task.worktree_path);
    let wt = std::path::Path::new(&created.task.worktree_path);
    assert_eq!(run_git(wt, &["rev-parse", "--abbrev-ref", "@{upstream}"]).trim(), "origin/feature/pr");
}

#[tokio::test]
async fn checkout_errors_are_clear_and_leave_no_task() {
    let env = setup();
    let missing = env.daemon.create_task(TaskCreateParams { checkout: Some("nope".into()), ..params(&env, "t") }).await.unwrap_err();
    assert_eq!(missing.kind, ErrorKind::NotFound);
    assert!(missing.message.contains("not found locally or on origin"), "{}", missing.message);
    let busy = env.daemon.create_task(TaskCreateParams { checkout: Some("main".into()), ..params(&env, "t") }).await.unwrap_err();
    assert_eq!(busy.kind, ErrorKind::BranchExists);
    assert!(busy.message.contains("already checked out"), "{}", busy.message);
    let mixed = TaskCreateParams { checkout: Some("feature/pr".into()), branch: Some("x".into()), ..params(&env, "t") };
    assert_eq!(env.daemon.create_task(mixed).await.unwrap_err().kind, ErrorKind::InvalidParams);
    assert_eq!(task_count(&env), 0);
}

#[tokio::test]
async fn checkout_of_a_local_branch_works_offline() {
    let env = setup();
    run_git(env.repo.path(), &["branch", "local-only"]);
    run_git(env.repo.path(), &["remote", "set-url", "origin", "/nonexistent/origin.git"]);
    let created = env.daemon.create_task(TaskCreateParams { checkout: Some("local-only".into()), ..params(&env, "t") }).await.unwrap();
    assert_eq!(created.task.branch, "local-only");
}

#[tokio::test]
async fn branches_report_the_worktree_root() {
    let env = setup();
    let root = env.daemon.project_branches(env.project_id).unwrap().worktree_root.unwrap();
    let created = env.daemon.create_task(TaskCreateParams { branch: Some("asterism/a-11111".into()), ..params(&env, "a") }).await.unwrap();
    assert_eq!(created.task.worktree_path, format!("{root}/a-11111"));
}
