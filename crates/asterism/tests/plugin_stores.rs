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
    store_repo(&local, "local", json!([{ "name": "one", "path": "plugins/one" }]));
    write_plugin(&local.join("plugins/one"), "one", "1.0.0", &[]);
    let remote = root.path().join("remote");
    store_repo(&remote, "remote", json!([]));
    commit_all(&remote, "init");

    let added = daemon.store_add(&local.display().to_string()).await.unwrap();
    assert_eq!((added.name.as_str(), added.plugin_count, added.official), ("local", 1, false));
    assert!(added.last_refreshed.is_some());
    assert_eq!(events.recv().await.unwrap(), Event::StoresChanged {});
    daemon.store_add(&format!("file://{}", remote.display())).await.unwrap();
    assert_eq!(events.recv().await.unwrap(), Event::StoresChanged {});

    let list = daemon.store_list();
    assert_eq!(list.stores.iter().map(|s| s.name.as_str()).collect::<Vec<_>>(), ["local", "remote"]);
    assert!(!list.auto_update && list.error.is_none());

    daemon.set_auto_update(true).await.unwrap();
    assert!(daemon.store_list().auto_update);
    assert_eq!(events.recv().await.unwrap(), Event::StoresChanged {});

    daemon.refresh_stores(None).await.unwrap();
    assert_eq!(events.recv().await.unwrap(), Event::StoresChanged {});

    daemon.store_remove("remote", false).await.unwrap();
    assert_eq!(events.recv().await.unwrap(), Event::StoresChanged {});
    assert_eq!(daemon.store_list().stores.len(), 1);
    assert_eq!(daemon.store_remove("nope", false).await.unwrap_err().kind, asterism_proto::rpc::ErrorKind::NotFound);
}

#[tokio::test]
async fn unreachable_store_keeps_its_index() {
    let root = tempfile::tempdir().unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    let remote = root.path().join("remote");
    store_repo(&remote, "remote", json!([{ "name": "one", "path": "plugins/one" }]));
    write_plugin(&remote.join("plugins/one"), "one", "1.0.0", &[]);
    commit_all(&remote, "init");
    daemon.store_add(&format!("file://{}", remote.display())).await.unwrap();

    std::fs::remove_dir_all(&remote).unwrap();
    daemon.refresh_stores(None).await.unwrap();
    let store = daemon.store_list().stores.remove(0);
    assert!(store.last_error.is_some());
    assert_eq!(store.plugin_count, 1);
    assert!(daemon.refresh_stores(Some("remote")).await.is_err());
    assert_eq!(daemon.refresh_stores(Some("nope")).await.unwrap_err().kind, asterism_proto::rpc::ErrorKind::NotFound);
}

#[tokio::test]
async fn broken_files_do_not_stop_the_daemon() {
    let root = tempfile::tempdir().unwrap();
    let paths = asterism_core::paths::Paths { home: root.path().join("h") };
    std::fs::create_dir_all(paths.plugins_dir()).unwrap();
    std::fs::write(paths.plugin_stores_file(), "store = [").unwrap();
    std::fs::write(paths.plugin_installed_file(), "plugins = 3").unwrap();
    let (_paths, daemon) = daemon_with_stores(root.path());
    assert!(daemon.store_list().error.is_some());
    let names: Vec<_> = daemon.plugin_list().unwrap().into_iter().map(|p| p.name).collect();
    assert_eq!(names, ["claude", "github"]);
}
