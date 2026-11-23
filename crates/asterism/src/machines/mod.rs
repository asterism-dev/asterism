pub mod conn;
pub mod install;
mod lifecycle;
mod link;
pub(crate) mod peer;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};
use std::time::Duration;

use asterism_proto::client::Client;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{
    method, Event, Machine, MachineForgetResult, MachineProvider, MachineSessionRef, MachineState,
    MachineStatus, MachineTestResult, MachineUpdatePreview, Role, Session, SessionListParams,
    SessionStatus, Task,
};
use tokio::sync::broadcast::error::RecvError;
use tokio::task::JoinHandle;

use crate::daemon::Daemon;
use crate::error::{Error, Result};
use crate::lock;
use crate::store::{StoredMachine, StoredProvider};
use conn::{Settings, Shared};

/// How long a newly provided VM may take to accept ssh and finish the machine install.
pub const PROVISION_READY_TIMEOUT: Duration = Duration::from_secs(10 * 60);

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
    lifecycle: Mutex<HashMap<i64, Arc<tokio::sync::Mutex<()>>>>,
}

impl Machines {
    pub(crate) fn new(daemon: Weak<Daemon>, role: Role, settings: Settings) -> Self {
        Self {
            daemon,
            role,
            settings: Arc::new(settings),
            live: Mutex::new(HashMap::new()),
            lifecycle: Mutex::new(HashMap::new()),
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
        self.insert(name, destination, None)
    }

    pub fn add_provided(
        &self,
        name: &str,
        destination: &str,
        p: StoredProvider,
    ) -> Result<Machine> {
        self.insert(name, destination, Some(p))
    }

    fn insert(
        &self,
        name: &str,
        destination: &str,
        provider: Option<StoredProvider>,
    ) -> Result<Machine> {
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
            match &provider {
                Some(p) => store.add_provided_machine(name, destination, p)?,
                None => store.add_machine(name, destination)?,
            }
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

    pub fn set_delete_with_tasks(&self, id: i64, on: bool) -> Result<Machine> {
        self.require_node()?;
        let daemon = self.daemon()?;
        let store = daemon.store();
        let owned = store
            .machine(id)?
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("no machine with id {id}")))?
            .provider
            .is_some_and(|p| p.owned);
        if !owned {
            return Err(Error::new(
                ErrorKind::InvalidParams,
                "only machines created by a provider can be deleted with their tasks",
            ));
        }
        store.set_machine_delete_with_tasks(id, on)?;
        match lock(&self.live).get(&id) {
            Some(live) => Ok(live.shared.set_delete_with_tasks(on)),
            None => Err(Error::new(
                ErrorKind::NotFound,
                format!("no connection for machine {id}"),
            )),
        }
    }

    fn connect(&self, stored: StoredMachine) -> Machine {
        let hibernated = stored.provider.as_ref().is_some_and(|p| p.hibernated);
        let mut initial = machine(stored);
        if hibernated {
            initial.status.state = MachineState::Hibernated;
        }
        let shared = Arc::new(Shared::new(self.settings.clone(), initial));
        shared.publish();
        let task = if hibernated {
            tokio::spawn(std::future::pending())
        } else {
            tokio::spawn(conn::run(shared.clone()))
        };
        let m = shared.machine();
        if let Some(old) = lock(&self.live).insert(m.id, Live { shared, task }) {
            old.shared.close();
            old.task.abort();
        }
        m
    }

    fn remove_row_and_connection(&self, id: i64) -> Result<()> {
        let daemon = self.daemon()?;
        let cleared = {
            let store = daemon.store();
            let pointing: Vec<i64> = store
                .projects()?
                .into_iter()
                .filter(|p| p.default_machine_id == Some(id))
                .map(|p| p.id)
                .collect();
            if !store.remove_machine(id)? {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("no machine with id {id}"),
                ));
            }
            pointing
                .into_iter()
                .filter_map(|p| store.project(p).transpose())
                .collect::<rusqlite::Result<Vec<_>>>()?
        };
        for project in cleared {
            daemon.emit(Event::ProjectChanged(project));
        }
        // Aborting drops the task's ssh children, which are killed on drop.
        if let Some(live) = lock(&self.live).remove(&id) {
            live.shared.close();
            live.task.abort();
        }
        Ok(())
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
        self.remove_row_and_connection(id)?;
        let _ = self
            .settings
            .events
            .send(Event::MachineRemoved { machine_id: id });
        Ok(())
    }

    /// Removes a machine that is gone for good: its active tasks are archived on the hub without asking it.
    pub async fn forget(&self, id: i64) -> Result<MachineForgetResult> {
        let daemon = self.daemon()?;
        let active = {
            let store = daemon.store();
            if store.machine(id)?.is_none() {
                return Err(Error::new(
                    ErrorKind::NotFound,
                    format!("no machine with id {id}"),
                ));
            }
            store.machine_active_tasks(id)?
        };
        self.remove_row_and_connection(id)?;
        let _ = self
            .settings
            .events
            .send(Event::MachineRemoved { machine_id: id });
        // With the row gone, archiving finds no machine to ask and archives locally.
        for task_id in &active {
            daemon.archive_task(*task_id).await?;
        }
        Ok(MachineForgetResult {
            archived_task_ids: active,
        })
    }

    pub fn tasks(&self, id: i64) -> Result<Vec<Task>> {
        let daemon = self.daemon()?;
        let store = daemon.store();
        store
            .machine_active_tasks(id)?
            .into_iter()
            .filter_map(|task_id| store.task(task_id).transpose())
            .collect::<std::result::Result<_, _>>()
            .map_err(Into::into)
    }

    pub async fn retry(&self, id: i64) -> Result<()> {
        self.require_node()?;
        self.wake(id).await?;
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
        self.wake(id).await?;
        let shared = self.shared(id)?;
        let listed = match self
            .call::<_, Vec<Session>>(
                id,
                method::SESSION_LIST,
                SessionListParams { task_id: None },
            )
            .await
        {
            Ok(sessions) => Some(sessions),
            Err(_) => conn::sessions_without_hello(&shared).await,
        };
        let daemon = self.daemon()?;
        let store = daemon.store();
        let mut entries = Vec::new();
        let Some(sessions) = listed else {
            // Unreachable: the hub's own mirrors, with unknown resumability.
            for task_id in store.machine_active_tasks(id)? {
                let Some(task) = store.task(task_id)? else {
                    continue;
                };
                for stored in store.sessions(Some(task_id))? {
                    if running(stored.session.status) {
                        let entry = MachineSessionRef {
                            session_id: stored.session.id,
                            task_id,
                            task_title: task.title.clone(),
                            kind: stored.session.kind,
                        };
                        entries.push((entry, None));
                    }
                }
            }
            return Ok(split_preview(entries, false));
        };
        for session in sessions {
            if !running(session.status) {
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
            entries.push((entry, session.resumable));
        }
        Ok(split_preview(entries, true))
    }

    /// Restarts the machine daemon on the current build; returns once the machine is ready on it.
    pub async fn update(&self, id: i64) -> Result<()> {
        self.require_node()?;
        self.wake(id).await?;
        self.shared(id)?.request_update().await
    }

    /// Returns once the machine is `Ready`; fails when it is blocked, removed or `timeout` passes.
    pub async fn wait_ready(&self, id: i64, timeout: Duration) -> Result<()> {
        let mut events = self.settings.events.subscribe();
        let wait = async {
            let mut machine = self.shared(id)?.machine();
            loop {
                match machine.status.state {
                    MachineState::Ready => return Ok(()),
                    MachineState::Blocked => {
                        return Err(Error::new(
                            ErrorKind::MachineOffline,
                            machine
                                .status
                                .error
                                .unwrap_or_else(|| format!("machine {} is blocked", machine.name)),
                        ))
                    }
                    _ => {}
                }
                machine = match events.recv().await {
                    Ok(Event::MachineChanged(m)) if m.id == id => m,
                    Ok(Event::MachineRemoved { machine_id }) if machine_id == id => {
                        return Err(Error::new(
                            ErrorKind::NotFound,
                            format!("machine {id} was removed"),
                        ))
                    }
                    Ok(_) => continue,
                    Err(RecvError::Lagged(_)) => self.shared(id)?.machine(),
                    Err(RecvError::Closed) => {
                        return Err(Error::new(
                            ErrorKind::Internal,
                            "the daemon is shutting down",
                        ))
                    }
                };
            }
        };
        tokio::time::timeout(timeout, wait).await.map_err(|_| {
            Error::new(
                ErrorKind::Timeout,
                format!(
                    "machine {id} did not connect within {} s",
                    timeout.as_secs()
                ),
            )
        })?
    }

    /// The machine's connection while it is ready.
    pub fn client(&self, id: i64) -> Option<Arc<Client>> {
        lock(&self.live).get(&id)?.shared.client()
    }
}

/// A hibernated session is not running, so a restart neither resumes nor ends it.
fn running(status: SessionStatus) -> bool {
    !matches!(status, SessionStatus::Exited | SessionStatus::Hibernated)
}

/// Sessions of unknown resumability are listed under `end`, which makes the preview inexact.
fn split_preview(
    entries: Vec<(MachineSessionRef, Option<bool>)>,
    from_machine: bool,
) -> MachineUpdatePreview {
    let mut preview = MachineUpdatePreview {
        exact: from_machine,
        ..Default::default()
    };
    for (entry, resumable) in entries {
        match resumable {
            Some(true) => preview.resume.push(entry),
            Some(false) => preview.end.push(entry),
            None => {
                preview.exact = false;
                preview.end.push(entry);
            }
        }
    }
    preview
}

fn machine(stored: StoredMachine) -> Machine {
    Machine {
        id: stored.id,
        name: stored.name,
        destination: stored.destination,
        created_at: stored.created_at,
        status: MachineStatus::default(),
        provider: stored.provider.map(|p| MachineProvider {
            provider: p.provider,
            owned: p.owned,
            delete_with_tasks: p.delete_with_tasks,
        }),
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

#[cfg(test)]
mod tests {
    use asterism_proto::types::{MachineSessionRef, SessionKind};

    use super::split_preview;

    fn entry(session_id: i64) -> MachineSessionRef {
        MachineSessionRef {
            session_id,
            task_id: 1,
            task_title: "t".into(),
            kind: SessionKind::Agent {
                name: "claude".into(),
            },
        }
    }

    #[test]
    fn unknown_resumability_ends_and_is_not_exact() {
        let preview = split_preview(
            vec![
                (entry(1), Some(true)),
                (entry(2), None),
                (entry(3), Some(false)),
            ],
            true,
        );
        assert_eq!(preview.resume, vec![entry(1)]);
        assert_eq!(preview.end, vec![entry(2), entry(3)]);
        assert!(!preview.exact);
        let known = split_preview(vec![(entry(1), Some(true)), (entry(3), Some(false))], true);
        assert!(known.exact);
        assert!(!split_preview(vec![(entry(1), Some(true))], false).exact);
    }
}
