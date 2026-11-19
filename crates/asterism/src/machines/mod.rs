pub mod conn;
pub mod install;
mod link;
pub(crate) mod peer;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use asterism_proto::client::Client;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{
    method, Event, Machine, MachineSessionRef, MachineStatus, MachineTestResult,
    MachineUpdatePreview, Role, Session, SessionListParams, SessionStatus,
};
use tokio::task::JoinHandle;

use crate::daemon::Daemon;
use crate::error::{Error, Result};
use crate::lock;
use crate::store::StoredMachine;
use conn::{Settings, Shared};

struct Live {
    shared: Arc<Shared>,
    task: JoinHandle<()>,
}

/// Registered machines and one supervised connection task per machine.
pub struct Machines {
    daemon: Weak<Daemon>,
    role: Role,
    settings: Arc<Settings>,
    live: Mutex<HashMap<i64, Live>>,
}

impl Machines {
    pub(crate) fn new(daemon: Weak<Daemon>, role: Role, settings: Settings) -> Self {
        Self {
            daemon,
            role,
            settings: Arc::new(settings),
            live: Mutex::new(HashMap::new()),
        }
    }

    fn daemon(&self) -> Result<Arc<Daemon>> {
        self.daemon
            .upgrade()
            .ok_or_else(|| Error::new(ErrorKind::Internal, "the daemon is shutting down"))
    }

    /// Only a node opens ssh connections; a machine daemon must never reach further hosts.
    fn require_node(&self) -> Result<()> {
        if self.role != Role::Node {
            return Err(Error::new(
                ErrorKind::Forbidden,
                "machines can only be managed from a node",
            ));
        }
        Ok(())
    }

    pub fn list(&self) -> Result<Vec<Machine>> {
        let stored = self.daemon()?.store().machines()?;
        let live = lock(&self.live);
        Ok(stored
            .into_iter()
            .map(|m| match live.get(&m.id) {
                Some(l) => l.shared.machine(),
                None => machine(m),
            })
            .collect())
    }

    pub fn add(&self, name: &str, destination: &str) -> Result<Machine> {
        self.require_node()?;
        let name = name.trim();
        if name.is_empty() {
            return Err(Error::new(ErrorKind::InvalidParams, "name is empty"));
        }
        check_destination(destination)?;
        let stored = {
            let daemon = self.daemon()?;
            let store = daemon.store();
            if store.machines()?.iter().any(|m| m.name == name) {
                return Err(Error::new(
                    ErrorKind::InvalidParams,
                    format!("a machine named {name} already exists"),
                ));
            }
            store.add_machine(name, destination)?
        };
        Ok(self.connect(stored))
    }

    pub fn start_stored(&self) -> Result<()> {
        self.require_node()?;
        let stored = self.daemon()?.store().machines()?;
        for m in stored {
            self.connect(m);
        }
        Ok(())
    }

    fn connect(&self, stored: StoredMachine) -> Machine {
        let shared = Arc::new(Shared::new(self.settings.clone(), machine(stored)));
        shared.publish();
        let task = tokio::spawn(conn::run(shared.clone()));
        let m = shared.machine();
        if let Some(old) = lock(&self.live).insert(m.id, Live { shared, task }) {
            old.shared.close();
            old.task.abort();
        }
        m
    }

    pub fn remove(&self, id: i64) -> Result<()> {
        let (active, name) = {
            let daemon = self.daemon()?;
            let store = daemon.store();
            let name = store
                .machine(id)?
                .map_or_else(|| id.to_string(), |m| m.name);
            (store.machine_active_tasks(id)?, name)
        };
        if !active.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidParams,
                format!("machine {name} still has {} active tasks", active.len()),
            ));
        }
        if !self.daemon()?.store().remove_machine(id)? {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("no machine with id {id}"),
            ));
        }
        // Aborting drops the task's ssh children, which are killed on drop.
        if let Some(live) = lock(&self.live).remove(&id) {
            live.shared.close();
            live.task.abort();
        }
        let _ = self
            .settings
            .events
            .send(Event::MachineRemoved { machine_id: id });
        Ok(())
    }

    pub fn retry(&self, id: i64) -> Result<()> {
        self.require_node()?;
        match lock(&self.live).get(&id) {
            Some(live) => {
                live.shared.retry();
                Ok(())
            }
            None => Err(Error::new(
                ErrorKind::NotFound,
                format!("no connection for machine {id}"),
            )),
        }
    }

    pub async fn test(&self, destination: &str) -> Result<MachineTestResult> {
        self.require_node()?;
        check_destination(destination)?;
        let platform = conn::test(&self.settings, destination)
            .await
            .map_err(|failure| Error::new(ErrorKind::Internal, failure.message()))?;
        Ok(MachineTestResult { platform })
    }

    /// Which of the machine's running sessions an update resumes and which it ends.
    pub async fn update_preview(&self, id: i64) -> Result<MachineUpdatePreview> {
        self.require_node()?;
        let sessions: Vec<Session> = self
            .call(
                id,
                method::SESSION_LIST,
                SessionListParams { task_id: None },
            )
            .await?;
        let daemon = self.daemon()?;
        let store = daemon.store();
        let mut preview = MachineUpdatePreview::default();
        for session in sessions {
            // A hibernated session is not running, so a restart neither resumes nor ends it.
            if matches!(
                session.status,
                SessionStatus::Exited | SessionStatus::Hibernated
            ) {
                continue;
            }
            let task = match store.task_by_remote(id, session.task_id)? {
                Some(task_id) => store.task(task_id)?,
                None => None,
            };
            let entry = MachineSessionRef {
                session_id: store.session_by_remote(id, session.id)?.unwrap_or(0),
                task_id: task.as_ref().map_or(0, |t| t.id),
                task_title: task.map(|t| t.title).unwrap_or_default(),
                kind: session.kind,
            };
            if session.resumable {
                preview.resume.push(entry);
            } else {
                preview.end.push(entry);
            }
        }
        Ok(preview)
    }

    /// Restarts the machine daemon on the current build; returns once the machine is ready on it.
    pub async fn update(&self, id: i64) -> Result<()> {
        self.require_node()?;
        self.shared(id)?.request_update().await
    }

    /// The machine's connection while it is ready.
    pub fn client(&self, id: i64) -> Option<Arc<Client>> {
        lock(&self.live).get(&id)?.shared.client()
    }
}

fn machine(stored: StoredMachine) -> Machine {
    Machine {
        id: stored.id,
        name: stored.name,
        destination: stored.destination,
        created_at: stored.created_at,
        status: MachineStatus::default(),
    }
}

/// ssh would read a leading `-` as an option, e.g. `-oProxyCommand=…`.
fn check_destination(destination: &str) -> Result<()> {
    if destination.is_empty()
        || destination.starts_with('-')
        || destination
            .chars()
            .any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(Error::new(
            ErrorKind::InvalidParams,
            format!("invalid ssh destination {destination:?}"),
        ));
    }
    Ok(())
}
