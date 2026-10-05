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
    let daemon = Daemon::with_options(Paths { home: home.path().to_path_buf() }, common::daemon_options()).unwrap();
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
async fn archive_keeps_worktree_changes_and_restore_brings_the_task_back() {
    let env = setup();
    let task = new_task(&env, "dirty");
    let scratch = Path::new(&task.worktree_path).join("scratch.txt");
    std::fs::write(&scratch, "wip").unwrap();

    let archived = env.daemon.archive_task(task.id).unwrap();
    assert!(archived.archived);
    assert_eq!(std::fs::read_to_string(&scratch).unwrap(), "wip");

    let restored = env.daemon.restore_task(task.id).unwrap();
    assert!(!restored.archived);
    assert_eq!(std::fs::read_to_string(&scratch).unwrap(), "wip");
}

#[tokio::test]
async fn restore_recreates_a_removed_worktree_from_its_branch() {
    let env = setup();
    let task = new_task(&env, "legacy");
    env.daemon.archive_task(task.id).unwrap();
    run_git(env.repo.path(), &["worktree", "remove", "--force", &task.worktree_path]);

    env.daemon.restore_task(task.id).unwrap();
    assert!(Path::new(&task.worktree_path).join("README.md").exists());
}

#[tokio::test]
async fn restore_fails_cleanly_without_the_branch() {
    let env = setup();
    let task = new_task(&env, "gone");
    env.daemon.archive_task(task.id).unwrap();
    run_git(env.repo.path(), &["worktree", "remove", "--force", &task.worktree_path]);
    run_git(env.repo.path(), &["branch", "-D", &task.branch]);

    assert_eq!(env.daemon.restore_task(task.id).unwrap_err().kind, ErrorKind::NotFound);
    assert!(env.daemon.task(task.id).unwrap().archived);
}

#[tokio::test]
async fn delete_check_reports_dirty_and_unmerged_work() {
    let env = setup();
    let task = new_task(&env, "check");
    let clean = env.daemon.delete_check(task.id).unwrap();
    assert_eq!(clean, TaskDeleteCheck { dirty: false, branch: task.branch.clone(), branch_exists: true, unmerged_commits: 0 });

    let wt = Path::new(&task.worktree_path);
    std::fs::write(wt.join("new.txt"), "x").unwrap();
    run_git(wt, &["add", "."]);
    run_git(wt, &["commit", "-qm", "work"]);
    std::fs::write(wt.join("more.txt"), "y").unwrap();
    let busy = env.daemon.delete_check(task.id).unwrap();
    assert!(busy.dirty);
    assert_eq!(busy.unmerged_commits, 1);
}

#[tokio::test(flavor = "multi_thread")]
async fn delete_removes_sessions_worktree_and_optionally_the_branch() {
    let env = setup();
    let keep = new_task(&env, "keep branch");
    let session = start(&env, &keep, sh("sleep 30"));
    let mut events = env.daemon.subscribe();
    assert_eq!(env.daemon.delete_task(keep.id, false).unwrap(), TaskDeleteResult { warning: None });
    assert!(!Path::new(&keep.worktree_path).exists());
    run_git(env.repo.path(), &["rev-parse", "--verify", &keep.branch]);
    assert_eq!(env.daemon.task(keep.id).unwrap_err().kind, ErrorKind::NotFound);
    assert!(env.daemon.sessions(Some(keep.id)).unwrap().is_empty());
    let mut saw = (false, false);
    while let Ok(event) = events.try_recv() {
        saw.0 |= matches!(event, Event::SessionRemoved { session_id } if session_id == session.id);
        saw.1 |= matches!(event, Event::TaskRemoved { task_id } if task_id == keep.id);
    }
    assert_eq!(saw, (true, true));

    let drop = new_task(&env, "drop branch");
    env.daemon.delete_task(drop.id, true).unwrap();
    let out = std::process::Command::new("git")
        .args(["-C", &env.repo.path().display().to_string(), "rev-parse", "--verify", "--quiet", &drop.branch])
        .output()
        .unwrap();
    assert!(!out.status.success());
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
    env.daemon.archive_task(task.id).unwrap();
    env.daemon.remove_project(task.project_id).unwrap();
}

#[tokio::test]
async fn task_ids_are_not_reused_after_project_removal() {
    let env = setup();
    let first = new_task(&env, "again");
    env.daemon.archive_task(first.id).unwrap();
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

    let restarted = Daemon::with_options(Paths { home: env.home.path().to_path_buf() }, common::daemon_options()).unwrap();
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

#[tokio::test]
async fn agent_env_policy_from_node_config_applies_to_sessions() {
    let env = setup();
    std::fs::write(
        env.home.path().join("config.toml"),
        "[agents.command.env]\nremove = [\"HOME\"]\nset = { FOO = \"bar\", ANTHROPIC_BASE_URL = \"http://proxy\" }\n",
    )
    .unwrap();
    let task = new_task(&env, "env policy");
    let session = start(&env, &task, sh("echo \"foo=$FOO home=[$HOME] base=$ANTHROPIC_BASE_URL task=$ASTERISM_TASK\"; sleep 30"));
    let read = || env.daemon.read(SessionReadParams { session_id: session.id, lines: 5 }).unwrap().text;
    let expected = format!("foo=bar home=[] base=http://proxy task={}", task.id);
    assert!(eventually(|| read().contains(&expected)).await, "{}", read());
    env.daemon.kill_session(session.id).unwrap();
}

#[tokio::test]
async fn broken_node_config_fails_session_start_with_a_clear_error() {
    let env = setup();
    let task = new_task(&env, "broken config");
    std::fs::write(env.home.path().join("config.toml"), "[agents.command\n").unwrap();
    let err = env.daemon
        .start_session(SessionStartParams { task_id: task.id, kind: sh("true"), prompt: None })
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    assert!(err.message.contains("config.toml"), "{}", err.message);
}

#[tokio::test]
async fn agent_config_roundtrips_through_the_daemon() {
    let env = setup();
    let config = AgentConfig {
        args: vec!["--model".into(), "opus".into()],
        hooks: Some(serde_json::json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]}})),
        ..Default::default()
    };
    env.daemon.set_agent_config("claude", &config).unwrap();
    assert_eq!(env.daemon.agent_config("claude").unwrap(), config);
    let merged = std::fs::read_to_string(env.home.path().join("claude-settings.json")).unwrap();
    assert!(merged.contains("say done") && merged.contains("hook stop"), "{merged}");
    assert_eq!(
        env.daemon.set_agent_config("shell", &config).unwrap_err().kind,
        ErrorKind::InvalidParams
    );
}

#[tokio::test]
async fn shell_env_settings_reach_new_sessions() {
    let env = setup();
    let config = AgentConfig {
        env: EnvSettings { set: [("FROM_SETTINGS".to_string(), "yes".to_string())].into(), ..Default::default() },
        ..Default::default()
    };
    env.daemon.set_agent_config("command", &config).unwrap();
    let task = new_task(&env, "settings env");
    let session = start(&env, &task, sh("echo \"value=$FROM_SETTINGS\"; sleep 30"));
    let read = || env.daemon.read(SessionReadParams { session_id: session.id, lines: 5 }).unwrap().text;
    assert!(eventually(|| read().contains("value=yes")).await, "{}", read());
    env.daemon.remove_session(session.id).await.unwrap();
}

#[tokio::test]
async fn remove_session_stops_live_sessions_and_deletes_them() {
    let env = setup();
    let task = new_task(&env, "remove");
    let live = start(&env, &task, sh("sleep 30"));
    let mut events = env.daemon.subscribe();
    env.daemon.remove_session(live.id).await.unwrap();
    assert_eq!(env.daemon.session(live.id).unwrap_err().kind, ErrorKind::NotFound);
    // Give the status forwarder time to (wrongly) report the exit.
    tokio::time::sleep(std::time::Duration::from_millis(300)).await;
    let mut removed = false;
    while let Ok(event) = events.try_recv() {
        assert!(!matches!(event, Event::SessionStatusChanged { session_id, .. } if session_id == live.id), "{event:?}");
        removed |= matches!(event, Event::SessionRemoved { session_id } if session_id == live.id);
    }
    assert!(removed);

    let exited = start(&env, &task, sh("exit 0"));
    env.daemon
        .wait(SessionWaitParams { session_id: exited.id, until: SessionStatus::Exited, timeout_ms: Some(5_000) })
        .await
        .unwrap();
    env.daemon.remove_session(exited.id).await.unwrap();
    assert!(env.daemon.sessions(Some(task.id)).unwrap().is_empty());
    assert_eq!(env.daemon.remove_session(9_999).await.unwrap_err().kind, ErrorKind::NotFound);
}

#[tokio::test]
async fn remove_force_kills_stubborn_sessions() {
    let env = setup();
    let task = new_task(&env, "stubborn");
    let session = start(&env, &task, sh("trap '' HUP TERM INT; echo pid=$$; while true; do sleep 1; done"));
    let read = || env.daemon.read(SessionReadParams { session_id: session.id, lines: 5 }).unwrap().text;
    assert!(eventually(|| read().contains("pid=")).await);
    let text = read();
    let pid = text.split("pid=").nth(1).unwrap().split_whitespace().next().unwrap().to_string();
    let alive = || std::process::Command::new("kill").args(["-0", &pid]).output().unwrap().status.success();
    assert!(alive());
    let started = std::time::Instant::now();
    env.daemon.remove_session(session.id).await.unwrap();
    assert!(started.elapsed() < std::time::Duration::from_secs(5));
    assert_eq!(env.daemon.session(session.id).unwrap_err().kind, ErrorKind::NotFound);
    assert!(eventually(|| !alive()).await, "process {pid} survived remove");
}

#[tokio::test(flavor = "multi_thread")]
async fn stats_cover_running_sessions_and_requested_pids() {
    let env = setup();
    let task = new_task(&env, "Stats");
    let session = start(&env, &task, sh("sleep 30"));
    let me = std::process::id();
    let stats = env.daemon.stats(NodeStatsParams { pids: vec![me] });
    assert!(stats.daemon.memory_bytes > 0);
    let entry = stats.sessions.iter().find(|s| s.session_id == session.id).expect("running session is measured");
    assert!(entry.stats.memory_bytes > 0);
    assert_eq!(stats.processes.len(), 1);
    assert_eq!(stats.processes[0].pid, me);
    env.daemon.kill_session(session.id).ok();
}

#[tokio::test]
async fn worktrees_cover_main_linked_and_foreign_checkouts() {
    let env = setup();
    let project = env.daemon.add_project(&env.repo.path().display().to_string()).unwrap();
    let task = new_task(&env, "listed");
    let foreign = tempfile::tempdir().unwrap();
    let foreign_path = foreign.path().join("hand-made");
    run_git(env.repo.path(), &["worktree", "add", "-q", "-b", "hand", &foreign_path.display().to_string()]);

    let list = env.daemon.project_worktrees(project.id).unwrap();
    assert_eq!(list.len(), 3);
    assert!(list[0].is_main);
    let linked = list.iter().find(|w| w.task_id == Some(task.id)).expect("task worktree is linked despite symlinked tmp paths");
    assert_eq!(linked.base_branch.as_deref(), Some(task.base_branch.as_str()));
    let hand = list.iter().find(|w| w.branch.as_deref() == Some("hand")).unwrap();
    assert_eq!((hand.task_id, hand.base_branch.as_ref()), (None, None));

    let sizes = env.daemon.project_worktree_sizes(project.id).unwrap();
    assert_eq!(sizes.len(), 3);
    assert!(sizes.iter().all(|s| s.bytes > 0));
}

#[tokio::test]
async fn worktree_remove_only_takes_unlinked_listed_worktrees() {
    let env = setup();
    let project = env.daemon.add_project(&env.repo.path().display().to_string()).unwrap();
    let task = new_task(&env, "linked");
    let foreign = tempfile::tempdir().unwrap();
    let foreign_path = foreign.path().join("hand-made");
    run_git(env.repo.path(), &["worktree", "add", "-q", "-b", "hand", &foreign_path.display().to_string()]);
    let list = env.daemon.project_worktrees(project.id).unwrap();
    let main = list.iter().find(|w| w.is_main).unwrap().path.clone();
    let hand = list.iter().find(|w| w.branch.as_deref() == Some("hand")).unwrap().path.clone();

    for refused in [main, task.worktree_path.clone(), "/not/a/worktree".to_string()] {
        assert_eq!(env.daemon.remove_worktree(project.id, &refused).unwrap_err().kind, ErrorKind::InvalidParams, "{refused}");
    }
    env.daemon.remove_worktree(project.id, &hand).unwrap();
    assert!(!foreign_path.exists());
    env.daemon.prune_worktrees(project.id).unwrap();
}

#[tokio::test]
async fn restore_refuses_an_occupied_worktree_path() {
    let env = setup();
    let task = new_task(&env, "occupied");
    env.daemon.archive_task(task.id).unwrap();
    run_git(env.repo.path(), &["worktree", "remove", "--force", &task.worktree_path]);
    std::fs::create_dir_all(&task.worktree_path).unwrap();

    let err = env.daemon.restore_task(task.id).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    assert!(err.message.contains(&task.worktree_path), "{}", err.message);
    assert!(env.daemon.task(task.id).unwrap().archived);
}
