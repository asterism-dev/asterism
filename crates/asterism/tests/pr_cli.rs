mod common;

use std::io::Write;
use std::process::Stdio;

use common::Node;

#[test]
fn tasks_show_their_pull_request() {
    let node = Node::new();
    common::run_git(
        node.repo(),
        &["remote", "add", "origin", "https://echo.test/acme/demo.git"],
    );
    let prs = node.home().join("prs.json");
    // The first command starts the daemon, which hands its environment to plugin backends.
    let linked = node
        .command(&[
            "--json",
            "plugin",
            "link",
            &common::fixture_dir().display().to_string(),
        ])
        .env("FIXTURE_PRS", &prs)
        .output()
        .unwrap();
    assert!(
        linked.status.success(),
        "{}",
        String::from_utf8_lossy(&linked.stderr)
    );
    let mut child = node
        .command(&["plugin", "config", "echo", "token"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child.stdin.take().unwrap().write_all(b"t\n").unwrap();
    assert!(child.wait_with_output().unwrap().status.success());

    let created = node.json(&["task", "new", "Fix login"]);
    let branch = created["task"]["branch"].as_str().unwrap().to_string();
    std::fs::write(&prs, serde_json::json!({ (branch): {"number": 7, "url": "https://echo.test/pr/7", "title": "T", "state": "open",
        "review": "none", "checks": {"state": "failure", "failing": ["lint"]}} }).to_string()).unwrap();

    let refreshed = node.json(&["pr", "refresh"]);
    assert_eq!(refreshed["prs"][0]["pr"]["number"], 7);
    let tasks = node.json(&["task", "list"]);
    assert_eq!(tasks[0]["pr"]["checks"]["failing"][0], "lint");
    let text = String::from_utf8_lossy(&node.cmd(&["task", "list"]).stdout).into_owned();
    assert!(
        text.lines()
            .next()
            .unwrap()
            .ends_with("\t#7 open, checks failing"),
        "{text}"
    );
}
