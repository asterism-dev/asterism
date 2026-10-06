mod common;

use std::os::unix::fs::symlink;
use std::path::PathBuf;

use asterism_core::paths::Paths;
use asterism_core::plugins::{install, store_ops};
use asterism_proto::rpc::ErrorKind;
use common::{commit_all, isolated_git_env, store_repo, write_plugin};
use serde_json::json;

struct Env {
    root: tempfile::TempDir,
    paths: Paths,
}

fn env() -> Env {
    let root = tempfile::tempdir().unwrap();
    let paths = Paths { home: root.path().join("h") };
    paths.ensure_dirs().unwrap();
    std::fs::create_dir_all(paths.plugins_dir()).unwrap();
    std::fs::write(paths.plugin_stores_file(), "auto_update = false\n").unwrap();
    Env { root, paths }
}

/// A local store (read in place) with plugin `one` at `version`.
fn local_store(env: &Env, version: &str, permissions: &[&str]) -> PathBuf {
    let dir = env.root.path().join("local-store");
    store_repo(&dir, "local", json!([{ "name": "one", "path": "plugins/one", "tags": ["agent"] }]));
    write_plugin(&dir.join("plugins/one"), "one", version, permissions);
    dir
}

fn installed_versions(paths: &Paths, name: &str) -> Vec<String> {
    let mut versions: Vec<String> = std::fs::read_dir(paths.plugins_installed().join(name))
        .map(|dir| dir.map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect())
        .unwrap_or_default();
    versions.sort();
    versions
}

fn install(env: &Env) -> asterism_core::error::Result<install::Resolved> {
    let (store, entry) = install::find_entry(&env.paths, "local", "one")?;
    let resolved = install::resolve(&env.paths, &store, &entry, &isolated_git_env())?;
    install::install_files(&env.paths, &resolved)?;
    install::record(&env.paths, "local", &resolved)?;
    Ok(resolved)
}

#[test]
fn local_and_git_stores_are_added_once() {
    let env = env();
    let local = local_store(&env, "1.0.0", &[]);
    let added = store_ops::add_store(&env.paths, &local.display().to_string(), &isolated_git_env()).unwrap();
    assert_eq!((added.name.as_str(), added.official), ("local", false));
    assert_eq!(store_ops::store_dir(&env.paths, &added), local.canonicalize().unwrap());

    let remote = env.root.path().join("remote");
    store_repo(&remote, "remote", json!([]));
    commit_all(&remote, "init");
    let url = format!("file://{}", remote.display());
    let added = store_ops::add_store(&env.paths, &url, &isolated_git_env()).unwrap();
    assert!(env.paths.plugin_stores_dir().join("remote/store.json").exists());
    assert_eq!(store_ops::sync_store(&env.paths, &added, &isolated_git_env()).unwrap().name, "remote");

    let again = store_ops::add_store(&env.paths, &url, &isolated_git_env()).unwrap_err();
    assert!(again.message.contains("already"), "{}", again.message);
    let leftovers: Vec<_> = std::fs::read_dir(env.paths.plugin_stores_dir()).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert_eq!(leftovers, ["remote"]);

    store_ops::remove_store(&env.paths, "remote").unwrap();
    assert!(!env.paths.plugin_stores_dir().join("remote").exists());
    store_ops::remove_store(&env.paths, "local").unwrap();
    assert!(local.exists(), "local stores are never deleted");
    assert!(store_ops::load(&env.paths).unwrap().stores.is_empty());
}

#[test]
fn install_update_rollback_and_uninstall() {
    let env = env();
    let local = local_store(&env, "1.0.0", &[]);
    store_ops::add_store(&env.paths, &local.display().to_string(), &isolated_git_env()).unwrap();
    install(&env).unwrap();
    std::fs::create_dir_all(env.paths.plugin_data("one")).unwrap();

    write_plugin(&local.join("plugins/one"), "one", "1.1.0", &[]);
    install(&env).unwrap();
    write_plugin(&local.join("plugins/one"), "one", "1.2.0", &[]);
    install(&env).unwrap();
    let entry = install::load(&env.paths).unwrap().plugins["one"].clone();
    assert_eq!((entry.version.as_str(), entry.previous.as_deref()), ("1.2.0", Some("1.1.0")));
    assert_eq!(installed_versions(&env.paths, "one"), ["1.1.0", "1.2.0"]);

    let rolled = install::rollback(&env.paths, "one").unwrap();
    assert_eq!((rolled.version.as_str(), rolled.previous.as_deref()), ("1.1.0", Some("1.2.0")));
    assert_eq!(install::rollback(&env.paths, "missing").unwrap_err().kind, ErrorKind::NotFound);

    install::set_enabled(&env.paths, "one", false).unwrap();
    assert!(install::load(&env.paths).unwrap().disabled.contains("one"));
    install::set_enabled(&env.paths, "one", true).unwrap();
    assert!(install::load(&env.paths).unwrap().disabled.is_empty());

    install::uninstall(&env.paths, "one").unwrap();
    assert!(!env.paths.plugins_installed().join("one").exists());
    assert!(env.paths.plugin_data("one").exists());
    assert!(!install::load(&env.paths).unwrap().plugins.contains_key("one"));
}

#[test]
fn escaping_entry_installs_nothing() {
    let env = env();
    let local = local_store(&env, "1.0.0", &[]);
    let outside = env.root.path().join("outside");
    write_plugin(&outside, "one", "6.6.6", &[]);
    std::fs::remove_dir_all(local.join("plugins/one")).unwrap();
    symlink(&outside, local.join("plugins/one")).unwrap();
    store_ops::add_store(&env.paths, &local.display().to_string(), &isolated_git_env()).unwrap();
    assert_eq!(install(&env).unwrap_err().kind, ErrorKind::InvalidParams);
    assert!(!env.paths.plugins_installed().join("one").exists());
}

#[test]
fn failed_copy_leaves_nothing_behind() {
    let env = env();
    let local = local_store(&env, "1.0.0", &[]);
    symlink("/etc/hosts", local.join("plugins/one/hosts")).unwrap();
    store_ops::add_store(&env.paths, &local.display().to_string(), &isolated_git_env()).unwrap();
    assert!(install(&env).unwrap_err().message.contains("symlink"));
    assert!(installed_versions(&env.paths, "one").is_empty());
    assert!(install::load(&env.paths).unwrap().plugins.is_empty());
}

#[test]
fn entries_must_name_their_plugin() {
    let env = env();
    let local = local_store(&env, "1.0.0", &[]);
    write_plugin(&local.join("plugins/one"), "other", "1.0.0", &[]);
    store_ops::add_store(&env.paths, &local.display().to_string(), &isolated_git_env()).unwrap();
    assert!(install(&env).unwrap_err().message.contains("other"));
}

#[test]
fn permission_helpers() {
    let s = |v: &[&str]| v.iter().map(|p| p.to_string()).collect::<Vec<_>>();
    assert!(install::same_permissions(&s(&["network", "exec:gh"]), &s(&["exec:gh", "network"])));
    assert!(!install::same_permissions(&s(&["network"]), &s(&["network", "exec:gh"])));
    assert!(install::adds_permissions(&s(&["network"]), &s(&["network", "exec:gh"])));
    assert!(!install::adds_permissions(&s(&["network", "exec:gh"]), &s(&["network"])));
}


#[test]
fn bad_names_are_refused_before_touching_disk() {
    let env = env();
    for name in ["../x", "a/b", "-x", ""] {
        assert_eq!(install::uninstall(&env.paths, name).unwrap_err().kind, ErrorKind::InvalidParams, "{name}");
        assert_eq!(install::rollback(&env.paths, name).unwrap_err().kind, ErrorKind::InvalidParams, "{name}");
        assert_eq!(install::set_enabled(&env.paths, name, true).unwrap_err().kind, ErrorKind::InvalidParams, "{name}");
    }
}
