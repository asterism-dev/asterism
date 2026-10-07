use std::collections::HashMap;
use std::future::Future;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use asterism_proto::rpc::{ErrorKind, Request, Response, RpcError};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot};

use crate::protocol::HOST_PREFIX;

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Result<Value, RpcError>>>>>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

fn closed() -> RpcError {
    RpcError::new(ErrorKind::Internal, "connection to the daemon closed")
}

/// Calls the daemon's host API; cheap to clone into request handlers.
#[derive(Clone)]
pub struct Host {
    out: mpsc::UnboundedSender<String>,
    pending: Pending,
    next_id: Arc<AtomicU64>,
}

impl Host {
    pub async fn call<P: Serialize, R: DeserializeOwned>(
        &self,
        method: &str,
        params: P,
    ) -> Result<R, RpcError> {
        let params = crate::to_value(params)?;
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        let line =
            serde_json::to_string(&Request::new(id, &format!("{HOST_PREFIX}{method}"), params))
                .map_err(|e| RpcError::new(ErrorKind::Internal, e.to_string()))?;
        if self.out.send(line).is_err() {
            lock(&self.pending).remove(&id);
            return Err(closed());
        }
        let value = rx.await.map_err(|_| closed())??;
        serde_json::from_value(value)
            .map_err(|e| RpcError::new(ErrorKind::Internal, format!("unexpected host reply: {e}")))
    }
}

/// Serves daemon requests on stdin/stdout until stdin closes.
pub async fn serve<F, Fut>(handler: F) -> std::io::Result<()>
where
    F: Fn(Host, String, Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, RpcError>> + Send + 'static,
{
    serve_io(tokio::io::stdin(), tokio::io::stdout(), handler).await
}

pub async fn serve_io<R, W, F, Fut>(reader: R, mut writer: W, handler: F) -> std::io::Result<()>
where
    R: AsyncRead + Unpin,
    W: AsyncWrite + Unpin + Send + 'static,
    F: Fn(Host, String, Value) -> Fut + Send + Sync + 'static,
    Fut: Future<Output = Result<Value, RpcError>> + Send + 'static,
{
    let (out, mut out_rx) = mpsc::unbounded_channel::<String>();
    tokio::spawn(async move {
        while let Some(mut line) = out_rx.recv().await {
            line.push('\n');
            if writer.write_all(line.as_bytes()).await.is_err() || writer.flush().await.is_err() {
                break;
            }
        }
    });
    let host = Host {
        out: out.clone(),
        pending: Pending::default(),
        next_id: Arc::new(AtomicU64::new(1)),
    };
    let handler = Arc::new(handler);
    let mut lines = BufReader::new(reader).lines();
    while let Some(line) = lines.next_line().await? {
        let Ok(message) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        if message.get("method").is_some() {
            // Notifications have no id and fail to parse as a request; plugins ignore them.
            let Ok(request) = serde_json::from_value::<Request>(message) else {
                continue;
            };
            let (handler, host, out) = (handler.clone(), host.clone(), out.clone());
            tokio::spawn(async move {
                let id = request.id;
                let response = match handler(host, request.method, request.params).await {
                    Ok(result) => Response::ok(id, result),
                    Err(e) => Response::err(id, e),
                };
                if let Ok(line) = serde_json::to_string(&response) {
                    let _ = out.send(line);
                }
            });
        } else if let Ok(response) = serde_json::from_value::<Response>(message) {
            if let Some(tx) = lock(&host.pending).remove(&response.id) {
                let _ = tx.send(match response.error {
                    Some(e) => Err(e),
                    None => Ok(response.result.unwrap_or(Value::Null)),
                });
            }
        }
    }
    Ok(())
}
