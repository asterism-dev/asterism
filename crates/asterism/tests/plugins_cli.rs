mod common;

use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::process::Stdio;

use common::Node;

fn fixture() -> String {
    common::fixture_dir().display().to_string()
}

fn plugin<'a>(list: &'a serde_json::Value, name: &str) -> Option<&'a serde_json::Value> {
    list.as_array().unwrap().iter().find(|p| p["name"] == name)
}

#[test]
fn link_list_command_and_unlink() {
    let node = Node::new();
    let builtins = node.json(&["plugin", "list"]);
    for name in ["claude", "github"] {
        assert_eq!(plugin(&builtins, name).unwrap()["state"]["state"], "ok", "{builtins}");
    }

    let linked = node.json(&["plugin", "link", &fixture()]);
    assert_eq!((linked["name"].as_str(), linked["origin"].as_str()), (Some("echo"), Some("linked")));
    let list = node.json(&["plugin", "list"]);
    let echo = plugin(&list, "echo").unwrap();
    assert_eq!(echo["state"], serde_json::json!({"state": "needs_setup", "missing": ["Token"]}));
    let kinds: Vec<_> = echo["capabilities"].as_array().unwrap().iter().map(|c| c["kind"].as_str().unwrap()).collect();
    assert_eq!(kinds, ["forge", "agent", "command", "task_source"]);

    let out = node.cmd(&["echo-cmd", "hello", "--exit", "4"]);
    assert_eq!(out.status.code(), Some(4));
    assert_eq!(String::from_utf8_lossy(&out.stdout).trim(), "echo-cmd hello --exit 4");

    let unknown = node.cmd(&["no-such-command"]);
    assert!(!unknown.status.success());
    assert!(String::from_utf8_lossy(&unknown.stderr).contains("unknown command"));

    node.json(&["plugin", "unlink", "echo"]);
    assert!(plugin(&node.json(&["plugin", "list"]), "echo").is_none());
}

#[test]
fn deleted_linked_plugins_are_broken_and_can_be_unlinked() {
    let node = Node::new();
    let copy = tempfile::tempdir().unwrap();
    std::fs::copy(common::fixture_dir().join("plugin.toml"), copy.path().join("plugin.toml")).unwrap();
    node.json(&["plugin", "link", &copy.path().display().to_string()]);
    std::fs::remove_file(copy.path().join("plugin.toml")).unwrap();
    node.json(&["plugin", "reload"]);
    let echo = plugin(&node.json(&["plugin", "list"]), "echo").cloned().unwrap();
    assert_eq!(echo["state"]["state"], "broken");
    assert!(echo["state"]["reason"].as_str().unwrap().contains("cannot read"));
    node.json(&["plugin", "unlink", "echo"]);
}

#[test]
fn config_sets_values_and_masks_secrets() {
    let node = Node::new();
    node.json(&["plugin", "link", &fixture()]);
    node.json(&["plugin", "config", "echo", "region", "us"]);
    assert!(!node.cmd(&["plugin", "config", "echo", "region", "mars"]).status.success());

    let mut child = node.command(&["plugin", "config", "echo", "token"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    child.stdin.take().unwrap().write_all(b"s3cret\n").unwrap();
    let out = child.wait_with_output().unwrap();
    assert!(out.status.success(), "{}", String::from_utf8_lossy(&out.stderr));

    let mut empty = node.command(&["plugin", "config", "echo", "token"]).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    empty.stdin.take().unwrap().write_all(b"\n").unwrap();
    let out = empty.wait_with_output().unwrap();
    assert!(!out.status.success() && String::from_utf8_lossy(&out.stderr).contains("cannot be empty"));

    let shown = node.cmd(&["plugin", "config", "echo"]);
    let text = String::from_utf8_lossy(&shown.stdout);
    assert!(text.contains("region = \"us\"") && text.contains("token = <set>") && !text.contains("s3cret"), "{text}");
    let json = node.json(&["plugin", "config", "echo"]);
    assert_eq!(json["secrets_set"], serde_json::json!(["token"]));
    assert!(!json.to_string().contains("s3cret"));
    let mode = std::fs::metadata(node.home().join("secrets.toml")).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
    assert_eq!(plugin(&node.json(&["plugin", "list"]), "echo").unwrap()["state"]["state"], "ok");

    node.json(&["plugin", "config", "echo", "token", "--clear"]);
    assert_eq!(plugin(&node.json(&["plugin", "list"]), "echo").unwrap()["state"]["state"], "needs_setup");
}
