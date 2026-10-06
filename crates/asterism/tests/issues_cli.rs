mod common;

use std::io::Write;
use std::process::Stdio;

use common::Node;

fn linked_with_token() -> Node {
    let node = Node::new();
    node.json(&["plugin", "link", &common::fixture_dir().display().to_string()]);
    let mut child = node.command(&["plugin", "config", "echo", "token"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"t\n").unwrap();
    assert!(child.wait_with_output().unwrap().status.success());
    node
}

#[test]
fn issue_search_lists_hits() {
    let node = linked_with_token();
    let hits = node.json(&["issue", "search", "echo-issues", "login"]);
    assert_eq!(hits[0]["key"], "ECH-1");
    let text = String::from_utf8_lossy(&node.cmd(&["issue", "search", "echo-issues", "--all"]).stdout).into_owned();
    assert!(text.lines().next().unwrap().starts_with("ECH-1\topen\tFix login timeout"), "{text}");
}

#[test]
fn tasks_are_created_from_issues() {
    let node = linked_with_token();
    let created = node.json(&["task", "create", "--issue", "echo-issues:ECH-1"]);
    assert_eq!(created["task"]["title"], "ech-1-fix-login-timeout");
    assert_eq!(created["task"]["branch"], "feature/ech-1-fix-login-timeout");
    assert_eq!(created["task"]["issue"]["key"], "ECH-1");
    assert!(created["task"]["prompt"].as_str().unwrap().starts_with("# Fix login timeout"));

    let titled = node.json(&["task", "new", "Mine", "--issue", "echo-issues:ECH-2", "--prompt", "do it"]);
    assert_eq!((titled["task"]["title"].as_str(), titled["task"]["prompt"].as_str()), (Some("Mine"), Some("do it")));

    assert!(!node.cmd(&["task", "new"]).status.success());
    let bad = node.cmd(&["task", "new", "--issue", "ECH-1"]);
    assert!(String::from_utf8_lossy(&bad.stderr).contains("<source>:<key>"));
}

#[test]
fn issues_without_a_usable_branch_fall_back_to_the_task_id() {
    let node = linked_with_token();
    let created = node.json(&["task", "new", "--issue", "echo-issues:%%"]);
    let id = created["task"]["id"].as_i64().unwrap();
    assert_eq!(created["task"]["branch"], format!("asterism/{id}"));
}
