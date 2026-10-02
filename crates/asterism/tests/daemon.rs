mod common;

use std::path::Path;
use std::sync::Arc;

use asterism_core::daemon::Daemon;
use asterism_core::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::*;
use common::{eventually, init_repo, run_git};
use tempfile::TempDir;

struct Env {
    home: TempDir,
    repo: TempDir,
    daemon: Arc<Daemon>,
}

fn setup() -> Env {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let daemon = Daemon::new(Paths { home: home.path().to_path_buf() }).unwrap();
    Env { home, repo, daemon }
}

fn sh(script: &str) -> SessionKind {
    SessionKind::Command { argv: vec!["sh".into(), "-c".into(), script.into()] }
}

fn new_task(env: &Env, title: &str) -> Task {
    let project = env.daemon.add_project(&env.repo.path().display().to_string()).unwrap();
    let params = TaskCreateParams { project_id: project.id, title: title.into(), prompt: None, agent: None };
    env.daemon.create_task(params).unwrap().task
}

fn start(env: &Env, task: &Task, kind: SessionKind) -> Session {
    env.daemon.start_session(SessionStartParams { task_id: task.id, kind, prompt: None }).unwrap()
}

#[tokio::test]
async fn projects_are_idempotent_and_must_be_repos() {
    let env = setup();
    let path = env.repo.path().display().to_string();
    let a = env.daemon.add_project(&path).unwrap();
    let b = env.daemon.add_project(&path).unwrap();
    assert_eq!(a, b);

    let plain = tempfile::tempdir().unwrap();
    let err = env.daemon.add_project(&plain.path().display().to_string()).unwrap_err();
    assert_eq!(err.kind, ErrorKind::NotARepo);
}

#[tokio::test]
async fn create_task_makes_branch_and_worktree() {
    let env = setup();
    let task = new_task(&env, "Fix login");
    assert_eq!(task.branch, "asterism/1-fix-login");
    assert_eq!(task.base_branch, "main");
    assert!(Path::new(&task.worktree_path).join("README.md").exists());
    assert!(Path::new(&task.worktree_path).starts_with(env.home.path().join("worktrees")));
    run_git(env.repo.path(), &["rev-parse", "--verify", "asterism/1-fix-login"]);
}

#[tokio::test]
async fn unknown_agent_fails_before_creating_anything() {
    let env = setup();
    let project = env.daemon.add_project(&env.repo.path().display().to_string()).unwrap();
    let params = TaskCreateParams { project_id: project.id, title: "t".into(), prompt: None, agent: Some("nope".into()) };
    assert_eq!(env.daemon.create_task(params).unwrap_err().kind, ErrorKind::AgentUnavailable);
    assert!(env.daemon.tasks(TaskListParams::default()).unwrap().is_empty());
}

#[tokio::test]
async fn session_send_read_wait_and_kill() {
    let env = setup();
    let task = new_task(&env, "shell work");
    let session = start(&env, &task, sh("echo ready; cat"));
    assert_eq!(session.status, SessionStatus::Working);

    let idle = env.daemon
        .wait(SessionWaitParams { session_id: session.id, until: SessionStatus::Idle, timeout_ms: Some(10_000) })
        .await
        .unwrap();
    assert_eq!(idle, SessionStatus::Idle);

    env.daemon.send(SessionSendParams { session_id: session.id, text: "ping\n".into(), submit: false }).await.unwrap();
    let read = |d: &Daemon| d.read(SessionReadParams { session_id: session.id, lines: 5 }).unwrap().text;
    assert!(eventually(|| read(&env.daemon).contains("ping")).await);

    env.daemon.kill_session(session.id).unwrap();
    let status = env.daemon
        .wait(SessionWaitParams { session_id: session.id, until: SessionStatus::Exited, timeout_ms: Some(5_000) })
        .await
        .unwrap();
    assert_eq!(status, SessionStatus::Exited);
    assert!(eventually(|| env.daemon.session(session.id).unwrap().status == SessionStatus::Exited).await);
    assert_eq!(env.daemon.kill_session(session.id).unwrap_err().kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn wait_times_out_while_busy() {
    let env = setup();
    let task = new_task(&env, "busy");
    let session = start(&env, &task, sh("while true; do echo x; sleep 0.2; done"));
    let err = env.daemon
        .wait(SessionWaitParams { session_id: session.id, until: SessionStatus::Idle, timeout_ms: Some(1_000) })
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout);
}

#[tokio::test]
async fn hooks_override_the_heuristic() {
    let env = setup();
    let task = new_task(&env, "hooked");
    let session = start(&env, &task, sh("echo x; sleep 30"));
    env.daemon
        .hook(SessionHookParams { session_id: session.id, event: HookEvent::Notification, agent_ref: Some("abc".into()) })
        .unwrap();
    let err = env.daemon
        .wait(SessionWaitParams { session_id: session.id, until: SessionStatus::Idle, timeout_ms: Some(3_000) })
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout);
    assert!(eventually(|| env.daemon.session(session.id).unwrap().status == SessionStatus::WaitingInput).await);
}

#[tokio::test]
async fn archive_refuses_dirty_worktrees_unless_forced() {
    let env = setup();
    let task = new_task(&env, "dirty");
    std::fs::write(Path::new(&task.worktree_path).join("scratch.txt"), "wip").unwrap();
    assert_eq!(env.daemon.archive_task(task.id, false).unwrap_err().kind, ErrorKind::DirtyWorktree);

    let archived = env.daemon.archive_task(task.id, true).unwrap();
    assert!(archived.archived);
    assert!(!Path::new(&task.worktree_path).exists());
    run_git(env.repo.path(), &["rev-parse", "--verify", &task.branch]);
}

#[tokio::test]
async fn diff_shows_worktree_changes() {
    let env = setup();
    let task = new_task(&env, "diff me");
    std::fs::write(Path::new(&task.worktree_path).join("added.txt"), "new line\n").unwrap();
    let patch = env.daemon.diff(task.id).unwrap().patch;
    assert!(patch.contains("+new line"), "{patch}");
}

#[tokio::test]
async fn changes_are_published_as_events() {
    let env = setup();
    let mut events = env.daemon.subscribe();
    let task = new_task(&env, "evented");
    let mut saw_task = false;
    while let Ok(event) = events.try_recv() {
        saw_task |= matches!(event, Event::TaskChanged(t) if t.id == task.id);
    }
    assert!(saw_task);
}

#[tokio::test]
async fn project_with_active_tasks_cannot_be_removed() {
    let env = setup();
    let task = new_task(&env, "keep");
    assert_eq!(env.daemon.remove_project(task.project_id).unwrap_err().kind, ErrorKind::InvalidParams);
    env.daemon.archive_task(task.id, true).unwrap();
    env.daemon.remove_project(task.project_id).unwrap();
}

#[tokio::test]
async fn task_ids_are_not_reused_after_project_removal() {
    let env = setup();
    let first = new_task(&env, "again");
    env.daemon.archive_task(first.id, true).unwrap();
    env.daemon.remove_project(first.project_id).unwrap();
    let second = new_task(&env, "again");
    assert_ne!(first.branch, second.branch);
}

#[tokio::test]
async fn restart_marks_non_resumable_sessions_exited() {
    let env = setup();
    let task = new_task(&env, "restart");
    let session = start(&env, &task, sh("sleep 30"));
    assert_eq!(env.daemon.session(session.id).unwrap().status, SessionStatus::Working);

    let restarted = Daemon::new(Paths { home: env.home.path().to_path_buf() }).unwrap();
    restarted.recover().unwrap();
    assert_eq!(restarted.session(session.id).unwrap().status, SessionStatus::Exited);
    env.daemon.kill_session(session.id).unwrap();
}

#[tokio::test]
async fn zero_sized_resize_is_rejected() {
    let env = setup();
    let task = new_task(&env, "resize");
    let session = start(&env, &task, sh("echo alive; cat"));
    let resize = |rows, cols| env.daemon.resize(SessionResizeParams { session_id: session.id, rows, cols });
    assert_eq!(resize(0, 80).unwrap_err().kind, ErrorKind::InvalidParams);
    assert_eq!(resize(24, 0).unwrap_err().kind, ErrorKind::InvalidParams);
    let read = || env.daemon.read(SessionReadParams { session_id: session.id, lines: 5 }).unwrap().text;
    assert!(eventually(|| read().contains("alive")).await);
}

#[tokio::test]
async fn read_includes_scrollback() {
    let env = setup();
    let task = new_task(&env, "scroll");
    let session = start(&env, &task, sh("read go; i=1; while [ $i -le 100 ]; do echo line$i; i=$((i+1)); done; cat"));
    env.daemon.resize(SessionResizeParams { session_id: session.id, rows: 24, cols: 80 }).unwrap();
    env.daemon.send(SessionSendParams { session_id: session.id, text: "go".into(), submit: true }).await.unwrap();
    let read = || env.daemon.read(SessionReadParams { session_id: session.id, lines: 60 }).unwrap().text;
    assert!(eventually(|| read().contains("line100")).await);
    let text = read();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), 60, "{text}");
    assert_eq!(lines[0], "line41");
    assert_eq!(lines[59], "line100");
}

#[tokio::test]
async fn exited_sessions_stay_readable() {
    let env = setup();
    let task = new_task(&env, "exited");
    let session = start(&env, &task, sh("echo done-marker"));
    let status = env.daemon
        .wait(SessionWaitParams { session_id: session.id, until: SessionStatus::Exited, timeout_ms: Some(5_000) })
        .await
        .unwrap();
    assert_eq!(status, SessionStatus::Exited);
    let read = || env.daemon.read(SessionReadParams { session_id: session.id, lines: 10 });
    assert!(eventually(|| read().is_ok_and(|r| r.text.contains("done-marker"))).await);
    let missing = env.daemon.read(SessionReadParams { session_id: session.id + 100, lines: 10 }).unwrap_err();
    assert_eq!(missing.kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn submit_marks_hooked_sessions_working() {
    let env = setup();
    let task = new_task(&env, "submit");
    let session = start(&env, &task, sh("cat"));
    env.daemon.hook(SessionHookParams { session_id: session.id, event: HookEvent::Stop, agent_ref: None }).unwrap();
    env.daemon.send(SessionSendParams { session_id: session.id, text: "hi".into(), submit: true }).await.unwrap();
    let err = env.daemon
        .wait(SessionWaitParams { session_id: session.id, until: SessionStatus::Idle, timeout_ms: Some(1_500) })
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::Timeout);
}
