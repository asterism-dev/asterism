use std::sync::Arc;
use std::time::Duration;

use asterism_proto::client::{Client, ClientError};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{
    method, Event, Session, SessionAttachResult, SessionIdParams, SessionListParams, SessionStatus,
};
use base64::engine::general_purpose::STANDARD as BASE64;
use base64::Engine;
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;

use super::conn::Shared;
use super::Machines;
use crate::daemon::Daemon;
use crate::error::{Error, Result};
use crate::lock;

const OUTPUT_QUEUE: usize = 1024;

impl Machines {
    pub(super) fn shared(&self, machine_id: i64) -> Result<Arc<Shared>> {
        lock(&self.live)
            .get(&machine_id)
            .map(|live| live.shared.clone())
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("no machine with id {machine_id}"),
                )
            })
    }

    /// Forwards one call to a ready machine, bounded by `machine_call_timeout`.
    pub async fn call<P: Serialize, T: DeserializeOwned>(
        &self,
        machine_id: i64,
        method: &str,
        params: P,
    ) -> Result<T> {
        let timeout = self.shared(machine_id)?.settings.call_timeout;
        self.call_with_timeout(machine_id, timeout, method, params)
            .await
    }

    /// Like `call`, for work such as a first clone that can outlast `machine_call_timeout`.
    pub async fn call_with_timeout<P: Serialize, T: DeserializeOwned>(
        &self,
        machine_id: i64,
        timeout: Duration,
        method: &str,
        params: P,
    ) -> Result<T> {
        let shared = self.shared(machine_id)?;
        let name = shared.machine().name;
        let client = shared.client().ok_or_else(|| offline(&name))?;
        call(&client, &name, timeout, method, params).await
    }

    /// Output of a remote session; dropping the receiver detaches.
    pub async fn attach_output(
        &self,
        machine_id: i64,
        remote_session_id: i64,
    ) -> Result<(SessionAttachResult, mpsc::Receiver<Vec<u8>>)> {
        let shared = self.shared(machine_id)?;
        let (tx, rx) = mpsc::channel(OUTPUT_QUEUE);
        // Registered first so output right after the snapshot is not lost.
        lock(&shared.outputs)
            .entry(remote_session_id)
            .or_default()
            .push(tx.clone());
        // Every subscriber needs its own snapshot; a repeated remote attach only restarts its forwarder.
        let attached = self
            .call(
                machine_id,
                method::SESSION_ATTACH,
                SessionIdParams {
                    session_id: remote_session_id,
                },
            )
            .await;
        match attached {
            Ok(result) => {
                tokio::spawn(detach_when_dropped(shared, remote_session_id, tx));
                Ok((result, rx))
            }
            Err(e) => {
                unsubscribe(&shared, remote_session_id, &tx);
                Err(e)
            }
        }
    }
}

fn offline(name: &str) -> Error {
    Error::new(
        ErrorKind::MachineOffline,
        format!("machine {name} is offline"),
    )
}

async fn call<P: Serialize, T: DeserializeOwned>(
    client: &Client,
    name: &str,
    timeout: Duration,
    method: &str,
    params: P,
) -> Result<T> {
    match tokio::time::timeout(timeout, client.call(method, params)).await {
        Ok(Ok(result)) => Ok(result),
        Ok(Err(ClientError::Rpc(e))) => Err(Error::new(e.kind(), e.message)),
        Ok(Err(ClientError::Closed | ClientError::Io(_))) => Err(offline(name)),
        Ok(Err(e)) => Err(Error::new(
            ErrorKind::Internal,
            format!("machine {name}: {e}"),
        )),
        Err(_) => Err(Error::new(
            ErrorKind::Timeout,
            format!("machine {name} did not answer {method} within {timeout:?}"),
        )),
    }
}

/// Returns whether that was the session's last subscriber.
fn unsubscribe(shared: &Shared, remote_session_id: i64, tx: &mpsc::Sender<Vec<u8>>) -> bool {
    let mut outputs = lock(&shared.outputs);
    let Some(subscribers) = outputs.get_mut(&remote_session_id) else {
        return false;
    };
    subscribers.retain(|s| !s.same_channel(tx));
    if !subscribers.is_empty() {
        return false;
    }
    outputs.remove(&remote_session_id);
    true
}

async fn detach_when_dropped(
    shared: Arc<Shared>,
    remote_session_id: i64,
    tx: mpsc::Sender<Vec<u8>>,
) {
    tx.closed().await;
    if !unsubscribe(&shared, remote_session_id, &tx) {
        return;
    }
    if let Some(client) = shared.client() {
        let name = shared.machine().name;
        let _ = call::<_, Value>(
            &client,
            &name,
            shared.settings.call_timeout,
            method::SESSION_DETACH,
            SessionIdParams {
                session_id: remote_session_id,
            },
        )
        .await;
    }
}

/// Mirrors one connection's events into the hub until the connection closes.
pub(super) async fn pump(
    shared: Arc<Shared>,
    client: Arc<Client>,
    mut events: mpsc::UnboundedReceiver<Event>,
) {
    let Some(daemon) = shared.settings.daemon.upgrade() else {
        return;
    };
    let machine_id = shared.machine().id;
    // Events that arrive meanwhile queue up and are applied after, so the newest status wins.
    if let Err(e) = reconcile(&daemon, &shared, &client, machine_id).await {
        eprintln!("asterismd: reconciling machine {machine_id}: {}", e.message);
    }
    while let Some(event) = events.recv().await {
        let local = |remote_id: i64| {
            daemon
                .store()
                .session_by_remote(machine_id, remote_id)
                .ok()
                .flatten()
        };
        match event {
            Event::SessionOutput { session_id, data } => {
                let Ok(bytes) = BASE64.decode(data) else {
                    continue;
                };
                if let Some(subscribers) = lock(&shared.outputs).get(&session_id) {
                    // ponytail: a full queue drops the chunk, like local attach's lagging broadcast.
                    for tx in subscribers {
                        let _ = tx.try_send(bytes.clone());
                    }
                }
            }
            Event::SessionStatusChanged { session_id, status } => {
                if let Some(id) = local(session_id) {
                    daemon.apply_session_status(id, status);
                }
            }
            Event::SessionChanged(session) => {
                let stored =
                    local(session.id).and_then(|id| daemon.store().session(id).ok().flatten());
                if let Some(stored) = stored {
                    daemon.emit(Event::SessionChanged(Session {
                        id: stored.session.id,
                        task_id: stored.session.task_id,
                        ..session
                    }));
                }
            }
            Event::SessionRemoved { session_id } => {
                if let Some(id) = local(session_id) {
                    daemon.forget_remote_session(id);
                }
            }
            Event::SubagentStarted {
                session_id,
                subagent,
            } => {
                if let Some(id) = local(session_id) {
                    daemon.emit(Event::SubagentStarted {
                        session_id: id,
                        subagent,
                    });
                }
            }
            Event::SubagentUpdated {
                session_id,
                subagent,
            } => {
                if let Some(id) = local(session_id) {
                    daemon.emit(Event::SubagentUpdated {
                        session_id: id,
                        subagent,
                    });
                }
            }
            // The hub owns task metadata; everything else is about the machine's own state.
            _ => {}
        }
    }
}

/// Brings mirrors up to date after a (re)connect and reattaches open terminals.
async fn reconcile(
    daemon: &Daemon,
    shared: &Shared,
    client: &Client,
    machine_id: i64,
) -> Result<()> {
    let name = shared.machine().name;
    let timeout = shared.settings.call_timeout;
    let tasks = daemon.store().machine_active_tasks(machine_id)?;
    for task_id in tasks {
        let Some((_, remote_task)) = daemon.store().task_remote(task_id)? else {
            continue;
        };
        let remote: Vec<Session> = call(
            client,
            &name,
            timeout,
            method::SESSION_LIST,
            SessionListParams {
                task_id: Some(remote_task),
            },
        )
        .await?;
        let mirrors = daemon.store().sessions(Some(task_id))?;
        for mirror in mirrors {
            let Some(remote_id) = daemon.store().session_remote(mirror.session.id)? else {
                continue;
            };
            let status = remote
                .iter()
                .find(|s| s.id == remote_id)
                .map_or(SessionStatus::Exited, |s| s.status);
            if status != mirror.session.status {
                daemon.apply_session_status(mirror.session.id, status);
            }
        }
    }
    let attached: Vec<i64> = lock(&shared.outputs).keys().copied().collect();
    for remote_id in attached {
        let snapshot = call::<_, SessionAttachResult>(
            client,
            &name,
            timeout,
            method::SESSION_ATTACH,
            SessionIdParams {
                session_id: remote_id,
            },
        )
        .await;
        // A session that is gone shows its end through the status above.
        let Ok(bytes) = snapshot.map(|s| BASE64.decode(s.snapshot).unwrap_or_default()) else {
            continue;
        };
        if let Some(subscribers) = lock(&shared.outputs).get(&remote_id) {
            for tx in subscribers {
                let _ = tx.try_send(bytes.clone());
            }
        }
    }
    Ok(())
}
