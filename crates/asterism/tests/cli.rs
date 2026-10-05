mod common;

use std::io::Write;
use std::path::Path;
use std::process::Stdio;

use common::Node;

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn task_new_autostarts_the_daemon_and_creates_a_worktree() {
    let node = Node::new();
    let created = node.json(&["task", "new", "Fix login"]);
    assert_eq!(created["task"]["branch"], "asterism/1-fix-login");
    assert!(Path::new(created["task"]["worktree_path"].as_str().unwrap()).join("README.md").exists());
    assert!(node.home().join("asterismd.sock").exists());

    let status = node.json(&["daemon", "status"]);
    assert_eq!(status["proto_version"], 1);
}

#[test]
fn session_send_read_wait_kill() {
    let node = Node::new();
    let task = node.json(&["task", "new", "shell"])["task"]["id"].to_string();
    let session = node.json(&["session", "start", &task, "--", "sh", "-c", "echo ready; cat"])["id"].to_string();

    let idle = node.json(&["wait", &session, "--until", "idle", "--timeout", "10s"]);
    assert_eq!(idle["status"], "idle");

    let out = node.cmd(&["send", &session, "ping\n", "--no-submit"]);
    assert!(out.status.success(), "{}", stderr(&out));
    node.json(&["wait", &session, "--until", "idle", "--timeout", "10s"]);
    let read = node.json(&["read", &session, "--lines", "5"]);
    assert!(read["text"].as_str().unwrap().contains("ping"), "{read}");

    assert!(node.cmd(&["session", "kill", &session]).status.success());
    let exited = node.json(&["wait", &session, "--until", "exited", "--timeout", "5s"]);
    assert_eq!(exited["status"], "exited");
}

#[test]
fn wait_timeout_is_an_error() {
    let node = Node::new();
    let task = node.json(&["task", "new", "busy"])["task"]["id"].to_string();
    let session = node.json(&["session", "start", &task, "--", "sh", "-c", "while true; do echo x; sleep 0.2; done"])
        ["id"]
        .to_string();
    let out = node.cmd(&["wait", &session, "--until", "idle", "--timeout", "1s"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(stderr(&out).contains("Timeout"), "{}", stderr(&out));
}

#[test]
fn archive_restore_and_delete_from_the_cli() {
    let node = Node::new();
    let created = node.json(&["task", "new", "lifecycle"]);
    let id = created["task"]["id"].to_string();
    let worktree = created["task"]["worktree_path"].as_str().unwrap().to_string();
    std::fs::write(Path::new(&worktree).join("wip.txt"), "x").unwrap();

    assert_eq!(node.json(&["task", "archive", &id])["archived"], true);
    assert!(Path::new(&worktree).join("wip.txt").exists());
    assert_eq!(node.json(&["task", "restore", &id])["archived"], false);
    node.json(&["task", "delete", &id, "--delete-branch"]);
    assert!(!Path::new(&worktree).exists());
}

#[test]
fn hook_reports_status_and_is_silent_outside_sessions() {
    let node = Node::new();
    let task = node.json(&["task", "new", "hooked"])["task"]["id"].to_string();
    let session = node.json(&["session", "start", &task, "--", "sh", "-c", "sleep 30"])["id"].to_string();

    let outside = node.command(&["hook", "stop"]).stdin(Stdio::null()).output().unwrap();
    assert!(outside.status.success());
    assert!(outside.stdout.is_empty() && outside.stderr.is_empty());

    let mut child = node
        .command(&["hook", "notification"])
        .env("ASTERISM_SESSION", &session)
        .stdin(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(br#"{"session_id":"claude-abc"}"#).unwrap();
    assert!(child.wait().unwrap().success());

    let waiting = node.json(&["wait", &session, "--until", "waiting_input", "--timeout", "5s"]);
    assert_eq!(waiting["status"], "waiting_input");
    node.cmd(&["session", "kill", &session]);
}

#[test]
fn hook_with_daemon_down_still_exits_cleanly() {
    let home = tempfile::tempdir().unwrap();
    let out = std::process::Command::new(env!("CARGO_BIN_EXE_asterism"))
        .args(["hook", "stop"])
        .env("ASTERISM_HOME", home.path())
        .env("ASTERISM_SESSION", "1")
        .env_remove("ASTERISM_SOCKET")
        .stdin(Stdio::null())
        .output()
        .unwrap();
    assert!(out.status.success());
    assert!(!home.path().join("asterismd.sock").exists());
}

#[test]
fn project_flag_accepts_id_or_name() {
    let node = Node::new();
    let project = node.json(&["project", "add"]);
    let name = project["name"].as_str().unwrap();
    let by_name = node.json(&["task", "new", "named", "--project", name]);
    assert_eq!(by_name["task"]["project_id"], project["id"]);

    let out = node.cmd(&["task", "new", "x", "--project", "does-not-exist"]);
    assert_eq!(out.status.code(), Some(1));
}

#[test]
fn json_mode_is_parseable_for_commands_without_a_result() {
    let node = Node::new();
    let task = node.json(&["task", "new", "k"])["task"]["id"].to_string();
    let session = node.json(&["session", "start", &task, "--", "sh", "-c", "sleep 30"])["id"].to_string();
    assert_eq!(node.json(&["session", "kill", &session]), serde_json::json!({"ok": true}));
    assert_eq!(node.json(&["daemon", "stop"]), serde_json::json!({"ok": true}));
}

#[test]
fn daemon_stop_does_not_autostart() {
    let node = Node::new();
    let out = node.cmd(&["daemon", "stop"]);
    assert!(out.status.success(), "{}", stderr(&out));
    assert!(!node.home().join("asterismd.sock").exists());
}

#[test]
fn hook_with_invalid_event_exits_zero_silently() {
    let node = Node::new();
    let out = node.command(&["hook", "bogus-event"]).stdin(Stdio::null()).output().unwrap();
    assert!(out.status.success());
    assert!(out.stdout.is_empty() && out.stderr.is_empty());
}

#[test]
fn hook_with_hung_stdin_returns_promptly() {
    let node = Node::new();
    let started = std::time::Instant::now();
    let mut child = node.command(&["hook", "stop"]).env("ASTERISM_SESSION", "1").stdin(Stdio::piped()).spawn().unwrap();
    let _held_open = child.stdin.take();
    assert!(child.wait().unwrap().success());
    assert!(started.elapsed() < std::time::Duration::from_secs(3));
}

#[test]
fn idle_prompt_notification_means_idle() {
    let node = Node::new();
    let task = node.json(&["task", "new", "idle prompt"])["task"]["id"].to_string();
    let session = node.json(&["session", "start", &task, "--", "sh", "-c", "sleep 30"])["id"].to_string();
    let hook = |event: &str, payload: &[u8]| {
        let mut child =
            node.command(&["hook", event]).env("ASTERISM_SESSION", &session).stdin(Stdio::piped()).spawn().unwrap();
        child.stdin.take().unwrap().write_all(payload).unwrap();
        assert!(child.wait().unwrap().success());
    };
    hook("prompt-submit", br#"{"session_id":"x"}"#);
    let working = (0..40).any(|_| {
        std::thread::sleep(std::time::Duration::from_millis(50));
        node.json(&["session", "list", "--task", &task])[0]["status"] == "working"
    });
    assert!(working);

    hook("notification", br#"{"session_id":"x","message":"Claude is waiting for your input"}"#);
    let idle = node.json(&["wait", &session, "--until", "idle", "--timeout", "5s"]);
    assert_eq!(idle["status"], "idle");
    node.cmd(&["session", "kill", &session]);
}

#[test]
fn autostarted_daemon_drops_the_callers_claude_env() {
    let node = Node::new();
    let created = node.command(&["--json", "task", "new", "env"]).env("CLAUDECODE", "1").output().unwrap();
    assert!(created.status.success(), "{}", stderr(&created));
    let task = serde_json::from_slice::<serde_json::Value>(&created.stdout).unwrap()["task"]["id"].to_string();
    let session =
        node.json(&["session", "start", &task, "--", "sh", "-c", "echo \"[$CLAUDECODE]\"; sleep 30"])["id"].to_string();
    node.json(&["wait", &session, "--until", "idle", "--timeout", "10s"]);
    let read = node.json(&["read", &session, "--lines", "5"]);
    assert!(read["text"].as_str().unwrap().contains("[]"), "{read}");
    node.cmd(&["session", "kill", &session]);
}
