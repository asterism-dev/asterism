mod common;

use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use asterism_core::daemon::{Daemon, DaemonOptions};
use asterism_core::paths::Paths;
use asterism_proto::client::Client;
use asterism_proto::types::{method, *};
use asterism_proto::PROTO_VERSION;
use futures_util::{SinkExt, StreamExt};
use serde_json::{json, Value};
use tokio::net::UnixStream;
use tokio_tungstenite::tungstenite::Message;

fn vmfake_dir() -> std::path::PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins/vmfake")
}

fn options(vms: &Path, fail: &str) -> DaemonOptions {
    DaemonOptions {
        plugin_env: vec![
            ("FIXTURE_VMS".into(), vms.display().to_string()),
            ("FIXTURE_FAIL".into(), fail.into()),
        ],
        ..common::daemon_options()
    }
}

fn link_vmfake(paths: &Paths) {
    std::fs::create_dir_all(paths.plugins_dir()).unwrap();
    std::fs::write(
        paths.plugin_links(),
        format!(
            "[links]\nvmfake = {:?}\n",
            vmfake_dir().display().to_string()
        ),
    )
    .unwrap();
}

fn daemon(home: &Path, fail: &str) -> Arc<Daemon> {
    let paths = Paths {
        home: home.join("h"),
    };
    paths.ensure_dirs().unwrap();
    link_vmfake(&paths);
    let vms = home.join("vms.json");
    std::fs::write(
        &vms,
        json!({"vms": {
            "a": {"name": "one", "state": "running", "destination": "u@one"},
            "b": {"name": "two", "state": "hibernated", "destination": "u@two"},
        }, "calls": []})
        .to_string(),
    )
    .unwrap();
    Daemon::with_options(paths, options(&vms, fail)).unwrap()
}

#[tokio::test]
async fn machine_provider_list_returns_vms_and_options() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), "");
    let all = d.machine_providers().await.unwrap();
    assert_eq!(all.len(), 1);
    let info = &all[0];
    assert_eq!(
        (info.provider.as_str(), info.display_name.as_str()),
        ("vmfake", "Fake VMs")
    );
    assert_eq!(info.error, None);
    assert_eq!(info.vms.len(), 2);
    assert_eq!(info.create_options[0].key, "cpus");
}

#[tokio::test]
async fn machine_provider_list_reports_plugin_errors_per_provider() {
    let home = tempfile::tempdir().unwrap();
    let d = daemon(home.path(), "machine_provider.list");
    let all = d.machine_providers().await.unwrap();
    assert_eq!(all.len(), 1);
    assert!(all[0].error.as_deref().unwrap().contains("forced failure"));
    assert!(all[0].vms.is_empty());
}

#[tokio::test]
async fn machine_provider_list_denied_to_remote_devices() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: home.path().to_path_buf(),
    };
    let socket = paths.socket();
    tokio::spawn(asterism_core::run_with_options(
        paths,
        DaemonOptions {
            remote_retry: Duration::from_millis(100),
            ..common::daemon_options()
        },
    ));
    for _ in 0..100 {
        if UnixStream::connect(&socket).await.is_ok() {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    let client = Client::connect_unix(&socket).await.unwrap();
    let _: HelloResult = client
        .call(
            method::HELLO,
            HelloParams {
                proto_version: PROTO_VERSION,
                client_kind: ClientKind::Cli,
            },
        )
        .await
        .unwrap();
    let _: Value = client
        .call(
            method::REMOTE_CONFIG_SET,
            RemoteConfigSetParams {
                config: RemoteConfig {
                    enabled: true,
                    bind: "127.0.0.1".into(),
                    port: 0,
                    ..RemoteConfig::default()
                },
            },
        )
        .await
        .unwrap();
    let addr: SocketAddr = loop {
        let status: RemoteStatus = client.call(method::REMOTE_STATUS, json!({})).await.unwrap();
        if status.state == RemoteState::Listening {
            break status.address.unwrap().parse().unwrap();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    };
    let pairing: RemotePairing = client
        .call(
            method::REMOTE_DEVICE_ADD,
            RemoteDeviceAddParams {
                name: "phone".into(),
            },
        )
        .await
        .unwrap();
    let token = pairing.pairing_url.split("#token=").nth(1).unwrap();

    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    ws.send(Message::Text(json!({ "auth": token }).to_string().into()))
        .await
        .unwrap();
    let request =
        json!({"jsonrpc": "2.0", "id": 1, "method": method::MACHINE_PROVIDER_LIST, "params": {}});
    ws.send(Message::Text(request.to_string().into()))
        .await
        .unwrap();
    let reply = loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Message::Text(text) = msg {
            let value: Value = serde_json::from_str(text.as_str()).unwrap();
            if value["id"] == 1 {
                break value;
            }
        }
    };
    assert_eq!(reply["error"]["data"]["kind"], "forbidden");
}
