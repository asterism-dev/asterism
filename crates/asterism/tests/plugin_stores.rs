mod common;

use asterism_proto::types::*;
use common::{commit_all, daemon_with_stores, store_repo, write_plugin};
use serde_json::json;

#[tokio::test]
async fn stores_are_added_listed_refreshed_and_removed() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    let mut events = daemon.subscribe();

    let local = root.path().join("local");
    store_repo(
        &local,
        "local",
        json!([{ "name": "one", "path": "plugins/one" }]),
    );
    write_plugin(&local.join("plugins/one"), "one", "1.0.0", &[]);
    let remote = root.path().join("remote");
    store_repo(&remote, "remote", json!([]));
    commit_all(&remote, "init");

    let added = daemon
        .store_add(&local.display().to_string())
        .await
        .unwrap();
    assert_eq!(
        (added.name.as_str(), added.plugin_count, added.official),
        ("local", 1, false)
    );
    assert!(added.last_refreshed.is_some());
    assert_eq!(events.recv().await.unwrap(), Event::StoresChanged {});
    daemon
        .store_add(&format!("file://{}", remote.display()))
        .await
        .unwrap();
    assert_eq!(events.recv().await.unwrap(), Event::StoresChanged {});

    let list = daemon.store_list();
    assert_eq!(
        list.stores
            .iter()
            .map(|s| s.name.as_str())
            .collect::<Vec<_>>(),
        ["local", "remote"]
    );
    assert!(!list.auto_update && list.error.is_none());

    daemon.set_auto_update(true).await.unwrap();
    assert!(daemon.store_list().auto_update);
    assert_eq!(events.recv().await.unwrap(), Event::StoresChanged {});

    daemon.refresh_stores(None).await.unwrap();
    assert_eq!(events.recv().await.unwrap(), Event::StoresChanged {});

    daemon.store_remove("remote", false).await.unwrap();
    assert_eq!(events.recv().await.unwrap(), Event::StoresChanged {});
    assert_eq!(daemon.store_list().stores.len(), 1);
    assert_eq!(
        daemon.store_remove("nope", false).await.unwrap_err().kind,
        asterism_proto::rpc::ErrorKind::NotFound
    );
}

#[tokio::test]
async fn unreachable_store_keeps_its_index() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    let remote = root.path().join("remote");
    store_repo(
        &remote,
        "remote",
        json!([{ "name": "one", "path": "plugins/one" }]),
    );
    write_plugin(&remote.join("plugins/one"), "one", "1.0.0", &[]);
    commit_all(&remote, "init");
    daemon
        .store_add(&format!("file://{}", remote.display()))
        .await
        .unwrap();

    std::fs::remove_dir_all(&remote).unwrap();
    daemon.refresh_stores(None).await.unwrap();
    let store = daemon.store_list().stores.remove(0);
    assert!(store.last_error.is_some());
    assert_eq!(store.plugin_count, 1);
    assert!(daemon.refresh_stores(Some("remote")).await.is_err());
    assert_eq!(
        daemon.refresh_stores(Some("nope")).await.unwrap_err().kind,
        asterism_proto::rpc::ErrorKind::NotFound
    );
}

#[tokio::test]
async fn broken_files_do_not_stop_the_daemon() {
    let root = tempfile::tempdir().unwrap();
    let paths = asterism_core::paths::Paths {
        home: root.path().join("h"),
    };
    std::fs::create_dir_all(paths.plugins_dir()).unwrap();
    std::fs::write(paths.plugin_stores_file(), "store = [").unwrap();
    std::fs::write(paths.plugin_installed_file(), "plugins = 3").unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    assert!(daemon.store_list().error.is_some());
    let names: Vec<_> = daemon
        .plugin_list()
        .unwrap()
        .into_iter()
        .map(|p| p.name)
        .collect();
    assert_eq!(names, ["claude", "github", "linear"]);
}

use std::path::{Path, PathBuf};

use asterism_proto::rpc::ErrorKind;

const FORGE_PLUGIN: &str = "version = \"1.0.0\"\nprotocol = 1\n[backend]\ncommand = [\"./missing\"]\n[[provides.forge]]\nid = \"github\"\ndisplay_name = \"Fake\"\n";

fn perms(list: &[&str]) -> Vec<String> {
    list.iter().map(|p| p.to_string()).collect()
}

async fn with_local_store(
    root: &Path,
    daemon: &asterism_core::daemon::Daemon,
    name: &str,
    version: &str,
    permissions: &[&str],
) -> PathBuf {
    let dir = root.join(format!("store-{name}"));
    store_repo(
        &dir,
        name,
        json!([{ "name": "one", "path": "plugins/one", "description": "First plugin", "tags": ["agent"] }]),
    );
    write_plugin(&dir.join("plugins/one"), "one", version, permissions);
    daemon.store_add(&dir.display().to_string()).await.unwrap();
    dir
}

fn plugin(daemon: &asterism_core::daemon::Daemon, name: &str) -> Option<PluginInfo> {
    daemon
        .plugin_list()
        .unwrap()
        .into_iter()
        .find(|p| p.name == name)
}

#[tokio::test]
async fn search_details_and_install() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    with_local_store(root.path(), &daemon, "acme", "1.0.0", &["network"]).await;

    let hits = daemon
        .plugin_search(&PluginSearchParams {
            query: Some("first".into()),
            ..Default::default()
        })
        .unwrap();
    assert_eq!(
        (
            hits[0].store.as_str(),
            hits[0].name.as_str(),
            hits[0].installed_version.clone()
        ),
        ("acme", "one", None)
    );
    let details = daemon.plugin_details("acme", "one").await.unwrap();
    assert_eq!(
        (
            details.version.as_str(),
            details.permissions.clone(),
            details.readme.as_deref()
        ),
        ("1.0.0", perms(&["network"]), Some("# one\n"))
    );

    let err = daemon
        .plugin_install("acme", "one", vec![])
        .await
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::PermissionsChanged);
    assert!(plugin(&daemon, "one").is_none());

    let info = daemon
        .plugin_install("acme", "one", perms(&["network"]))
        .await
        .unwrap();
    assert_eq!(
        (info.origin, info.store.as_deref(), info.version.as_deref()),
        (PluginOrigin::Installed, Some("acme"), Some("1.0.0"))
    );
    assert!(daemon.agent_infos().iter().any(|a| a.name == "one-agent"));
    let hits = daemon
        .plugin_search(&PluginSearchParams::default())
        .unwrap();
    assert_eq!(hits[0].installed_version.as_deref(), Some("1.0.0"));
}

#[tokio::test]
async fn updates_need_new_permissions_accepted_and_can_be_rolled_back() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    let store = with_local_store(root.path(), &daemon, "acme", "1.0.0", &[]).await;
    daemon.plugin_install("acme", "one", vec![]).await.unwrap();
    assert!(!plugin(&daemon, "one").unwrap().update_available);

    write_plugin(&store.join("plugins/one"), "one", "1.1.0", &["network"]);
    assert!(plugin(&daemon, "one").unwrap().update_available);
    assert_eq!(
        daemon.plugin_update("one", None).await.unwrap_err().kind,
        ErrorKind::PermissionsChanged
    );
    let updated = daemon
        .plugin_update("one", Some(perms(&["network"])))
        .await
        .unwrap();
    assert_eq!(
        (
            updated.version.as_deref(),
            updated.previous_version.as_deref()
        ),
        (Some("1.1.0"), Some("1.0.0"))
    );
    assert!(!updated.update_available);

    let rolled = daemon.plugin_rollback("one").await.unwrap();
    assert_eq!(rolled.version.as_deref(), Some("1.0.0"));
    daemon.plugin_uninstall("one").await.unwrap();
    assert!(plugin(&daemon, "one").is_none());
}

#[tokio::test]
async fn git_entries_install_pinned_refs() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    let repo = root.path().join("linear");
    std::fs::create_dir_all(&repo).unwrap();
    common::run_git(&repo, &["init", "-q", "-b", "main"]);
    write_plugin(&repo, "linear", "0.3.0", &[]);
    commit_all(&repo, "0.3.0");
    common::run_git(&repo, &["tag", "v0.3.0"]);
    write_plugin(&repo, "linear", "0.4.0", &[]);
    commit_all(&repo, "0.4.0");
    common::run_git(&repo, &["tag", "v0.4.0"]);

    let store = root.path().join("store");
    let entry = |git_ref: &str| json!([{ "name": "linear", "git": format!("file://{}", repo.display()), "ref": git_ref }]);
    store_repo(&store, "acme", entry("v0.3.0"));
    daemon
        .store_add(&store.display().to_string())
        .await
        .unwrap();
    assert_eq!(
        daemon
            .plugin_install("acme", "linear", vec![])
            .await
            .unwrap()
            .version
            .as_deref(),
        Some("0.3.0")
    );

    store_repo(&store, "acme", entry("v0.4.0"));
    assert!(plugin(&daemon, "linear").unwrap().update_available);
    assert_eq!(
        daemon
            .plugin_update("linear", None)
            .await
            .unwrap()
            .version
            .as_deref(),
        Some("0.4.0")
    );

    store_repo(&store, "acme", entry("main"));
    assert!(daemon
        .plugin_update("linear", None)
        .await
        .unwrap_err()
        .message
        .contains("branch"));
}

#[tokio::test]
async fn installed_overrides_builtin_and_disabling_hides_capabilities() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    let store = root.path().join("store");
    store_repo(
        &store,
        "acme",
        json!([{ "name": "github", "path": "github" }, { "name": "gh2", "path": "gh2" }]),
    );
    std::fs::create_dir_all(store.join("github")).unwrap();
    std::fs::write(
        store.join("github/plugin.toml"),
        format!("name = \"github\"\n{FORGE_PLUGIN}"),
    )
    .unwrap();
    std::fs::create_dir_all(store.join("gh2")).unwrap();
    std::fs::write(
        store.join("gh2/plugin.toml"),
        format!("name = \"gh2\"\n{FORGE_PLUGIN}"),
    )
    .unwrap();
    daemon
        .store_add(&store.display().to_string())
        .await
        .unwrap();

    let err = daemon
        .plugin_install("acme", "gh2", vec![])
        .await
        .unwrap_err();
    assert!(err.message.contains("github"), "{}", err.message);
    assert!(plugin(&daemon, "gh2").is_none());

    daemon
        .plugin_install("acme", "github", vec![])
        .await
        .unwrap();
    assert_eq!(
        plugin(&daemon, "github").unwrap().origin,
        PluginOrigin::Installed
    );
    daemon.plugin_uninstall("github").await.unwrap();
    assert_eq!(
        plugin(&daemon, "github").unwrap().origin,
        PluginOrigin::Builtin
    );

    daemon.plugin_set_enabled("github", false).await.unwrap();
    assert_eq!(
        plugin(&daemon, "github").unwrap().state,
        PluginState::Disabled
    );
    assert!(daemon.forges().is_empty());
    assert!(daemon.forge_status("github").await.is_err());
    daemon.plugin_set_enabled("github", true).await.unwrap();
    assert_eq!(daemon.forges().len(), 1);
    assert_eq!(
        daemon
            .plugin_set_enabled("nope", false)
            .await
            .unwrap_err()
            .kind,
        ErrorKind::NotFound
    );
}

#[tokio::test]
async fn one_plugin_name_in_two_stores() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    with_local_store(root.path(), &daemon, "acme", "1.0.0", &[]).await;
    with_local_store(root.path(), &daemon, "other", "2.0.0", &[]).await;
    let stores: Vec<_> = daemon
        .plugin_search(&PluginSearchParams::default())
        .unwrap()
        .into_iter()
        .map(|h| h.store)
        .collect();
    assert_eq!(stores, ["acme", "other"]);
    daemon.plugin_install("acme", "one", vec![]).await.unwrap();
    assert!(daemon
        .plugin_install("other", "one", vec![])
        .await
        .unwrap_err()
        .message
        .contains("uninstall it first"));

    let err = daemon.store_remove("acme", false).await.unwrap_err();
    assert!(err.message.contains("one"), "{}", err.message);
    daemon.store_remove("acme", true).await.unwrap();
    assert!(plugin(&daemon, "one").is_none());
}

#[tokio::test]
async fn auto_update_skips_new_permissions() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    let store = with_local_store(root.path(), &daemon, "acme", "1.0.0", &[]).await;
    daemon.plugin_install("acme", "one", vec![]).await.unwrap();
    daemon.set_auto_update(true).await.unwrap();

    write_plugin(&store.join("plugins/one"), "one", "1.1.0", &[]);
    daemon.refresh_stores(None).await.unwrap();
    assert_eq!(
        plugin(&daemon, "one").unwrap().version.as_deref(),
        Some("1.1.0")
    );

    write_plugin(&store.join("plugins/one"), "one", "1.2.0", &["network"]);
    daemon.refresh_stores(None).await.unwrap();
    let info = plugin(&daemon, "one").unwrap();
    assert_eq!(
        (info.version.as_deref(), info.update_available),
        (Some("1.1.0"), true)
    );
}

#[tokio::test]
async fn auto_update_does_not_undo_a_rollback() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    let store = with_local_store(root.path(), &daemon, "acme", "1.0.0", &[]).await;
    daemon.plugin_install("acme", "one", vec![]).await.unwrap();
    write_plugin(&store.join("plugins/one"), "one", "1.1.0", &[]);
    daemon.refresh_stores(None).await.unwrap();
    daemon.plugin_update("one", None).await.unwrap();
    daemon.plugin_rollback("one").await.unwrap();

    daemon.set_auto_update(true).await.unwrap();
    daemon.refresh_stores(None).await.unwrap();
    assert_eq!(
        plugin(&daemon, "one").unwrap().version.as_deref(),
        Some("1.0.0")
    );
}

#[tokio::test]
async fn invalid_store_json_keeps_the_previous_index() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    let remote = root.path().join("remote");
    store_repo(
        &remote,
        "remote",
        json!([{ "name": "one", "path": "plugins/one" }]),
    );
    write_plugin(&remote.join("plugins/one"), "one", "1.0.0", &[]);
    commit_all(&remote, "init");
    daemon
        .store_add(&format!("file://{}", remote.display()))
        .await
        .unwrap();

    std::fs::write(remote.join("store.json"), "not json").unwrap();
    commit_all(&remote, "break");
    daemon.refresh_stores(None).await.unwrap();
    let store = daemon.store_list().stores.remove(0);
    assert!(store.last_error.is_some());
    assert_eq!(store.plugin_count, 1);
}
