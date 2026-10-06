mod common;

use std::os::unix::fs::symlink;
use std::path::Path;

use asterism_core::plugins::source::*;
use asterism_proto::rpc::ErrorKind;
use common::{commit_all, isolated_git_env, run_git, store_repo, write_plugin};
use serde_json::json;

fn url(path: &Path) -> String {
    format!("file://{}", path.display())
}

#[test]
fn stores_clone_and_refresh() {
    let root = tempfile::tempdir().unwrap();
    let origin = root.path().join("origin");
    store_repo(&origin, "acme", json!([{ "name": "one", "path": "plugins/one" }]));
    write_plugin(&origin.join("plugins/one"), "one", "1.0.0", &[]);
    commit_all(&origin, "first");

    let checkout = root.path().join("checkout");
    clone_store(&url(&origin), &checkout, &isolated_git_env()).unwrap();
    assert_eq!(read_index(&checkout).unwrap().plugins.len(), 1);

    store_repo(&origin, "acme", json!([{ "name": "one", "path": "plugins/one" }, { "name": "two", "path": "plugins/two" }]));
    write_plugin(&origin.join("plugins/two"), "two", "1.0.0", &[]);
    commit_all(&origin, "second");
    refresh_store(&checkout, &isolated_git_env()).unwrap();
    assert_eq!(read_index(&checkout).unwrap().plugins.len(), 2);

    std::fs::remove_dir_all(&origin).unwrap();
    assert_eq!(refresh_store(&checkout, &isolated_git_env()).unwrap_err().kind, ErrorKind::Git);
    assert_eq!(read_index(&checkout).unwrap().plugins.len(), 2);
}

#[test]
fn git_refs_must_be_tags_or_commits() {
    let root = tempfile::tempdir().unwrap();
    let repo = root.path().join("linear");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q", "-b", "main"]);
    write_plugin(&repo, "linear", "0.3.0", &["network"]);
    commit_all(&repo, "v0.3.0");
    run_git(&repo, &["tag", "v0.3.0"]);
    let sha = run_git(&repo, &["rev-parse", "HEAD"]).trim().to_string();
    write_plugin(&repo, "linear", "0.4.0", &["network"]);
    commit_all(&repo, "v0.4.0");

    let cache = root.path().join("cache");
    let env = isolated_git_env();
    let dir = checkout_git(&cache, &url(&repo), "v0.3.0", &env).unwrap();
    assert!(std::fs::read_to_string(dir.join("plugin.toml")).unwrap().contains("0.3.0"));
    let dir = checkout_git(&cache, &url(&repo), &sha, &env).unwrap();
    assert!(std::fs::read_to_string(dir.join("plugin.toml")).unwrap().contains("0.3.0"));
    let err = checkout_git(&cache, &url(&repo), "main", &env).unwrap_err();
    assert!(err.message.contains("branch"), "{}", err.message);
    assert_eq!(checkout_git(&cache, &url(&repo), "v9", &env).unwrap_err().kind, ErrorKind::InvalidParams);
}

#[test]
fn plugin_dir_rejects_escapes() {
    let root = tempfile::tempdir().unwrap();
    let store = root.path().join("store");
    let outside = root.path().join("outside");
    std::fs::create_dir_all(store.join("plugins/ok")).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    symlink(&outside, store.join("plugins/sneaky")).unwrap();
    assert!(plugin_dir(&store, "plugins/ok").unwrap().ends_with("plugins/ok"));
    for bad in ["../outside", "/etc", "plugins/sneaky", "plugins/missing"] {
        assert_eq!(plugin_dir(&store, bad).unwrap_err().kind, ErrorKind::InvalidParams, "{bad}");
    }
}

#[test]
fn copy_skips_git_and_refuses_symlinks() {
    let root = tempfile::tempdir().unwrap();
    let from = root.path().join("from");
    write_plugin(&from, "one", "1.0.0", &[]);
    std::fs::create_dir_all(from.join(".git/objects")).unwrap();
    std::fs::create_dir_all(from.join("lib")).unwrap();
    std::fs::write(from.join("lib/run.sh"), "#!/bin/sh\n").unwrap();
    copy_tree(&from, &root.path().join("to")).unwrap();
    assert!(root.path().join("to/lib/run.sh").exists());
    assert!(!root.path().join("to/.git").exists());

    symlink("/etc/passwd", from.join("lib/link")).unwrap();
    assert!(copy_tree(&from, &root.path().join("to2")).unwrap_err().message.contains("symlink"));
    assert_eq!(readme(&from).as_deref(), Some("# one\n"));
    std::fs::write(from.join("README.md"), "x".repeat(README_LIMIT + 10)).unwrap();
    assert_eq!(readme(&from).unwrap().len(), README_LIMIT);
}
