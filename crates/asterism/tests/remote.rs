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
    start_with_tailscale(mobile_dir, None).await
}

async fn start_with_tailscale(
    mobile_dir: Option<PathBuf>,
    tailscale_bin: Option<PathBuf>,
) -> Daemon {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths {
        home: home.path().to_path_buf(),
    };
    let socket = paths.socket();
    let options = asterism_core::daemon::DaemonOptions {
        mobile_dir,
        remote_retry: Duration::from_millis(100),
        tailscale_bin,
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
            ..RemoteConfig::default()
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
        serve: RemoteServe::Manual,
        serve_port: 443,
    };
    set_config(&d, config.clone()).await.unwrap();
    assert_eq!(status(&d).await.config, config);
    let text = std::fs::read_to_string(d.home.path().join("config.toml")).unwrap();
    assert!(text.contains("[remote]"), "{text}");
    assert!(text.contains("serve = \"manual\""), "{text}");
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

use futures_util::{SinkExt, StreamExt};
use std::net::SocketAddr;
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::Message;

type Ws =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

async fn enable(d: &Daemon) -> SocketAddr {
    set_config(
        d,
        RemoteConfig {
            enabled: true,
            bind: "127.0.0.1".into(),
            port: 0,
            ..RemoteConfig::default()
        },
    )
    .await
    .unwrap();
    for _ in 0..100 {
        let s = status(d).await;
        if s.state == RemoteState::Listening {
            return s.address.unwrap().parse().unwrap();
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("remote listener did not start: {:?}", status(d).await);
}

async fn connect(addr: SocketAddr, token: &str) -> Ws {
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    ws.send(Message::Text(json!({ "auth": token }).to_string().into()))
        .await
        .unwrap();
    ws
}

async fn request(ws: &mut Ws, id: u64, method_name: &str, params: Value) -> Value {
    let line = json!({ "jsonrpc": "2.0", "id": id, "method": method_name, "params": params });
    ws.send(Message::Text(line.to_string().into()))
        .await
        .unwrap();
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Message::Text(text) = msg {
            let value: Value = serde_json::from_str(text.as_str()).unwrap();
            if value["id"] == id {
                return value;
            }
        }
    }
}

async fn next_notification(ws: &mut Ws, method_name: &str) -> Value {
    loop {
        let msg = tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .unwrap()
            .unwrap()
            .unwrap();
        if let Message::Text(text) = msg {
            let value: Value = serde_json::from_str(text.as_str()).unwrap();
            if value["method"] == method_name {
                return value;
            }
        }
    }
}

async fn close_code(ws: &mut Ws) -> Option<u16> {
    loop {
        match tokio::time::timeout(Duration::from_secs(5), ws.next())
            .await
            .unwrap()
        {
            Some(Ok(Message::Close(frame))) => return frame.map(|f| u16::from(f.code)),
            Some(Ok(_)) => continue,
            _ => return None,
        }
    }
}

fn hello() -> Value {
    json!({ "proto_version": PROTO_VERSION, "client_kind": "app" })
}

#[tokio::test]
async fn paired_device_uses_rpc_and_receives_events() {
    let d = start().await;
    let addr = enable(&d).await;
    let token = token_of(&pair(&d, "phone").await);
    let mut ws = connect(addr, &token).await;
    assert!(request(&mut ws, 1, "hello", hello()).await["result"]["daemon_version"].is_string());
    request(&mut ws, 2, "subscribe", json!({})).await;

    let repo = tempfile::tempdir().unwrap();
    common::init_repo(repo.path());
    let _: Project = d
        .client
        .call(
            method::PROJECT_ADD,
            ProjectAddParams {
                path: repo.path().display().to_string(),
            },
        )
        .await
        .unwrap();
    next_notification(&mut ws, "project.changed").await;
    let list = request(&mut ws, 3, "project.list", json!({})).await;
    assert_eq!(list["result"].as_array().unwrap().len(), 1);

    let devices: Vec<RemoteDevice> = d
        .client
        .call(method::REMOTE_DEVICE_LIST, json!({}))
        .await
        .unwrap();
    assert_eq!(devices[0].connections, 1);
    assert!(devices[0].last_seen_at.is_some());
}

#[tokio::test]
async fn wrong_token_is_rejected() {
    let d = start().await;
    let addr = enable(&d).await;
    let mut ws = connect(addr, "not-a-token").await;
    assert_eq!(close_code(&mut ws).await, Some(4401));
}

#[tokio::test]
async fn foreign_origin_is_rejected_before_upgrade() {
    let d = start().await;
    let addr = enable(&d).await;
    let mut req = format!("ws://{addr}/ws").into_client_request().unwrap();
    req.headers_mut()
        .insert("Origin", "http://evil.example".parse().unwrap());
    let err = tokio_tungstenite::connect_async(req).await.unwrap_err();
    match err {
        tokio_tungstenite::tungstenite::Error::Http(resp) => assert_eq!(resp.status(), 403),
        other => panic!("unexpected {other:?}"),
    }
}

#[tokio::test]
async fn remote_peers_get_forbidden_for_local_only_methods() {
    let d = start().await;
    let addr = enable(&d).await;
    let token = token_of(&pair(&d, "phone").await);
    let mut ws = connect(addr, &token).await;
    for (id, name) in [
        "shutdown",
        "session.hook",
        "remote.device_list",
        "remote.config_set",
    ]
    .iter()
    .enumerate()
    {
        let reply = request(&mut ws, id as u64 + 1, name, json!({})).await;
        assert_eq!(reply["error"]["data"]["kind"], "forbidden", "{name}");
    }
    assert_eq!(status(&d).await.state, RemoteState::Listening);
}

#[tokio::test]
async fn rotate_closes_only_that_device() {
    let d = start().await;
    let addr = enable(&d).await;
    let phone = pair(&d, "phone").await;
    let tablet = pair(&d, "tablet").await;
    let mut phone_ws = connect(addr, &token_of(&phone)).await;
    let mut tablet_ws = connect(addr, &token_of(&tablet)).await;
    request(&mut phone_ws, 1, "hello", hello()).await;
    request(&mut tablet_ws, 1, "hello", hello()).await;

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
    assert_eq!(close_code(&mut phone_ws).await, Some(4401));
    assert!(request(&mut tablet_ws, 2, "hello", hello()).await["result"].is_object());

    let mut stale = connect(addr, &token_of(&phone)).await;
    assert_eq!(close_code(&mut stale).await, Some(4401));
    let mut fresh = connect(addr, &token_of(&rotated)).await;
    assert!(request(&mut fresh, 1, "hello", hello()).await["result"].is_object());
}

#[tokio::test]
async fn removed_device_is_disconnected() {
    let d = start().await;
    let addr = enable(&d).await;
    let phone = pair(&d, "phone").await;
    let mut ws = connect(addr, &token_of(&phone)).await;
    request(&mut ws, 1, "hello", hello()).await;
    let _: Value = d
        .client
        .call(
            method::REMOTE_DEVICE_REMOVE,
            RemoteDeviceIdParams {
                device_id: phone.device.id,
            },
        )
        .await
        .unwrap();
    assert_eq!(close_code(&mut ws).await, Some(4401));
}

#[tokio::test]
async fn disabling_closes_remote_connections() {
    let d = start().await;
    let addr = enable(&d).await;
    let mut ws = connect(addr, &token_of(&pair(&d, "phone").await)).await;
    request(&mut ws, 1, "hello", hello()).await;
    set_config(
        &d,
        RemoteConfig {
            enabled: false,
            bind: "127.0.0.1".into(),
            port: 0,
            ..RemoteConfig::default()
        },
    )
    .await
    .unwrap();
    assert_eq!(close_code(&mut ws).await, Some(u16::from(CloseCode::Away)));
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert_eq!(status(&d).await.state, RemoteState::Disabled);
}

#[tokio::test]
async fn bind_failure_is_retried() {
    let d = start().await;
    let taken = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let port = taken.local_addr().unwrap().port();
    set_config(
        &d,
        RemoteConfig {
            enabled: true,
            bind: "127.0.0.1".into(),
            port,
            ..RemoteConfig::default()
        },
    )
    .await
    .unwrap();
    let mut saw_error = false;
    for _ in 0..50 {
        let s = status(&d).await;
        if s.state == RemoteState::Error {
            saw_error = s.error.is_some();
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(saw_error, "{:?}", status(&d).await);
    drop(taken);
    let mut listening = false;
    for _ in 0..100 {
        if status(&d).await.state == RemoteState::Listening {
            listening = true;
            break;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    assert!(listening, "{:?}", status(&d).await);
}

fn http_get(url: String) -> (u16, String, Option<String>, Option<String>) {
    let response = match ureq::get(&url).call() {
        Ok(r) => r,
        Err(ureq::Error::Status(_, r)) => r,
        Err(e) => panic!("{e}"),
    };
    let csp = response
        .header("content-security-policy")
        .map(str::to_string);
    let cache = response.header("cache-control").map(str::to_string);
    let status = response.status();
    (status, response.into_string().unwrap(), csp, cache)
}

#[tokio::test]
async fn serves_mobile_app_with_csp() {
    let assets = tempfile::tempdir().unwrap();
    std::fs::write(assets.path().join("mobile.html"), "<p>mobile</p>").unwrap();
    std::fs::create_dir(assets.path().join("assets")).unwrap();
    std::fs::write(assets.path().join("assets/app.js"), "console.log(1)").unwrap();
    let d = start_with(Some(assets.path().to_path_buf())).await;
    let addr = enable(&d).await;

    let (status, body, csp, cache) =
        tokio::task::spawn_blocking(move || http_get(format!("http://{addr}/")))
            .await
            .unwrap();
    assert_eq!(status, 200);
    assert!(body.contains("mobile"));
    assert!(csp.unwrap().contains("default-src 'self'"));
    assert!(cache.unwrap().contains("no-cache"));
    let (status, body, _, _) =
        tokio::task::spawn_blocking(move || http_get(format!("http://{addr}/assets/app.js")))
            .await
            .unwrap();
    assert_eq!(status, 200);
    assert!(body.contains("console.log"));
}

#[tokio::test]
async fn missing_assets_show_an_error_page() {
    let d = start_with(Some(PathBuf::from("/nonexistent"))).await;
    let addr = enable(&d).await;
    let (status, body, _, _) =
        tokio::task::spawn_blocking(move || http_get(format!("http://{addr}/")))
            .await
            .unwrap();
    assert_eq!(status, 503);
    assert!(body.contains("not installed"));
}

#[tokio::test]
async fn rotation_during_auth_closes_connection() {
    let d = start().await;
    let addr = enable(&d).await;
    let phone = pair(&d, "phone").await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    let _: RemotePairing = d
        .client
        .call(
            method::REMOTE_DEVICE_ROTATE,
            RemoteDeviceIdParams {
                device_id: phone.device.id,
            },
        )
        .await
        .unwrap();
    ws.send(Message::Text(
        json!({ "auth": token_of(&phone) }).to_string().into(),
    ))
    .await
    .unwrap();
    assert_eq!(close_code(&mut ws).await, Some(4401));
}

#[tokio::test]
async fn disabling_during_auth_closes_connection() {
    let d = start().await;
    let addr = enable(&d).await;
    let token = token_of(&pair(&d, "phone").await);
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    set_config(
        &d,
        RemoteConfig {
            enabled: false,
            bind: "127.0.0.1".into(),
            port: 0,
            ..RemoteConfig::default()
        },
    )
    .await
    .unwrap();
    ws.send(Message::Text(json!({ "auth": token }).to_string().into()))
        .await
        .unwrap();
    let code = close_code(&mut ws).await;
    assert!(matches!(code, Some(1001) | Some(4401) | None), "{code:?}");
}

async fn close_code_within(ws: &mut Ws, limit: Duration) -> Option<u16> {
    tokio::time::timeout(limit, async {
        loop {
            match ws.next().await {
                Some(Ok(Message::Close(frame))) => return frame.map(|f| u16::from(f.code)),
                Some(Ok(_)) => continue,
                _ => return None,
            }
        }
    })
    .await
    .expect("socket was not closed")
}

#[tokio::test]
async fn auth_timeout_is_not_unauthorized() {
    let d = start().await;
    let addr = enable(&d).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    let code = close_code_within(&mut ws, Duration::from_secs(10)).await;
    assert_eq!(code, Some(1008));
}

#[tokio::test]
async fn malformed_auth_is_not_unauthorized() {
    let d = start().await;
    let addr = enable(&d).await;
    let (mut ws, _) = tokio_tungstenite::connect_async(format!("ws://{addr}/ws"))
        .await
        .unwrap();
    ws.send(Message::Text("not json".into())).await.unwrap();
    let code = close_code_within(&mut ws, Duration::from_secs(5)).await;
    assert_eq!(code, Some(1008));
}

struct FakeTailscale {
    dir: TempDir,
}

impl FakeTailscale {
    fn new(cert_domains: &str) -> Self {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("log");
        let status = format!(
            r#"{{"Self":{{"DNSName":"mac.example.ts.net."}},"CertDomains":{cert_domains}}}"#
        );
        let script = format!(
            "#!/bin/sh\necho \"$*\" >> '{}'\nif [ \"$1\" = status ]; then echo '{status}'; fi\n",
            log.display()
        );
        let bin = dir.path().join("tailscale");
        std::fs::write(&bin, script).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        Self { dir }
    }

    fn bin(&self) -> PathBuf {
        self.dir.path().join("tailscale")
    }

    fn calls(&self) -> Vec<String> {
        std::fs::read_to_string(self.dir.path().join("log"))
            .unwrap_or_default()
            .lines()
            .map(str::to_string)
            .collect()
    }
}

fn serve_config(serve: RemoteServe, bind: &str) -> RemoteConfig {
    RemoteConfig {
        enabled: true,
        bind: bind.into(),
        port: 0,
        serve,
        ..RemoteConfig::default()
    }
}

async fn wait_for_state(d: &Daemon, state: RemoteState) -> RemoteStatus {
    for _ in 0..100 {
        let s = status(d).await;
        if s.state == state {
            return s;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!(
        "remote listener never reached {state:?}: {:?}",
        status(d).await
    );
}

#[tokio::test]
async fn managed_serve_is_started_and_stopped() {
    let fake = FakeTailscale::new(r#"["mac.example.ts.net"]"#);
    let d = start_with_tailscale(None, Some(fake.bin())).await;
    set_config(&d, serve_config(RemoteServe::Managed, "127.0.0.1"))
        .await
        .unwrap();
    let s = wait_for_state(&d, RemoteState::Listening).await;
    let bound: SocketAddr = s.address.unwrap().parse().unwrap();
    assert_eq!(
        s.public_url.as_deref(),
        Some("https://mac.example.ts.net:8443")
    );
    let on = format!("serve --bg --https=8443 http://127.0.0.1:{}", bound.port());
    assert!(fake.calls().contains(&on), "{:?}", fake.calls());
    assert!(s.serve_command.unwrap().ends_with(&on));
    assert!(pair(&d, "phone")
        .await
        .pairing_url
        .starts_with("https://mac.example.ts.net:8443/#token="));

    set_config(
        &d,
        RemoteConfig {
            enabled: false,
            ..serve_config(RemoteServe::Managed, "127.0.0.1")
        },
    )
    .await
    .unwrap();
    for _ in 0..100 {
        if fake.calls().iter().any(|c| c == "serve --https=8443 off") {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("serve was not stopped: {:?}", fake.calls());
}

#[tokio::test]
async fn manual_serve_only_shows_the_command() {
    let fake = FakeTailscale::new(r#"["mac.example.ts.net"]"#);
    let d = start_with_tailscale(None, Some(fake.bin())).await;
    set_config(&d, serve_config(RemoteServe::Manual, "127.0.0.1"))
        .await
        .unwrap();
    let s = wait_for_state(&d, RemoteState::Listening).await;
    assert!(s
        .serve_command
        .unwrap()
        .contains("serve --bg --https=8443 http://127.0.0.1:"));
    assert!(pair(&d, "phone")
        .await
        .pairing_url
        .starts_with("https://mac.example.ts.net:8443/#token="));
    assert!(
        !fake.calls().iter().any(|c| c.starts_with("serve")),
        "{:?}",
        fake.calls()
    );
}

#[tokio::test]
async fn serve_listens_on_loopback_whatever_the_bind() {
    let fake = FakeTailscale::new(r#"["mac.example.ts.net"]"#);
    let d = start_with_tailscale(None, Some(fake.bin())).await;
    set_config(&d, serve_config(RemoteServe::Manual, "0.0.0.0"))
        .await
        .unwrap();
    let s = wait_for_state(&d, RemoteState::Listening).await;
    assert!(s.address.unwrap().starts_with("127.0.0.1:"));
    assert_eq!(s.config.bind, "0.0.0.0");
}

#[tokio::test]
async fn serve_needs_https_certificates() {
    let fake = FakeTailscale::new("null");
    let d = start_with_tailscale(None, Some(fake.bin())).await;
    set_config(&d, serve_config(RemoteServe::Managed, "127.0.0.1"))
        .await
        .unwrap();
    let s = wait_for_state(&d, RemoteState::Error).await;
    assert!(
        s.error
            .unwrap()
            .contains("HTTPS certificates are not enabled"),
        "{:?}",
        status(&d).await
    );
    assert!(!fake.calls().iter().any(|c| c.starts_with("serve")));
}

#[tokio::test]
async fn managed_serve_without_tailscale_is_an_error() {
    let d = start_with_tailscale(None, Some(PathBuf::from("/nonexistent/tailscale"))).await;
    set_config(&d, serve_config(RemoteServe::Managed, "127.0.0.1"))
        .await
        .unwrap();
    let s = wait_for_state(&d, RemoteState::Error).await;
    assert!(s.error.unwrap().contains("/nonexistent/tailscale"));
    assert!(s.address.is_none());
}
