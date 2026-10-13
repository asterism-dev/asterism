use std::net::SocketAddr;
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;

use asterism_proto::types::{RemoteConfig, RemoteState};
use axum::extract::ws::{CloseFrame, Message, WebSocket, WebSocketUpgrade};
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode};
use axum::response::{Html, IntoResponse, Response};
use axum::routing::get;
use axum::Router;
use serde::Deserialize;
use tokio::net::TcpListener;
use tokio::sync::broadcast::{self, error::RecvError};
use tokio::sync::mpsc;
use tower_http::services::{ServeDir, ServeFile};
use tower_http::set_header::SetResponseHeaderLayer;

use crate::daemon::Daemon;
use crate::rpc::{self, Peer, OUT_QUEUE};

pub const UNAUTHORIZED: u16 = 4401;
const GOING_AWAY: u16 = 1001;
const POLICY_VIOLATION: u16 = 1008;
const AUTH_TIMEOUT: Duration = Duration::from_secs(5);
// Slows down guessing; tokens are 256 bits, so this is not the main defence.
const AUTH_FAILURE_DELAY: Duration = Duration::from_secs(1);
const MAX_MESSAGE: usize = 1 << 20;
const CSP: &str =
    "default-src 'self'; connect-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:";
const MISSING_ASSETS: &str =
    "<!doctype html><title>asterism</title><p>The mobile app is not installed with this daemon.</p>";

/// Keeps the remote listener in step with `[remote]`; config changes wake it through `Hub::changed`.
pub async fn run(daemon: Arc<Daemon>) {
    loop {
        let config = match super::load_config(daemon.paths()) {
            Ok(config) => config,
            Err(e) => {
                daemon.remote.set(RemoteState::Error, None, Some(e.message));
                wait(&daemon).await;
                continue;
            }
        };
        if !config.enabled {
            daemon.remote.set(RemoteState::Disabled, None, None);
            daemon.remote.changed.notified().await;
            continue;
        }
        match bind(&config).await {
            Ok(listener) => {
                daemon
                    .remote
                    .set(RemoteState::Listening, listener.local_addr().ok(), None);
                tokio::select! {
                    result = axum::serve(listener, router(daemon.clone())) => {
                        let message = result.err().map_or_else(
                            || "the remote listener stopped".to_string(),
                            |e| e.to_string(),
                        );
                        daemon.remote.set(RemoteState::Error, None, Some(message));
                        wait(&daemon).await;
                    }
                    _ = daemon.remote.changed.notified() => {}
                }
            }
            Err(message) => {
                daemon.remote.set(RemoteState::Error, None, Some(message));
                wait(&daemon).await;
            }
        }
    }
}

async fn wait(daemon: &Daemon) {
    tokio::select! {
        _ = tokio::time::sleep(daemon.remote_retry()) => {}
        _ = daemon.remote.changed.notified() => {}
    }
}

async fn bind(config: &RemoteConfig) -> Result<TcpListener, String> {
    let ip = config
        .bind
        .parse()
        .map_err(|_| format!("invalid bind address {:?}", config.bind))?;
    let addr = SocketAddr::new(ip, config.port);
    TcpListener::bind(addr)
        .await
        .map_err(|e| format!("cannot listen on {addr}: {e}"))
}

fn router(daemon: Arc<Daemon>) -> Router {
    let assets = daemon
        .mobile_dir()
        .filter(|dir| dir.join("mobile.html").is_file())
        .map(Path::to_path_buf);
    let router = Router::new().route("/ws", get(upgrade));
    let router = match assets {
        Some(dir) => router
            // Hashed assets are cacheable, but a stale page would reference missing ones.
            .merge(
                Router::new()
                    .route_service("/", ServeFile::new(dir.join("mobile.html")))
                    .layer(SetResponseHeaderLayer::overriding(
                        header::CACHE_CONTROL,
                        HeaderValue::from_static("no-cache"),
                    )),
            )
            .fallback_service(ServeDir::new(dir)),
        None => {
            router.fallback(|| async { (StatusCode::SERVICE_UNAVAILABLE, Html(MISSING_ASSETS)) })
        }
    };
    router
        .layer(SetResponseHeaderLayer::overriding(
            header::CONTENT_SECURITY_POLICY,
            HeaderValue::from_static(CSP),
        ))
        .with_state(daemon)
}

async fn upgrade(
    State(daemon): State<Arc<Daemon>>,
    headers: HeaderMap,
    ws: WebSocketUpgrade,
) -> Response {
    // Stops a web page open elsewhere on a tailnet device from talking to the daemon.
    if !same_origin(&headers) {
        return StatusCode::FORBIDDEN.into_response();
    }
    // Subscribed before the upgrade so revocations during auth are not missed.
    let closing = daemon.remote.closing();
    ws.max_message_size(MAX_MESSAGE)
        .max_frame_size(MAX_MESSAGE)
        .on_upgrade(move |socket| serve_socket(daemon, socket, closing))
}

/// Browsers always send `Origin` on WebSocket requests; non-browser clients may omit it.
fn same_origin(headers: &HeaderMap) -> bool {
    let Some(origin) = headers.get(header::ORIGIN) else {
        return true;
    };
    let Some(host) = headers.get(header::HOST).and_then(|h| h.to_str().ok()) else {
        return false;
    };
    origin
        .to_str()
        .is_ok_and(|o| o == format!("http://{host}") || o == format!("https://{host}"))
}

#[derive(Deserialize)]
struct Auth {
    auth: String,
}

/// `Err` carries the close code: only a token that was looked up and not found means "unauthorized".
async fn authenticate(daemon: &Daemon, socket: &mut WebSocket) -> Result<i64, u16> {
    let Ok(Some(Ok(Message::Text(text)))) = tokio::time::timeout(AUTH_TIMEOUT, socket.recv()).await
    else {
        return Err(POLICY_VIOLATION);
    };
    let auth: Auth = serde_json::from_str(text.as_str()).map_err(|_| POLICY_VIOLATION)?;
    match super::authenticate(daemon, &auth.auth) {
        Ok(Some(id)) => Ok(id),
        Ok(None) => Err(UNAUTHORIZED),
        Err(e) => {
            eprintln!("asterismd: remote auth failed: {e}");
            Err(POLICY_VIOLATION)
        }
    }
}

async fn close(socket: &mut WebSocket, code: u16, reason: &'static str) {
    let frame = CloseFrame {
        code,
        reason: reason.into(),
    };
    let _ = socket.send(Message::Close(Some(frame))).await;
}

async fn serve_socket(
    daemon: Arc<Daemon>,
    mut socket: WebSocket,
    mut closing: broadcast::Receiver<Option<i64>>,
) {
    let device_id = match authenticate(&daemon, &mut socket).await {
        Ok(id) => id,
        Err(code) => {
            tokio::time::sleep(AUTH_FAILURE_DELAY).await;
            close(&mut socket, code, "authentication failed").await;
            return;
        }
    };
    if !super::load_config(daemon.paths()).is_ok_and(|c| c.enabled) {
        close(&mut socket, GOING_AWAY, "remote access changed").await;
        return;
    }
    daemon.remote.connected(device_id);
    let (lines, incoming) = mpsc::channel::<String>(OUT_QUEUE);
    let (out, mut outgoing) = mpsc::channel::<String>(OUT_QUEUE);
    let conn = tokio::spawn(rpc::run_conn(
        daemon.clone(),
        Peer::Remote { device_id },
        incoming,
        out,
    ));
    loop {
        tokio::select! {
            message = socket.recv() => match message {
                Some(Ok(Message::Text(text))) => {
                    if lines.send(text.to_string()).await.is_err() {
                        break;
                    }
                }
                Some(Ok(Message::Close(_))) | Some(Err(_)) | None => break,
                Some(Ok(_)) => {}
            },
            line = outgoing.recv() => match line {
                Some(line) => {
                    if socket.send(Message::Text(line.into())).await.is_err() {
                        break;
                    }
                }
                None => break,
            },
            target = closing.recv() => match target {
                Ok(Some(id)) if id == device_id => {
                    close(&mut socket, UNAUTHORIZED, "device revoked").await;
                    break;
                }
                Ok(Some(_)) => {}
                // A missed message might have revoked this device, so lagging closes too.
                Ok(None) | Err(RecvError::Lagged(_)) => {
                    close(&mut socket, GOING_AWAY, "remote access changed").await;
                    break;
                }
                Err(RecvError::Closed) => break,
            },
        }
    }
    drop(lines);
    let _ = conn.await;
    daemon.remote.disconnected(device_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn headers(origin: Option<&str>, host: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(header::HOST, host.parse().unwrap());
        if let Some(o) = origin {
            h.insert(header::ORIGIN, o.parse().unwrap());
        }
        h
    }

    #[test]
    fn origin_must_match_host() {
        assert!(same_origin(&headers(None, "100.1.2.3:7420")));
        assert!(same_origin(&headers(
            Some("http://100.1.2.3:7420"),
            "100.1.2.3:7420"
        )));
        assert!(same_origin(&headers(
            Some("https://mac.ts.net"),
            "mac.ts.net"
        )));
        assert!(!same_origin(&headers(
            Some("http://evil.example"),
            "100.1.2.3:7420"
        )));
        assert!(!same_origin(&headers(
            Some("http://100.1.2.3:7421"),
            "100.1.2.3:7420"
        )));
    }
}
