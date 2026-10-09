mod common;

use std::path::PathBuf;
use std::sync::Arc;

use asterism_core::daemon::{Daemon, DaemonOptions};
use asterism_core::paths::Paths;
use asterism_proto::rpc::ErrorKind;
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
async fn pr_mode_diffs_from_the_forge_base() {
    let env = setup();
    let task = task(&env).await;
    let base = commit(&task, "one\n");
    let head = commit(&task, "one\ntwo\n");
    with_pr(&env, &task, &head).await;
    std::fs::write(
        &env.review,
        json!({"head_sha": head, "base_sha": base, "head_ref": "refs/pull/7/head",
        "threads": [], "conversation": [], "viewed_files": [], "pending_review": null})
        .to_string(),
    )
    .unwrap();
    let r = env
        .daemon
        .review_get(&get(ReviewSource::Pr, &task))
        .await
        .unwrap();
    assert!(
        r.patch.contains("+two") && !r.patch.contains("+one"),
        "{}",
        r.patch
    );
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
    let forge_calls = log_lines(&env, "review get").len();
    assert!(env
        .daemon
        .review_get(&get(ReviewSource::Local, &task))
        .await
        .is_ok());
    assert_eq!(
        log_lines(&env, "review get").len(),
        forge_calls,
        "Local mode called the forge"
    );
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

fn log_lines(env: &Env, prefix: &str) -> Vec<Value> {
    std::fs::read_to_string(&env.log)
        .unwrap_or_default()
        .lines()
        .filter_map(|l| l.strip_prefix(prefix))
        .map(|l| serde_json::from_str(l.split_once(' ').map_or(l, |(_, j)| j)).unwrap())
        .collect()
}

fn comment(
    task: &Task,
    source: ReviewSource,
    target: CommentTarget,
    body: &str,
) -> ReviewCommentParams {
    ReviewCommentParams {
        task_id: task.id,
        source,
        path: "a.rs".into(),
        line: 1,
        side: DiffSide::New,
        body: body.into(),
        target,
    }
}

#[tokio::test]
async fn local_comments_thread_resolve_and_reach_the_prompt() {
    let env = setup();
    let task = task(&env).await;
    commit(&task, "one\n");
    let d = &env.daemon;
    d.review_comment(&comment(
        &task,
        ReviewSource::Local,
        CommentTarget::Local,
        "Add a test.",
    ))
    .await
    .unwrap();
    let r = d
        .review_get(&get(ReviewSource::Local, &task))
        .await
        .unwrap();
    let thread = r.threads[0].id.clone();
    d.review_reply(&ReviewReplyParams {
        task_id: task.id,
        thread_id: thread.clone(),
        body: "and docs".into(),
    })
    .await
    .unwrap();
    let prompt = d
        .review_prompt(&ReviewPromptParams {
            task_id: task.id,
            source: ReviewSource::Local,
            thread_ids: None,
            include_resolved: false,
        })
        .await
        .unwrap()
        .prompt;
    assert_eq!(prompt, "Review comments on your changes:\n\na.rs:1\n> one\n- (local) Add a test.\n- (local) and docs\n");
    d.review_resolve(&ReviewResolveParams {
        task_id: task.id,
        thread_id: thread,
        resolved: true,
    })
    .await
    .unwrap();
    let prompt = d
        .review_prompt(&ReviewPromptParams {
            task_id: task.id,
            source: ReviewSource::Local,
            thread_ids: None,
            include_resolved: false,
        })
        .await
        .unwrap()
        .prompt;
    assert_eq!(prompt, "Review comments on your changes:\n");
}

#[tokio::test]
async fn forge_comments_go_to_the_plugin_and_refresh_the_cache() {
    let env = setup();
    let task = task(&env).await;
    let head = commit(&task, "one\n");
    with_pr(&env, &task, &head).await;
    let d = &env.daemon;
    let mut events = d.subscribe();
    d.review_get(&get(ReviewSource::Pr, &task)).await.unwrap();
    d.review_comment(&comment(
        &task,
        ReviewSource::Pr,
        CommentTarget::Review,
        "pending one",
    ))
    .await
    .unwrap();
    let r = d.review_get(&get(ReviewSource::Pr, &task)).await.unwrap();
    assert!(
        r.threads[0].pending && r.pending_review.is_some(),
        "cache was dropped after the mutation"
    );
    assert!(
        matches!(events.try_recv(), Ok(Event::ReviewChanged { task_id }) if task_id == task.id)
    );
    d.review_resolve(&ReviewResolveParams {
        task_id: task.id,
        thread_id: r.threads[0].id.clone(),
        resolved: true,
    })
    .await
    .unwrap();
    d.review_set_viewed(&ReviewViewedParams {
        task_id: task.id,
        source: ReviewSource::Pr,
        path: "a.rs".into(),
        viewed: true,
    })
    .await
    .unwrap();
    d.review_submit(&ReviewSubmitParams {
        task_id: task.id,
        event: ReviewEvent::Approve,
        body: "ship".into(),
    })
    .await
    .unwrap();
    let r = d.review_get(&get(ReviewSource::Pr, &task)).await.unwrap();
    assert!(r.threads[0].resolved && !r.threads[0].pending && r.pending_review.is_none());
    assert_eq!(r.viewed_files, vec!["a.rs".to_string()]);
    let submitted = log_lines(&env, "review submit");
    assert_eq!(submitted[0]["event"], "approve");
    assert_eq!(submitted[0]["number"], 7);
}

#[tokio::test]
async fn publish_hides_the_local_thread_and_failure_keeps_it() {
    let env = setup();
    let task = task(&env).await;
    let head = commit(&task, "one\n");
    with_pr(&env, &task, &head).await;
    let d = &env.daemon;
    d.review_comment(&comment(
        &task,
        ReviewSource::Pr,
        CommentTarget::Local,
        "local first",
    ))
    .await
    .unwrap();
    let local = d
        .review_get(&get(ReviewSource::Pr, &task))
        .await
        .unwrap()
        .threads
        .into_iter()
        .find(|t| t.local)
        .unwrap();
    let saved = std::fs::read_to_string(&env.review).unwrap();
    std::fs::write(&env.review, json!({"error": "boom"}).to_string()).unwrap();
    let publish = ReviewPublishParams {
        task_id: task.id,
        thread_id: local.id.clone(),
        target: CommentTarget::Single,
    };
    assert!(d.review_publish(&publish).await.is_err());
    std::fs::write(&env.review, saved).unwrap();
    assert!(d
        .review_get(&get(ReviewSource::Local, &task))
        .await
        .unwrap()
        .threads
        .iter()
        .any(|t| t.id == local.id));
    d.review_publish(&publish).await.unwrap();
    let r = d.review_get(&get(ReviewSource::Pr, &task)).await.unwrap();
    assert!(!r.threads.iter().any(|t| t.local));
    assert!(r
        .threads
        .iter()
        .any(|t| t.comments[0].body == "local first"));
}

#[tokio::test]
async fn forge_targets_without_a_pull_request_are_rejected() {
    let env = setup();
    let task = task(&env).await;
    commit(&task, "one\n");
    let err = env
        .daemon
        .review_comment(&comment(
            &task,
            ReviewSource::Local,
            CommentTarget::Single,
            "x",
        ))
        .await
        .unwrap_err();
    assert!(err.message.contains("pull request"), "{}", err.message);
}

#[tokio::test]
async fn published_local_threads_cannot_be_published_replied_or_resolved_again() {
    let env = setup();
    let task = task(&env).await;
    let head = commit(&task, "one\n");
    with_pr(&env, &task, &head).await;
    let d = &env.daemon;
    d.review_comment(&comment(
        &task,
        ReviewSource::Pr,
        CommentTarget::Local,
        "local first",
    ))
    .await
    .unwrap();
    let local = d
        .review_get(&get(ReviewSource::Local, &task))
        .await
        .unwrap()
        .threads[0]
        .id
        .clone();
    let publish = ReviewPublishParams {
        task_id: task.id,
        thread_id: local.clone(),
        target: CommentTarget::Single,
    };
    d.review_publish(&publish).await.unwrap();
    let again = d.review_publish(&publish).await.unwrap_err();
    assert_eq!(again.kind, ErrorKind::InvalidParams, "{}", again.message);
    assert_eq!(
        log_lines(&env, "review comment").len(),
        1,
        "published twice"
    );
    let reply = ReviewReplyParams {
        task_id: task.id,
        thread_id: local.clone(),
        body: "more".into(),
    };
    assert_eq!(
        d.review_reply(&reply).await.unwrap_err().kind,
        ErrorKind::InvalidParams
    );
    let resolve = ReviewResolveParams {
        task_id: task.id,
        thread_id: local,
        resolved: true,
    };
    assert_eq!(
        d.review_resolve(&resolve).await.unwrap_err().kind,
        ErrorKind::InvalidParams
    );
    assert!(d
        .review_get(&get(ReviewSource::Local, &task))
        .await
        .unwrap()
        .threads
        .is_empty());
}

#[tokio::test]
async fn resolving_an_unknown_local_thread_is_not_found() {
    let env = setup();
    let task = task(&env).await;
    let resolve = ReviewResolveParams {
        task_id: task.id,
        thread_id: "local-999".into(),
        resolved: true,
    };
    assert_eq!(
        env.daemon.review_resolve(&resolve).await.unwrap_err().kind,
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn publishing_emits_one_change() {
    let env = setup();
    let task = task(&env).await;
    let head = commit(&task, "one\n");
    with_pr(&env, &task, &head).await;
    let d = &env.daemon;
    d.review_comment(&comment(&task, ReviewSource::Pr, CommentTarget::Local, "x"))
        .await
        .unwrap();
    let thread_id = d
        .review_get(&get(ReviewSource::Local, &task))
        .await
        .unwrap()
        .threads[0]
        .id
        .clone();
    let mut events = d.subscribe();
    d.review_publish(&ReviewPublishParams {
        task_id: task.id,
        thread_id,
        target: CommentTarget::Single,
    })
    .await
    .unwrap();
    let changes = std::iter::from_fn(|| events.try_recv().ok())
        .filter(|e| matches!(e, Event::ReviewChanged { .. }))
        .count();
    assert_eq!(changes, 1);
}
