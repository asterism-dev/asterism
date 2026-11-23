use std::sync::Arc;

use asterism_plugin::protocol::{method as plugin_method, ProviderResumeResult, ProviderVmParams};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::Event;
use serde_json::Value;

use super::{check_destination, install, Machines, PROVISION_READY_TIMEOUT};
use crate::error::{Error, Result};
use crate::lock;
use crate::store::{StoredMachine, StoredProvider};

impl Machines {
    /// Serialises hibernating, waking and deleting one machine's VM.
    fn lifecycle(&self, id: i64) -> Arc<tokio::sync::Mutex<()>> {
        lock(&self.lifecycle).entry(id).or_default().clone()
    }

    /// Hibernates an owned VM once no active task is left on it.
    pub async fn hibernate_if_idle(&self, id: i64) -> Result<()> {
        let lifecycle = self.lifecycle(id);
        let _held = lifecycle.lock().await;
        let daemon = self.daemon()?;
        let (stored, p) = {
            let store = daemon.store();
            let Some(stored) = store.machine(id)? else {
                return Ok(());
            };
            let Some(p) = stored.provider.clone().filter(|p| p.owned && !p.hibernated) else {
                return Ok(());
            };
            if !store.machine_active_tasks(id)?.is_empty() {
                return Ok(());
            }
            (stored, p)
        };
        let _: Value = daemon
            .provider_call(
                &p.provider,
                plugin_method::MACHINE_PROVIDER_HIBERNATE,
                vm_params(&p),
                None,
            )
            .await?;
        daemon.store().set_machine_hibernated(id, true)?;
        // Replaces the live connection with an idle entry shown as `Hibernated`.
        self.connect(StoredMachine {
            provider: Some(StoredProvider {
                hibernated: true,
                ..p
            }),
            ..stored
        });
        Ok(())
    }

    /// Resumes a hibernated VM and returns once its machine is ready again.
    pub async fn wake(&self, id: i64) -> Result<()> {
        let lifecycle = self.lifecycle(id);
        let _held = lifecycle.lock().await;
        let daemon = self.daemon()?;
        let Some(stored) = daemon.store().machine(id)? else {
            return Ok(());
        };
        let Some(p) = stored.provider.clone().filter(|p| p.hibernated) else {
            return Ok(());
        };
        // Not NotFound: callers read that as "the machine is gone" and drop the task.
        let offline = |e: Error| {
            Error::new(
                ErrorKind::MachineOffline,
                format!("waking machine {}: {}", stored.name, e.message),
            )
        };
        let resumed: ProviderResumeResult = daemon
            .provider_call(
                &p.provider,
                plugin_method::MACHINE_PROVIDER_RESUME,
                vm_params(&p),
                None,
            )
            .await
            .map_err(offline)?;
        if resumed.destination != stored.destination {
            check_destination(&resumed.destination).map_err(offline)?;
            // Trusted before it is stored, so a failure here retries the trust on the next wake.
            install::trust_host_key(
                &self.settings.ssh_bin,
                &resumed.destination,
                PROVISION_READY_TIMEOUT,
            )
            .await
            .map_err(|e| Error::new(ErrorKind::MachineOffline, e.to_string()))?;
            daemon
                .store()
                .set_machine_destination(id, &resumed.destination)?;
        }
        daemon.store().set_machine_hibernated(id, false)?;
        self.connect(StoredMachine {
            destination: resumed.destination,
            provider: Some(StoredProvider {
                hibernated: false,
                ..p
            }),
            ..stored
        });
        self.wait_ready(id, PROVISION_READY_TIMEOUT).await
    }

    /// Deletes an owned VM, its `known_hosts` entry and its machine row.
    pub async fn delete_vm(&self, id: i64) -> Result<()> {
        let lifecycle = self.lifecycle(id);
        let _held = lifecycle.lock().await;
        let daemon = self.daemon()?;
        let stored = daemon
            .store()
            .machine(id)?
            .ok_or_else(|| Error::new(ErrorKind::NotFound, format!("no machine with id {id}")))?;
        let p = stored.provider.filter(|p| p.owned).ok_or_else(|| {
            Error::new(
                ErrorKind::InvalidParams,
                format!("machine {} was not created by a provider", stored.name),
            )
        })?;
        daemon.delete_vm(&p.provider, &p.vm_ref).await?;
        install::forget_host_key(
            &self.settings.ssh_bin,
            &daemon.options.keygen_bin,
            &stored.destination,
        )
        .await;
        self.remove_row_and_connection(id)?;
        let _ = self
            .settings
            .events
            .send(Event::MachineRemoved { machine_id: id });
        Ok(())
    }
}

fn vm_params(p: &StoredProvider) -> ProviderVmParams {
    ProviderVmParams {
        provider: p.provider.clone(),
        vm_ref: p.vm_ref.clone(),
    }
}
