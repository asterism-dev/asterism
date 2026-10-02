mod common;

use std::time::Duration;

use asterism_core::session::{Pty, SpawnSpec};
use common::eventually;

fn spec(cwd: &std::path::Path, argv: &[&str]) -> SpawnSpec {
    SpawnSpec {
        argv: argv.iter().map(|s| s.to_string()).collect(),
        cwd: cwd.to_path_buf(),
        env: vec![("ASTERISM_TEST".into(), "yes".into())],
        rows: 24,
        cols: 80,
    }
}

#[tokio::test]
async fn output_lands_on_screen_and_input_reaches_process() {
    let dir = tempfile::tempdir().unwrap();
    let pty = Pty::spawn(spec(dir.path(), &["sh", "-c", "echo ready $ASTERISM_TEST; cat"])).unwrap();
    assert!(eventually(|| pty.text().contains("ready yes")).await);
    pty.write(b"ping\n").unwrap();
    assert!(eventually(|| pty.text().contains("ping")).await);
}

#[tokio::test]
async fn attach_returns_snapshot_then_streams_new_output() {
    let dir = tempfile::tempdir().unwrap();
    let pty = Pty::spawn(spec(dir.path(), &["sh", "-c", "echo before; cat"])).unwrap();
    assert!(eventually(|| pty.text().contains("before")).await);

    let (snapshot, mut rx) = pty.attach();
    assert!(String::from_utf8_lossy(&snapshot.screen).contains("before"));
    assert_eq!((snapshot.rows, snapshot.cols), (24, 80));

    pty.write(b"after\n").unwrap();
    let mut seen = String::new();
    while !seen.contains("after") {
        let chunk = tokio::time::timeout(Duration::from_secs(5), rx.recv()).await.unwrap().unwrap();
        seen.push_str(&String::from_utf8_lossy(&chunk));
    }
}

#[tokio::test]
async fn resize_changes_snapshot_size() {
    let dir = tempfile::tempdir().unwrap();
    let pty = Pty::spawn(spec(dir.path(), &["sh", "-c", "cat"])).unwrap();
    pty.resize(10, 40).unwrap();
    let (snapshot, _) = pty.attach();
    assert_eq!((snapshot.rows, snapshot.cols), (10, 40));
}

#[tokio::test]
async fn exit_and_kill_are_reported() {
    let dir = tempfile::tempdir().unwrap();
    let done = Pty::spawn(spec(dir.path(), &["sh", "-c", "exit 0"])).unwrap();
    let mut rx = done.exited();
    tokio::time::timeout(Duration::from_secs(5), rx.wait_for(|e| *e)).await.unwrap().unwrap();

    let sleeper = Pty::spawn(spec(dir.path(), &["sh", "-c", "sleep 30"])).unwrap();
    sleeper.kill().unwrap();
    let mut rx = sleeper.exited();
    tokio::time::timeout(Duration::from_secs(5), rx.wait_for(|e| *e)).await.unwrap().unwrap();
}

#[test]
fn empty_argv_is_rejected() {
    let dir = tempfile::tempdir().unwrap();
    assert!(Pty::spawn(spec(dir.path(), &[])).is_err());
}
