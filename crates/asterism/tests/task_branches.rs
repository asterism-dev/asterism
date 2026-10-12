mod common;

use std::path::{Path, PathBuf};
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
    let paths = Paths {
        home: home.path().join("h"),
    };
    paths.ensure_dirs().unwrap();
    let origin = tempfile::tempdir().unwrap();
    run_git(origin.path(), &["init", "-q", "--bare", "-b", "main"]);
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    run_git(
        repo.path(),
        &[
            "remote",
            "add",
            "origin",
            &origin.path().display().to_string(),
        ],
    );
    run_git(
        repo.path(),
        &["push", "-q", "origin", "main", "main:feature/pr"],
    );
    run_git(
        repo.path(),
        &["update-ref", "-d", "refs/remotes/origin/feature/pr"],
    );
    let daemon = Daemon::with_options(paths, common::daemon_options()).unwrap();
    let project_id = daemon
        .add_project(&repo.path().display().to_string())
        .unwrap()
        .id;
    Env {
        _home: home,
        origin,
        repo,
        daemon,
        project_id,
    }
}

fn params(env: &Env, title: &str) -> TaskCreateParams {
    TaskCreateParams {
        project_id: env.project_id,
        title: title.into(),
        ..Default::default()
    }
}

fn task_count(env: &Env) -> usize {
    env.daemon
        .tasks(TaskListParams {
            project_id: Some(env.project_id),
            include_archived: true,
        })
        .unwrap()
        .len()
}

#[tokio::test]
async fn explicit_branches_name_the_branch_and_worktree() {
    let env = setup();
    let wanted = TaskCreateParams {
        branch: Some("asterism/fix-login-x8d4t".into()),
        push: true,
        ..params(&env, "fix-login")
    };
    let created = env.daemon.create_task(wanted.clone()).await.unwrap();
    assert_eq!(created.task.title, "fix-login");
    assert_eq!(created.task.branch, "asterism/fix-login-x8d4t");
    assert!(
        created
            .task
            .worktree_path
            .ends_with("/asterism/fix-login-x8d4t"),
        "{}",
        created.task.worktree_path
    );
    assert_eq!(created.warning, None);
    run_git(
        env.origin.path(),
        &[
            "rev-parse",
            "--verify",
            "refs/heads/asterism/fix-login-x8d4t",
        ],
    );
    let again = env.daemon.create_task(wanted).await.unwrap_err();
    assert_eq!(again.kind, ErrorKind::BranchExists);
    assert!(
        again.message.contains("asterism/fix-login-x8d4t"),
        "{}",
        again.message
    );
    assert_eq!(task_count(&env), 1);
}

#[tokio::test]
async fn invalid_explicit_branches_leave_no_task() {
    let env = setup();
    for bad in ["bad..name", "-x", "a@{1}"] {
        let err = env
            .daemon
            .create_task(TaskCreateParams {
                branch: Some(bad.into()),
                ..params(&env, "t")
            })
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidParams, "{bad}");
    }
    assert_eq!(task_count(&env), 0);
}

#[tokio::test]
async fn a_failed_push_is_only_a_warning() {
    let env = setup();
    run_git(
        env.repo.path(),
        &["remote", "set-url", "origin", "/nonexistent/origin.git"],
    );
    let created = env
        .daemon
        .create_task(TaskCreateParams {
            branch: Some("asterism/offline-ab12c".into()),
            push: true,
            ..params(&env, "offline")
        })
        .await
        .unwrap();
    assert!(created
        .warning
        .unwrap()
        .contains("Couldn't push `asterism/offline-ab12c`"));
    assert!(asterism_core::git::branch_exists(
        env.repo.path(),
        "asterism/offline-ab12c"
    ));
}

#[tokio::test]
async fn checkout_tracks_a_branch_that_only_exists_on_origin() {
    let env = setup();
    let created = env
        .daemon
        .create_task(TaskCreateParams {
            checkout: Some("feature/pr".into()),
            ..params(&env, "add-search")
        })
        .await
        .unwrap();
    assert_eq!(created.task.branch, "feature/pr");
    assert!(
        created.task.worktree_path.ends_with("/feature/pr"),
        "{}",
        created.task.worktree_path
    );
    let wt = std::path::Path::new(&created.task.worktree_path);
    assert_eq!(
        run_git(wt, &["rev-parse", "--abbrev-ref", "@{upstream}"]).trim(),
        "origin/feature/pr"
    );
}

#[tokio::test]
async fn checkout_errors_are_clear_and_leave_no_task() {
    let env = setup();
    let missing = env
        .daemon
        .create_task(TaskCreateParams {
            checkout: Some("nope".into()),
            ..params(&env, "t")
        })
        .await
        .unwrap_err();
    assert_eq!(missing.kind, ErrorKind::NotFound);
    assert!(
        missing.message.contains("not found locally or on origin"),
        "{}",
        missing.message
    );
    let busy = env
        .daemon
        .create_task(TaskCreateParams {
            checkout: Some("main".into()),
            ..params(&env, "t")
        })
        .await
        .unwrap_err();
    assert_eq!(busy.kind, ErrorKind::BranchExists);
    assert!(
        busy.message.contains("already checked out"),
        "{}",
        busy.message
    );
    let mixed = TaskCreateParams {
        checkout: Some("feature/pr".into()),
        branch: Some("x".into()),
        ..params(&env, "t")
    };
    assert_eq!(
        env.daemon.create_task(mixed).await.unwrap_err().kind,
        ErrorKind::InvalidParams
    );
    assert_eq!(task_count(&env), 0);
}

#[tokio::test]
async fn checkout_of_a_local_branch_works_offline() {
    let env = setup();
    run_git(env.repo.path(), &["branch", "local-only"]);
    run_git(
        env.repo.path(),
        &["remote", "set-url", "origin", "/nonexistent/origin.git"],
    );
    let created = env
        .daemon
        .create_task(TaskCreateParams {
            checkout: Some("local-only".into()),
            ..params(&env, "t")
        })
        .await
        .unwrap();
    assert_eq!(created.task.branch, "local-only");
}

#[tokio::test]
async fn branches_report_the_worktree_root() {
    let env = setup();
    let branches = env.daemon.project_branches(env.project_id).unwrap();
    assert_eq!(
        (branches.local, branches.remote),
        (
            vec!["main".to_string()],
            vec!["origin/feature/pr".to_string(), "origin/main".to_string()]
        )
    );
    let root = branches.worktree_root.unwrap();
    let created = env
        .daemon
        .create_task(TaskCreateParams {
            branch: Some("asterism/a-11111".into()),
            ..params(&env, "a")
        })
        .await
        .unwrap();
    assert_eq!(
        created.task.worktree_path,
        format!("{root}/asterism/a-11111")
    );
}

#[tokio::test]
async fn checkout_rejects_a_branch_deleted_on_origin_despite_a_stale_tracking_ref() {
    let env = setup();
    run_git(
        env.repo.path(),
        &[
            "fetch",
            "-q",
            "origin",
            "feature/pr:refs/remotes/origin/feature/pr",
        ],
    );
    run_git(env.origin.path(), &["branch", "-D", "feature/pr"]);
    run_git(
        env.repo.path(),
        &["rev-parse", "--verify", "refs/remotes/origin/feature/pr"],
    );
    let err = env
        .daemon
        .create_task(TaskCreateParams {
            checkout: Some("feature/pr".into()),
            ..params(&env, "t")
        })
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotFound);
    assert!(
        err.message.contains("not found locally or on origin"),
        "{}",
        err.message
    );
    assert_eq!(task_count(&env), 0);
}

#[tokio::test]
async fn checkout_warns_when_the_local_branch_differs_from_origin() {
    let env = setup();
    run_git(env.repo.path(), &["branch", "feature/pr", "main"]);
    let current = env
        .daemon
        .create_task(TaskCreateParams {
            checkout: Some("feature/pr".into()),
            ..params(&env, "a")
        })
        .await
        .unwrap();
    assert_eq!(current.warning, None);
    env.daemon.delete_task(current.task.id, false).unwrap();

    let other = tempfile::tempdir().unwrap();
    run_git(
        other.path(),
        &["clone", "-q", &env.origin.path().display().to_string(), "."],
    );
    run_git(
        other.path(),
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@t",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "more",
        ],
    );
    run_git(other.path(), &["push", "-q", "origin", "HEAD:feature/pr"]);
    let created = env
        .daemon
        .create_task(TaskCreateParams {
            checkout: Some("feature/pr".into()),
            ..params(&env, "b")
        })
        .await
        .unwrap();
    let warning = created.warning.unwrap();
    assert!(
        warning.contains("differs from origin/feature/pr"),
        "{warning}"
    );
    assert!(warning.contains("1 behind"), "{warning}");
}

#[tokio::test]
async fn worktrees_mirror_the_branch_path_and_deleting_tidies_parents() {
    let env = setup();
    run_git(env.repo.path(), &["branch", "other/pr"]);
    let first = env
        .daemon
        .create_task(TaskCreateParams {
            checkout: Some("feature/pr".into()),
            ..params(&env, "a")
        })
        .await
        .unwrap();
    let second = env
        .daemon
        .create_task(TaskCreateParams {
            checkout: Some("other/pr".into()),
            ..params(&env, "b")
        })
        .await
        .unwrap();
    assert!(
        first.task.worktree_path.ends_with("/feature/pr"),
        "{}",
        first.task.worktree_path
    );
    assert!(
        second.task.worktree_path.ends_with("/other/pr"),
        "{}",
        second.task.worktree_path
    );
    let parent = std::path::Path::new(&second.task.worktree_path)
        .parent()
        .unwrap()
        .to_path_buf();
    env.daemon.delete_task(second.task.id, false).unwrap();
    assert!(
        !parent.exists(),
        "empty {} should be removed",
        parent.display()
    );
    assert!(std::path::Path::new(&first.task.worktree_path).exists());
}

#[tokio::test]
async fn branches_nested_inside_an_old_flat_worktree_are_rejected() {
    let env = setup();
    let flat = env.daemon.create_task(params(&env, "old")).await.unwrap();
    let dir = std::path::Path::new(&flat.task.worktree_path)
        .file_name()
        .unwrap()
        .to_str()
        .unwrap()
        .to_string();
    let err = env
        .daemon
        .create_task(TaskCreateParams {
            branch: Some(format!("{dir}/x")),
            ..params(&env, "t")
        })
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::BranchExists);
    assert!(
        err.message.contains("inside the worktree"),
        "{}",
        err.message
    );
    assert_eq!(task_count(&env), 1);
}

fn explicit(
    env: &Env,
    title: &str,
    branch: &str,
    on_conflict: Option<OnConflict>,
) -> TaskCreateParams {
    TaskCreateParams {
        branch: Some(branch.into()),
        on_conflict,
        ..params(env, title)
    }
}

fn worktree_root(env: &Env) -> PathBuf {
    PathBuf::from(
        env.daemon
            .project_branches(env.project_id)
            .unwrap()
            .worktree_root
            .unwrap(),
    )
}

/// A worktree no task knows about, holding one untracked file.
fn orphan_worktree(env: &Env, args: &[&str], dir: &str) -> PathBuf {
    let path = worktree_root(env).join(dir);
    let shown = path.display().to_string();
    let mut cmd = vec!["worktree", "add", "-q"];
    cmd.extend_from_slice(args);
    cmd.push(&shown);
    if args.is_empty() {
        cmd.push(dir);
    }
    run_git(env.repo.path(), &cmd);
    std::fs::write(path.join("stale.txt"), "x").unwrap();
    path
}

fn head(dir: &str) -> String {
    run_git(Path::new(dir), &["rev-parse", "--abbrev-ref", "HEAD"])
        .trim()
        .to_string()
}

#[tokio::test]
async fn reuse_attaches_a_second_task_to_the_existing_worktree() {
    let env = setup();
    let first = env
        .daemon
        .create_task(explicit(&env, "a", "feature/x", None))
        .await
        .unwrap();
    let second = env
        .daemon
        .create_task(explicit(&env, "b", "feature/x", Some(OnConflict::Reuse)))
        .await
        .unwrap();
    assert_eq!(second.task.worktree_path, first.task.worktree_path);
    assert_eq!(second.task.branch, "feature/x");
    assert_eq!(task_count(&env), 2);
}

#[tokio::test]
async fn reuse_checks_out_an_existing_branch_without_worktree() {
    let env = setup();
    run_git(env.repo.path(), &["branch", "feature/y"]);
    let created = env
        .daemon
        .create_task(explicit(&env, "t", "feature/y", Some(OnConflict::Reuse)))
        .await
        .unwrap();
    assert!(
        created.task.worktree_path.ends_with("/feature/y"),
        "{}",
        created.task.worktree_path
    );
    assert_eq!(created.task.branch, "feature/y");
    assert_eq!(head(&created.task.worktree_path), "feature/y");
}

#[tokio::test]
async fn reuse_of_a_hand_deleted_worktree_prunes_and_checks_out_again() {
    let env = setup();
    let path = worktree_root(&env).join("feature/gone");
    run_git(
        env.repo.path(),
        &[
            "worktree",
            "add",
            "-q",
            "-b",
            "feature/gone",
            &path.display().to_string(),
        ],
    );
    std::fs::remove_dir_all(&path).unwrap();
    let created = env
        .daemon
        .create_task(explicit(&env, "t", "feature/gone", Some(OnConflict::Reuse)))
        .await
        .unwrap();
    assert_eq!(head(&created.task.worktree_path), "feature/gone");
}

#[tokio::test]
async fn suffix_picks_the_next_free_branch_and_directory() {
    let env = setup();
    env.daemon
        .create_task(explicit(&env, "a", "feature/x", None))
        .await
        .unwrap();
    run_git(env.repo.path(), &["branch", "feature/x-2"]);
    let created = env
        .daemon
        .create_task(explicit(&env, "b", "feature/x", Some(OnConflict::Suffix)))
        .await
        .unwrap();
    assert_eq!(created.task.branch, "feature/x-3");
    assert!(
        created.task.worktree_path.ends_with("/feature/x-3"),
        "{}",
        created.task.worktree_path
    );
}

#[tokio::test]
async fn on_conflict_without_a_conflict_creates_normally() {
    let env = setup();
    let created = env
        .daemon
        .create_task(explicit(
            &env,
            "t",
            "feature/fresh",
            Some(OnConflict::Replace),
        ))
        .await
        .unwrap();
    assert_eq!(created.task.branch, "feature/fresh");
}

#[tokio::test]
async fn replace_recreates_an_orphaned_worktree_and_branch_from_base() {
    let env = setup();
    let wt = orphan_worktree(&env, &["-b", "feature/x"], "feature/x");
    run_git(&wt, &["add", "."]);
    run_git(&wt, &["commit", "-qm", "stale"]);
    let created = env
        .daemon
        .create_task(explicit(&env, "t", "feature/x", Some(OnConflict::Replace)))
        .await
        .unwrap();
    assert!(created.task.worktree_path.ends_with("/feature/x"));
    assert!(!Path::new(&created.task.worktree_path)
        .join("stale.txt")
        .exists());
    assert_eq!(
        run_git(env.repo.path(), &["rev-list", "--count", "main..feature/x"]).trim(),
        "0"
    );
}

#[tokio::test]
async fn replace_is_refused_while_a_task_owns_the_worktree() {
    let env = setup();
    let first = env
        .daemon
        .create_task(explicit(&env, "a", "feature/x", None))
        .await
        .unwrap();
    let err = env
        .daemon
        .create_task(explicit(&env, "b", "feature/x", Some(OnConflict::Replace)))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    assert!(err.message.contains("\"a\""), "{}", err.message);
    assert!(Path::new(&first.task.worktree_path).exists());
    assert_eq!(task_count(&env), 1);
}

#[tokio::test]
async fn replace_is_refused_when_the_branch_is_checked_out_elsewhere() {
    let env = setup();
    let elsewhere = tempfile::tempdir().unwrap();
    let elsewhere = elsewhere.path().join("x");
    run_git(env.repo.path(), &["branch", "feature/x"]);
    run_git(
        env.repo.path(),
        &[
            "worktree",
            "add",
            "-q",
            &elsewhere.display().to_string(),
            "feature/x",
        ],
    );
    let err = env
        .daemon
        .create_task(explicit(&env, "t", "feature/x", Some(OnConflict::Replace)))
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    assert!(elsewhere.exists());
    assert!(asterism_core::git::branch_exists(
        env.repo.path(),
        "feature/x"
    ));
    assert_eq!(task_count(&env), 0);
}

#[tokio::test]
async fn foreign_directories_are_never_reused_or_replaced() {
    let env = setup();
    let dir = worktree_root(&env).join("feature/z");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("keep.txt"), "x").unwrap();
    for on in [OnConflict::Reuse, OnConflict::Replace] {
        let err = env
            .daemon
            .create_task(explicit(&env, "t", "feature/z", Some(on)))
            .await
            .unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidParams, "{on:?}");
    }
    assert!(dir.join("keep.txt").exists());
    assert_eq!(task_count(&env), 0);
}

#[tokio::test]
async fn checkout_conflicts_allow_reuse_and_replace_but_not_suffix() {
    let env = setup();
    run_git(env.repo.path(), &["branch", "other/pr"]);
    let co = |title: &str, on: Option<OnConflict>| TaskCreateParams {
        checkout: Some("other/pr".into()),
        on_conflict: on,
        ..params(&env, title)
    };
    let first = env.daemon.create_task(co("a", None)).await.unwrap();
    assert_eq!(
        env.daemon
            .create_task(co("b", None))
            .await
            .unwrap_err()
            .kind,
        ErrorKind::BranchExists
    );
    assert_eq!(
        env.daemon
            .create_task(co("b", Some(OnConflict::Suffix)))
            .await
            .unwrap_err()
            .kind,
        ErrorKind::InvalidParams
    );
    let reused = env
        .daemon
        .create_task(co("b", Some(OnConflict::Reuse)))
        .await
        .unwrap();
    assert_eq!(reused.task.worktree_path, first.task.worktree_path);
}

#[tokio::test]
async fn checkout_replace_recreates_the_worktree_and_keeps_the_branch() {
    let env = setup();
    run_git(env.repo.path(), &["branch", "other/pr"]);
    let wt = orphan_worktree(&env, &[], "other/pr");
    let created = env
        .daemon
        .create_task(TaskCreateParams {
            checkout: Some("other/pr".into()),
            on_conflict: Some(OnConflict::Replace),
            ..params(&env, "t")
        })
        .await
        .unwrap();
    assert_eq!(created.task.branch, "other/pr");
    assert!(!wt.join("stale.txt").exists());
    assert_eq!(head(&created.task.worktree_path), "other/pr");
}

#[tokio::test]
async fn shared_worktrees_survive_until_their_last_task_is_deleted() {
    let env = setup();
    let first = env
        .daemon
        .create_task(explicit(&env, "a", "feature/x", None))
        .await
        .unwrap();
    let second = env
        .daemon
        .create_task(explicit(&env, "b", "feature/x", Some(OnConflict::Reuse)))
        .await
        .unwrap();
    assert_eq!(
        env.daemon.delete_check(first.task.id).unwrap().shared_with,
        vec![second.task.id]
    );
    let listed = env.daemon.project_worktrees(env.project_id).unwrap();
    let shared = listed
        .iter()
        .find(|w| w.branch.as_deref() == Some("feature/x"))
        .unwrap();
    assert_eq!(shared.task_ids, vec![first.task.id, second.task.id]);

    env.daemon.delete_task(first.task.id, true).unwrap();
    assert!(Path::new(&second.task.worktree_path).exists());
    assert!(asterism_core::git::branch_exists(
        env.repo.path(),
        "feature/x"
    ));
    assert!(env
        .daemon
        .delete_check(second.task.id)
        .unwrap()
        .shared_with
        .is_empty());

    env.daemon.delete_task(second.task.id, true).unwrap();
    assert!(!Path::new(&second.task.worktree_path).exists());
    assert!(!asterism_core::git::branch_exists(
        env.repo.path(),
        "feature/x"
    ));
}

#[tokio::test]
async fn branches_list_worktrees_under_the_root_with_their_tasks() {
    let env = setup();
    let first = env
        .daemon
        .create_task(explicit(&env, "a", "feature/x", None))
        .await
        .unwrap();
    let second = env
        .daemon
        .create_task(explicit(&env, "b", "feature/x", Some(OnConflict::Reuse)))
        .await
        .unwrap();
    env.daemon.archive_task(second.task.id).unwrap();
    orphan_worktree(&env, &["-b", "loose"], "loose");

    let mut listed = env
        .daemon
        .project_branches(env.project_id)
        .unwrap()
        .worktrees;
    listed.sort_by(|a, b| a.dir.cmp(&b.dir));
    assert_eq!(
        listed,
        vec![
            BranchWorktree {
                dir: "feature/x".into(),
                branch: Some("feature/x".into()),
                tasks: vec![
                    WorktreeTask {
                        id: first.task.id,
                        title: "a".into(),
                        archived: false
                    },
                    WorktreeTask {
                        id: second.task.id,
                        title: "b".into(),
                        archived: true
                    },
                ],
            },
            BranchWorktree {
                dir: "loose".into(),
                branch: Some("loose".into()),
                tasks: vec![],
            },
        ]
    );
}
