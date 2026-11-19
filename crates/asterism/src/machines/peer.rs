//! What a machine may call on the hub over its ssh pipe, and how ids cross it.

use std::sync::Arc;

use asterism_proto::client::Client;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{method, Session, Task, TaskCreateResult};
use serde::de::DeserializeOwned;
use serde::Serialize;
use serde_json::Value;
use tokio::sync::mpsc;

use super::conn::Shared;
use crate::daemon::Daemon;
use crate::error::{Error, Result};
use crate::rpc::{self, Peer};

/// Everything else is `Forbidden`; session reads and writes are served by the machine daemon itself.
const MACHINE_ALLOWED: &[&str] = &[
    method::HELLO,
    method::TASK_LIST,
    method::TASK_CREATE,
    method::TASK_ARCHIVE,
    method::TASK_RESTORE,
    method::TASK_DELETE,
    method::TASK_DIFF,
    method::SESSION_START,
    method::PROJECT_LIST,
    method::PROJECT_BRANCHES,
    method::TASK_SOURCE_GET,
    method::TASK_SOURCE_SEARCH,
    method::REVIEW_GET,
    method::REVIEW_PROMPT,
    method::REVIEW_CHECK_LOG,
    method::PR_LIST,
    method::PR_REFRESH,
];

pub fn machine_allowed(method_name: &str) -> bool {
    MACHINE_ALLOWED.contains(&method_name)
}

/// Serves one call from `machine_id`; task and session ids on the wire are the machine's own.
pub(crate) async fn dispatch(
    daemon: &Arc<Daemon>,
    machine_id: i64,
    method_name: &str,
    params: Value,
) -> Result<Value> {
    if !machine_allowed(method_name) {
        return Err(Error::new(
            ErrorKind::Forbidden,
            format!("{method_name} is not available to machines"),
        ));
    }
    let params = inbound(daemon, machine_id, method_name, params)?;
    let result = rpc::dispatch_method(daemon, method_name, params).await?;
    outbound(daemon, machine_id, method_name, result)
}

fn inbound(daemon: &Daemon, machine_id: i64, method_name: &str, params: Value) -> Result<Value> {
    let mut params = match params {
        Value::Null => Value::Object(Default::default()),
        params => params,
    };
    let Some(fields) = params.as_object_mut() else {
        return Ok(params);
    };
    if method_name == method::TASK_CREATE {
        fields.insert("machine_id".into(), machine_id.into());
    }
    for key in ["task_id", "session_id"] {
        let Some(value) = fields.get_mut(key) else {
            continue;
        };
        let remote_id = value.as_i64().ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidParams,
                format!("{key} must be an integer"),
            )
        })?;
        let store = daemon.store();
        let local = if key == "task_id" {
            store.task_by_remote(machine_id, remote_id)?
        } else {
            store.session_by_remote(machine_id, remote_id)?
        };
        *value = local.ok_or_else(|| not_found(key, remote_id))?.into();
    }
    Ok(params)
}

fn outbound(daemon: &Daemon, machine_id: i64, method_name: &str, result: Value) -> Result<Value> {
    match method_name {
        method::TASK_LIST => {
            let tasks: Vec<Task> = decode(result)?;
            // Tasks still being created have no machine id yet.
            let mine = tasks
                .into_iter()
                .filter(|task| task.machine_id == Some(machine_id))
                .filter_map(|task| remote_task(daemon, machine_id, task).ok())
                .collect::<Vec<_>>();
            encode(mine)
        }
        method::TASK_ARCHIVE | method::TASK_RESTORE => {
            encode(remote_task(daemon, machine_id, decode(result)?)?)
        }
        method::TASK_CREATE => {
            let created: TaskCreateResult = decode(result)?;
            encode(TaskCreateResult {
                task: remote_task(daemon, machine_id, created.task)?,
                session: created
                    .session
                    .map(|session| remote_session(daemon, machine_id, session))
                    .transpose()?,
                warning: created.warning,
            })
        }
        method::SESSION_START => encode(remote_session(daemon, machine_id, decode(result)?)?),
        _ => Ok(result),
    }
}

fn remote_task_id(daemon: &Daemon, machine_id: i64, task_id: i64) -> Result<i64> {
    match daemon.store().task_remote(task_id)? {
        Some((machine, remote_id)) if machine == machine_id => Ok(remote_id),
        _ => Err(not_found("task_id", task_id)),
    }
}

fn remote_task(daemon: &Daemon, machine_id: i64, task: Task) -> Result<Task> {
    Ok(Task {
        id: remote_task_id(daemon, machine_id, task.id)?,
        ..task
    })
}

fn remote_session(daemon: &Daemon, machine_id: i64, session: Session) -> Result<Session> {
    let id = daemon
        .store()
        .session_remote(session.id)?
        .ok_or_else(|| not_found("session_id", session.id))?;
    Ok(Session {
        id,
        task_id: remote_task_id(daemon, machine_id, session.task_id)?,
        ..session
    })
}

fn not_found(key: &str, id: i64) -> Error {
    Error::new(
        ErrorKind::NotFound,
        format!("no {key} {id} on this machine"),
    )
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T> {
    serde_json::from_value(value).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))
}

fn encode<T: Serialize>(value: T) -> Result<Value> {
    serde_json::to_value(value).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))
}

/// Answers the machine's calls on its link until the link or the daemon goes away.
pub(super) async fn serve(
    shared: Arc<Shared>,
    client: Arc<Client>,
    mut requests: mpsc::UnboundedReceiver<String>,
) {
    let Some(daemon) = shared.settings.daemon.upgrade() else {
        return;
    };
    let machine_id = shared.machine().id;
    let (incoming_tx, incoming) = mpsc::channel(rpc::OUT_QUEUE);
    let (out, mut out_rx) = mpsc::channel::<String>(rpc::OUT_QUEUE);
    let feed = async move {
        while let Some(line) = requests.recv().await {
            if incoming_tx.send(line).await.is_err() {
                break;
            }
        }
    };
    let reply = async move {
        while let Some(line) = out_rx.recv().await {
            client.send_raw(line);
        }
    };
    tokio::join!(
        feed,
        rpc::run_conn(daemon, Peer::Machine { machine_id }, incoming, out),
        reply
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn machines_cannot_reach_privileged_methods() {
        for name in [
            method::SHUTDOWN,
            method::SUBSCRIBE,
            method::SESSION_ATTACH,
            method::SESSION_HOOK,
            method::SESSION_SEND,
            method::PROJECT_ADD,
            method::PROJECT_ENSURE,
            method::PLUGIN_SET_SETTINGS,
            method::AGENT_CONFIG_SET,
            method::NODE_CONFIG_SET,
            method::REMOTE_DEVICE_ADD,
            method::MACHINE_ADD,
            method::STORE_ADD,
        ] {
            assert!(!machine_allowed(name), "{name}");
        }
    }
}
