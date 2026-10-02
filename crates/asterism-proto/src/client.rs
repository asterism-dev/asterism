use std::collections::HashMap;
use std::fmt;
use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;
use tokio::sync::{mpsc, oneshot};

use crate::rpc::{Request, Response, RpcError, ServerMessage};
use crate::types::Event;

#[derive(Debug)]
pub enum ClientError {
    Io(io::Error),
    Rpc(RpcError),
    Decode(serde_json::Error),
    Closed,
}

impl fmt::Display for ClientError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Io(e) => write!(f, "{e}"),
            Self::Rpc(e) => write!(f, "{e}"),
            Self::Decode(e) => write!(f, "invalid response: {e}"),
            Self::Closed => write!(f, "connection to the daemon closed"),
        }
    }
}

impl std::error::Error for ClientError {}

impl From<io::Error> for ClientError {
    fn from(e: io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<serde_json::Error> for ClientError {
    fn from(e: serde_json::Error) -> Self {
        Self::Decode(e)
    }
}

type Pending = Arc<Mutex<HashMap<u64, oneshot::Sender<Response>>>>;

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub struct Client {
    next_id: AtomicU64,
    pending: Pending,
    closed: Arc<AtomicBool>,
    outgoing: mpsc::UnboundedSender<String>,
    events: Mutex<Option<mpsc::UnboundedReceiver<Event>>>,
}

impl Client {
    pub async fn connect_unix(path: &Path) -> io::Result<Self> {
        let (reader, writer) = UnixStream::connect(path).await?.into_split();
        Ok(Self::new(reader, writer))
    }

    pub fn new<R, W>(reader: R, mut writer: W) -> Self
    where
        R: AsyncRead + Unpin + Send + 'static,
        W: AsyncWrite + Unpin + Send + 'static,
    {
        let pending: Pending = Arc::default();
        let closed = Arc::new(AtomicBool::new(false));
        let (outgoing, mut outgoing_rx) = mpsc::unbounded_channel::<String>();
        let (event_tx, event_rx) = mpsc::unbounded_channel();

        tokio::spawn(async move {
            while let Some(mut line) = outgoing_rx.recv().await {
                line.push('\n');
                if writer.write_all(line.as_bytes()).await.is_err() || writer.flush().await.is_err() {
                    break;
                }
            }
        });

        let reader_pending = pending.clone();
        let reader_closed = closed.clone();
        tokio::spawn(async move {
            let mut lines = BufReader::new(reader).lines();
            while let Ok(Some(line)) = lines.next_line().await {
                match serde_json::from_str::<ServerMessage>(&line) {
                    Ok(ServerMessage::Response(response)) => {
                        if let Some(tx) = lock(&reader_pending).remove(&response.id) {
                            let _ = tx.send(response);
                        }
                    }
                    Ok(ServerMessage::Notification(notification)) => {
                        if let Some(event) = Event::from_notification(&notification) {
                            let _ = event_tx.send(event);
                        }
                    }
                    Err(_) => {}
                }
            }
            reader_closed.store(true, Ordering::SeqCst);
            lock(&reader_pending).clear();
        });

        Self { next_id: AtomicU64::new(1), pending, closed, outgoing, events: Mutex::new(Some(event_rx)) }
    }

    pub async fn call<P: Serialize, T: DeserializeOwned>(&self, method: &str, params: P) -> Result<T, ClientError> {
        let id = self.next_id.fetch_add(1, Ordering::Relaxed);
        let (tx, rx) = oneshot::channel();
        lock(&self.pending).insert(id, tx);
        if self.closed.load(Ordering::SeqCst) {
            lock(&self.pending).remove(&id);
            return Err(ClientError::Closed);
        }
        let request = Request::new(id, method, serde_json::to_value(params)?);
        self.outgoing.send(serde_json::to_string(&request)?).map_err(|_| ClientError::Closed)?;
        let response = rx.await.map_err(|_| ClientError::Closed)?;
        match (response.result, response.error) {
            (_, Some(error)) => Err(ClientError::Rpc(error)),
            (result, None) => Ok(serde_json::from_value(result.unwrap_or(Value::Null))?),
        }
    }

    /// Hands out the notification stream; only the first caller receives it.
    pub fn take_events(&self) -> Option<mpsc::UnboundedReceiver<Event>> {
        lock(&self.events).take()
    }
}
