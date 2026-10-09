mod common;

use common::Node;

#[test]
fn review_comments_prints_open_local_threads() {
    let node = Node::new();
    let created = node.json(&["task", "new", "t"]);
    let id = created["task"]["id"].as_i64().unwrap();
    let wt = created["task"]["worktree_path"].as_str().unwrap();
    std::fs::write(std::path::Path::new(wt).join("a.rs"), "one\n").unwrap();
    common::call(
        &node,
        "review.comment",
        serde_json::json!({"task_id": id, "source": "local", "path": "a.rs",
            "line": 1, "side": "new", "body": "Add a test.", "target": "local"}),
    );
    let id = id.to_string();

    let out = node.cmd(&["review", "comments", "--task", &id]);
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout),
        "Review comments on your changes:\n\na.rs:1\n> one\n- (local) Add a test.\n\n"
    );

    let threads = node.json(&["review", "comments", "--task", &id]);
    assert_eq!(threads[0]["comments"][0]["body"], "Add a test.");
}

#[test]
fn review_checks_needs_a_pull_request() {
    let node = Node::new();
    let created = node.json(&["task", "new", "t"]);
    let id = created["task"]["id"].as_i64().unwrap().to_string();
    let out = node.cmd(&["review", "checks", "--task", &id]);
    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("pull request"), "{stderr}");
}
