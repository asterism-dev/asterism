mod common;

use std::path::{Path, PathBuf};
use std::process::Stdio;

use common::{store_repo, write_plugin, Node};
use serde_json::json;

fn store(root: &Path, name: &str, version: &str) -> PathBuf {
    let dir = root.join(name);
    store_repo(&dir, name, json!([{ "name": "one", "path": "plugins/one", "description": "First", "tags": ["agent"] }]));
    write_plugin(&dir.join("plugins/one"), "one", version, &["network"]);
    dir
}

fn stderr(out: &std::process::Output) -> String {
    String::from_utf8_lossy(&out.stderr).into_owned()
}

#[test]
fn add_search_install_and_list() {
    let node = Node::new();
    let root = tempfile::tempdir().unwrap();
    let acme = store(root.path(), "acme", "1.0.0").display().to_string();

    let unconfirmed = node.command(&["store", "add", &acme]).stdin(Stdio::null()).output().unwrap();
    assert!(!unconfirmed.status.success());
    assert!(stderr(&unconfirmed).contains("--yes"), "{}", stderr(&unconfirmed));
    assert_eq!(node.json(&["store", "add", &acme, "--yes"])["name"], "acme");
    assert_eq!(node.json(&["store", "list"])["stores"][0]["plugin_count"], 1);

    let hits = node.json(&["plugin", "search", "one"]);
    assert_eq!((hits[0]["store"].as_str(), hits[0]["name"].as_str()), (Some("acme"), Some("one")));
    let info = node.json(&["plugin", "info", "one"]);
    assert_eq!(info["permissions"], json!(["network"]));

    let installed = node.json(&["plugin", "install", "one", "--yes"]);
    assert_eq!((installed["origin"].as_str(), installed["store"].as_str()), (Some("installed"), Some("acme")));
    let listed = node.cmd(&["plugin", "list"]);
    assert!(String::from_utf8_lossy(&listed.stdout).contains("installed from acme"));
}

#[test]
fn ambiguous_install_needs_a_store() {
    let node = Node::new();
    let root = tempfile::tempdir().unwrap();
    for name in ["acme", "other"] {
        node.json(&["store", "add", &store(root.path(), name, "1.0.0").display().to_string(), "--yes"]);
    }
    let out = node.cmd(&["plugin", "install", "one", "--yes"]);
    assert!(!out.status.success());
    assert!(stderr(&out).contains("--store"), "{}", stderr(&out));
    assert_eq!(node.json(&["plugin", "install", "one", "--store", "other", "--yes"])["store"], "other");
}

#[test]
fn update_all_and_auto_update_switch() {
    let node = Node::new();
    let root = tempfile::tempdir().unwrap();
    let acme = store(root.path(), "acme", "1.0.0");
    node.json(&["store", "add", &acme.display().to_string(), "--yes"]);
    node.json(&["plugin", "install", "one", "--yes"]);
    write_plugin(&acme.join("plugins/one"), "one", "1.1.0", &["network", "exec:glab"]);

    let refused = node.command(&["plugin", "update", "--all"]).stdin(Stdio::null()).output().unwrap();
    assert!(!refused.status.success());
    let updated = node.json(&["plugin", "update", "--all", "--yes"]);
    assert_eq!(updated[0]["version"], "1.1.0");

    node.json(&["store", "auto-update", "on"]);
    assert_eq!(node.json(&["store", "list"])["auto_update"], true);
    node.json(&["plugin", "disable", "one"]);
    assert_eq!(node.json(&["plugin", "list"]).as_array().unwrap().iter().find(|p| p["name"] == "one").unwrap()["state"]["state"], "disabled");
    node.json(&["plugin", "uninstall", "one"]);
    node.json(&["store", "remove", "acme"]);
}
