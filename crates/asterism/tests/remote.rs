mod common;

use std::path::PathBuf;
use std::time::Duration;

use asterism_core::paths::Paths;
use asterism_proto::client::{Client, ClientError};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{method, *};
use asterism_proto::PROTO_VERSION;
use serde_json::{json, Value};
use tempfile::TempDir;
use tokio::net::UnixStream;

struct Daemon {
    home: TempDir,
    client: Client,
}

async fn start_with(mobile_dir: Option<PathBuf>) -> Daemon {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: home.path().to_path_buf(),
    };
    let socket = paths.socket();
    let options = asterism_core::daemon::DaemonOptions {
        mobile_dir,
        remote_retry: Duration::from_millis(100),
        ..common::daemon_options()
    };
    tokio::spawn(asterism_core::run_with_options(paths, options));
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
    Daemon { home, client }
}

async fn start() -> Daemon {
    start_with(None).await
}

async fn status(d: &Daemon) -> RemoteStatus {
    d.client
        .call(method::REMOTE_STATUS, json!({}))
        .await
        .unwrap()
}

async fn set_config(d: &Daemon, config: RemoteConfig) -> Result<Value, ClientError> {
    d.client
        .call(method::REMOTE_CONFIG_SET, RemoteConfigSetParams { config })
        .await
}

async fn pair(d: &Daemon, name: &str) -> RemotePairing {
    d.client
        .call(
            method::REMOTE_DEVICE_ADD,
            RemoteDeviceAddParams { name: name.into() },
        )
        .await
        .unwrap()
}

fn token_of(pairing: &RemotePairing) -> String {
    pairing
        .pairing_url
        .split("#token=")
        .nth(1)
        .unwrap()
        .to_string()
}

#[tokio::test]
async fn remote_access_is_disabled_by_default() {
    let d = start().await;
    let s = status(&d).await;
    assert_eq!(s.config, RemoteConfig::default());
    assert_eq!(s.state, RemoteState::Disabled);
}

#[tokio::test]
async fn config_rejects_invalid_bind_address() {
    let d = start().await;
    let err = set_config(
        &d,
        RemoteConfig {
            enabled: true,
            bind: "my-mac".into(),
            port: 7420,
        },
    )
    .await
    .unwrap_err();
    assert!(matches!(err, ClientError::Rpc(e) if e.kind() == ErrorKind::InvalidParams));
}

#[tokio::test]
async fn config_is_saved_to_config_toml() {
    let d = start().await;
    let config = RemoteConfig {
        enabled: false,
        bind: "100.100.1.1".into(),
        port: 7999,
    };
    set_config(&d, config.clone()).await.unwrap();
    assert_eq!(status(&d).await.config, config);
    let text = std::fs::read_to_string(d.home.path().join("config.toml")).unwrap();
    assert!(text.contains("[remote]"), "{text}");
}

#[tokio::test]
async fn interfaces_include_loopback() {
    let d = start().await;
    let list: Vec<RemoteInterface> = d
        .client
        .call(method::REMOTE_INTERFACES, json!({}))
        .await
        .unwrap();
    assert!(list.iter().any(|i| i.address == "127.0.0.1"));
    assert!(list.iter().any(|i| i.address == "0.0.0.0"));
}

#[tokio::test]
async fn devices_can_be_added_rotated_and_removed() {
    let d = start().await;
    let phone = pair(&d, "phone").await;
    let tablet = pair(&d, "tablet").await;
    assert_eq!(phone.device.name, "phone");
    assert!(phone
        .pairing_url
        .starts_with("http://127.0.0.1:7420/#token="));

    let rotated: RemotePairing = d
        .client
        .call(
            method::REMOTE_DEVICE_ROTATE,
            RemoteDeviceIdParams {
                device_id: phone.device.id,
            },
        )
        .await
        .unwrap();
    assert_ne!(token_of(&rotated), token_of(&phone));

    let _: Value = d
        .client
        .call(
            method::REMOTE_DEVICE_REMOVE,
            RemoteDeviceIdParams {
                device_id: tablet.device.id,
            },
        )
        .await
        .unwrap();
    let list: Vec<RemoteDevice> = d
        .client
        .call(method::REMOTE_DEVICE_LIST, json!({}))
        .await
        .unwrap();
    assert_eq!(
        list.iter().map(|x| x.name.as_str()).collect::<Vec<_>>(),
        ["phone"]
    );

    let missing = d
        .client
        .call::<_, Value>(
            method::REMOTE_DEVICE_REMOVE,
            RemoteDeviceIdParams { device_id: 999 },
        )
        .await
        .unwrap_err();
    assert!(matches!(missing, ClientError::Rpc(e) if e.kind() == ErrorKind::NotFound));
}

#[tokio::test]
async fn only_the_token_hash_is_stored() {
    let d = start().await;
    let token = token_of(&pair(&d, "phone").await);
    let db = rusqlite::Connection::open(d.home.path().join("state.db")).unwrap();
    let stored: String = db
        .query_row("SELECT token_hash FROM remote_devices", [], |r| r.get(0))
        .unwrap();
    assert_ne!(stored, token);
    assert_eq!(stored, asterism_core::remote::token::hash(&token));
}
