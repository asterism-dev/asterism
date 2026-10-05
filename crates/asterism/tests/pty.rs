mod common;

use std::time::Duration;

use asterism_core::session::{Pty, SpawnSpec, SCROLLBACK_LINES, TAIL_BYTES};
use common::eventually;

fn spec(cwd: &std::path::Path, argv: &[&str]) -> SpawnSpec {
    SpawnSpec {
        argv: argv.iter().map(|s| s.to_string()).collect(),
        cwd: cwd.to_path_buf(),
        env: vec![("ASTERISM_TEST".into(), "yes".into()), ("PATH".into(), std::env::var("PATH").unwrap())],
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

async fn replayed(argv: &[&str], ready: &str) -> (std::sync::Arc<Pty>, vt100::Parser, usize) {
    let dir = tempfile::tempdir().unwrap();
    let pty = Pty::spawn(spec(dir.path(), argv)).unwrap();
    assert!(eventually(|| pty.text().contains(ready)).await);
    let (snapshot, _) = pty.attach();
    let mut parser = vt100::Parser::new(snapshot.rows, snapshot.cols, SCROLLBACK_LINES);
    parser.process(&snapshot.screen);
    (pty, parser, snapshot.screen.len())
}

// vt100 panics when scrolled back beyond one screen, so grow the screen first (as `Pty::history` does).
fn oldest_rows(parser: &mut vt100::Parser) -> String {
    let (rows, cols) = parser.screen().size();
    parser.set_scrollback(usize::MAX);
    let depth = parser.screen().scrollback();
    parser.set_size(rows + depth as u16, cols);
    parser.set_scrollback(depth);
    parser.screen().contents()
}

#[tokio::test]
async fn attach_restores_scrollback() {
    let (pty, mut parser, _) = replayed(&["sh", "-c", "seq 1 100; cat"], "100").await;
    assert_eq!(parser.screen().contents(), pty.text());
    assert!(oldest_rows(&mut parser).starts_with("1\n2\n"));
}

#[tokio::test]
async fn attach_restores_alternate_screen_over_scrollback() {
    let argv = ["sh", "-c", r"seq 1 100; printf '\033[?1049h\033[HTUI'; cat"];
    let (_pty, mut parser, _) = replayed(&argv, "TUI").await;
    assert!(parser.screen().alternate_screen());
    assert!(parser.screen().contents().starts_with("TUI"));
    parser.process(b"\x1b[?1049l");
    assert!(parser.screen().contents().contains("100"));
    assert!(oldest_rows(&mut parser).starts_with("1\n2\n"));
}

#[tokio::test]
async fn attach_bounds_the_replayed_output() {
    let argv = ["sh", "-c", r"yes 0123456789 | head -c 3000000; printf '\033[?1049h\033[HTUI'; cat"];
    let (_pty, parser, len) = replayed(&argv, "TUI").await;
    assert!(len < TAIL_BYTES + 64 * 1024, "snapshot is {len} bytes");
    assert!(parser.screen().alternate_screen());
    assert!(parser.screen().contents().starts_with("TUI"));
}
