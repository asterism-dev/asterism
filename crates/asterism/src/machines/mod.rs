pub mod conn;
pub mod install;

use std::collections::HashMap;
use std::sync::{Arc, Mutex, Weak};

use asterism_proto::client::Client;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{Event, Machine, MachineStatus, MachineTestResult};
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
    settings: Arc<Settings>,
    live: Mutex<HashMap<i64, Live>>,
}

impl Machines {
    pub(crate) fn new(daemon: Weak<Daemon>, settings: Settings) -> Self {
        Self {
            daemon,
            settings: Arc::new(settings),
            live: Mutex::new(HashMap::new()),
        }
    }

    fn daemon(&self) -> Result<Arc<Daemon>> {
        self.daemon
            .upgrade()
            .ok_or_else(|| Error::new(ErrorKind::Internal, "the daemon is shutting down"))
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

    /// Node role only: a machine daemon must never open connections of its own.
    pub fn start_stored(&self) -> Result<()> {
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
        check_destination(destination)?;
        let platform = conn::test(&self.settings, destination)
            .await
            .map_err(|failure| Error::new(ErrorKind::Internal, failure.message()))?;
        Ok(MachineTestResult { platform })
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
