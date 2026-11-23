use std::sync::Arc;

use asterism_plugin::protocol::{
    method as plugin_method, ProviderCreateParams, ProviderCreateResult, ProviderListResult,
    ProviderParams, ProviderResumeResult, ProviderVmParams,
};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{
    ActivityKind, ProviderMachineChoice, ProviderVm, ProviderVmChoice, ProviderVmState,
    SettingType, TaskCreateParams, TaskCreateResult,
};
use serde_json::Value;

use super::{activity, Daemon};
use crate::error::{Error, Result};
use crate::machines::install;
use crate::machines::PROVISION_READY_TIMEOUT;
use crate::plugins::settings;
use crate::store::StoredProvider;

/// What a failed provisioning has to undo.
#[derive(Default)]
struct Undo {
    pending: Option<i64>,
    /// Ref and destination of a VM this provisioning created.
    created: Option<(String, String)>,
    machine_id: Option<i64>,
}

impl Daemon {
    /// Provisions the chosen VM as a machine, then creates the task on it; a failure leaves neither behind.
    pub(super) async fn create_provider_task(
        self: &Arc<Self>,
        choice: ProviderMachineChoice,
        params: TaskCreateParams,
    ) -> Result<TaskCreateResult> {
        let first = match choice.vm {
            ProviderVmChoice::New { .. } => "Create VM",
            ProviderVmChoice::Existing { .. } => "Start VM",
        };
        let activity = self.start_activity(
            ActivityKind::CreateTask,
            params.title.clone(),
            params.activity_token.clone(),
            &[first, "Accept host key", "Connect"],
        );
        let guard = activity::ActivityGuard {
            daemon: self,
            id: activity,
        };
        let machine_id = match self.provision_machine(&choice, Some(activity)).await {
            Ok(id) => id,
            Err(e) => {
                self.fail_activity_step(activity, e.message.clone(), true);
                return Err(e);
            }
        };
        drop(guard);
        let params = TaskCreateParams {
            machine_id: Some(machine_id),
            provider_machine: None,
            ..params
        };
        let created = self.create_remote_task(machine_id, params).await;
        if created.is_err() {
            // An agent that failed to start leaves its task behind, and the machine stays with it.
            let tasks = self.store().machine_task_count(machine_id);
            match tasks {
                Ok(0) => self.undo_provisioned(&choice, machine_id).await,
                Ok(_) => {}
                Err(e) => eprintln!("asterismd: machine {machine_id}: counting tasks failed: {e}"),
            }
        }
        created
    }

    /// Adds the chosen provider VM as a machine and waits until it is ready; returns the machine id.
    pub async fn provision_machine(
        self: &Arc<Self>,
        choice: &ProviderMachineChoice,
        activity: Option<i64>,
    ) -> Result<i64> {
        let mut undo = Undo::default();
        let provisioned = self.provision(choice, activity, &mut undo).await;
        if provisioned.is_err() {
            self.undo(choice, undo).await;
        }
        provisioned
    }

    async fn provision(
        self: &Arc<Self>,
        choice: &ProviderMachineChoice,
        activity: Option<i64>,
        undo: &mut Undo,
    ) -> Result<i64> {
        let step = || {
            if let Some(id) = activity {
                self.advance_activity(id, None);
            }
        };
        let provider = &choice.provider;
        let (name, destination, stored) = match &choice.vm {
            ProviderVmChoice::New {
                name,
                options,
                delete_with_tasks,
            } => {
                let name = name.trim();
                self.check_new_vm(provider, name, options).await?;
                undo.pending = Some(self.store().add_pending_vm(provider, name)?);
                let created: ProviderCreateResult = match self
                    .provider_call(
                        provider,
                        plugin_method::MACHINE_PROVIDER_CREATE,
                        ProviderCreateParams {
                            provider: provider.clone(),
                            name: name.to_string(),
                            options: options.clone(),
                        },
                        None,
                    )
                    .await
                {
                    Ok(created) => created,
                    Err(e) => {
                        // The VM may exist anyway; the startup sweep finds it by its unique name.
                        undo.pending = None;
                        return Err(e);
                    }
                };
                undo.created = Some((created.vm_ref.clone(), created.destination.clone()));
                let stored = StoredProvider {
                    provider: provider.clone(),
                    vm_ref: created.vm_ref,
                    owned: true,
                    delete_with_tasks: *delete_with_tasks,
                    hibernated: false,
                };
                (name.to_string(), created.destination, stored)
            }
            ProviderVmChoice::Existing { vm_ref } => {
                let vm = self
                    .provider_list(provider)
                    .await?
                    .vms
                    .into_iter()
                    .find(|vm| &vm.vm_ref == vm_ref)
                    .ok_or_else(|| {
                        Error::new(
                            ErrorKind::NotFound,
                            format!("{provider} has no VM {vm_ref:?}"),
                        )
                    })?;
                let destination = self.start_vm(provider, &vm).await?;
                let stored = StoredProvider {
                    provider: provider.clone(),
                    vm_ref: vm.vm_ref,
                    owned: false,
                    delete_with_tasks: false,
                    hibernated: false,
                };
                (vm.name, destination, stored)
            }
        };
        step();
        install::trust_host_key(&self.options.ssh_bin, &destination, PROVISION_READY_TIMEOUT)
            .await
            .map_err(|e| Error::new(ErrorKind::MachineOffline, e.to_string()))?;
        step();
        let machine = self.machines().add_provided(&name, &destination, stored)?;
        undo.machine_id = Some(machine.id);
        if let Some(id) = undo.pending.take() {
            self.store().remove_pending_vm(id)?;
        }
        self.machines()
            .wait_ready(machine.id, PROVISION_READY_TIMEOUT)
            .await?;
        step();
        Ok(machine.id)
    }

    async fn provider_list(&self, provider: &str) -> Result<ProviderListResult> {
        self.provider_call(
            provider,
            plugin_method::MACHINE_PROVIDER_LIST,
            ProviderParams {
                provider: provider.to_string(),
            },
            Some(self.call_timeout()),
        )
        .await
    }

    async fn check_new_vm(
        &self,
        provider: &str,
        name: &str,
        options: &serde_json::Map<String, Value>,
    ) -> Result<()> {
        if name.is_empty() {
            return Err(Error::new(ErrorKind::InvalidParams, "VM name is empty"));
        }
        if self.store().machines()?.iter().any(|m| m.name == name) {
            return Err(Error::new(
                ErrorKind::InvalidParams,
                format!("a machine named {name} already exists"),
            ));
        }
        let listed = self.provider_list(provider).await?;
        // Pending VMs are deleted by name, so the name must not match a VM Asterism did not create.
        if listed.vms.iter().any(|vm| vm.name == name) {
            return Err(Error::new(
                ErrorKind::InvalidParams,
                format!("{provider} already has a VM named {name}"),
            ));
        }
        let specs: Vec<_> = listed
            .create_options
            .into_iter()
            .filter(|spec| spec.kind != SettingType::Secret)
            .collect();
        if let Some(missing) = specs.iter().find(|spec| {
            spec.required
                && spec.default.is_none()
                && options.get(&spec.key).is_none_or(Value::is_null)
        }) {
            return Err(Error::new(
                ErrorKind::InvalidParams,
                format!("{} is required", missing.title),
            ));
        }
        for (key, value) in options {
            let spec = specs.iter().find(|s| &s.key == key).ok_or_else(|| {
                Error::new(
                    ErrorKind::InvalidParams,
                    format!("{provider} has no create option {key:?}"),
                )
            })?;
            settings::check(spec, value)?;
        }
        Ok(())
    }

    /// Deletes a provider VM; one the provider no longer knows counts as deleted.
    pub(crate) async fn delete_vm(&self, provider: &str, vm_ref: &str) -> Result<()> {
        let deleted: Result<Value> = self
            .provider_call(
                provider,
                plugin_method::MACHINE_PROVIDER_DELETE,
                ProviderVmParams {
                    provider: provider.to_string(),
                    vm_ref: vm_ref.to_string(),
                },
                None,
            )
            .await;
        match deleted {
            // A missing provider is NotFound too, and must not count as a deleted VM.
            Err(e)
                if e.kind == ErrorKind::NotFound
                    && self
                        .plugin_set()
                        .registry
                        .machine_provider(provider)
                        .is_some() =>
            {
                Ok(())
            }
            other => other.map(drop),
        }
    }

    /// Resumes a hibernated or stopped VM; returns where it is reachable.
    async fn start_vm(&self, provider: &str, vm: &ProviderVm) -> Result<String> {
        if !matches!(
            vm.state,
            ProviderVmState::Hibernated | ProviderVmState::Stopped
        ) {
            return Ok(vm.destination.clone());
        }
        let resumed: ProviderResumeResult = self
            .provider_call(
                provider,
                plugin_method::MACHINE_PROVIDER_RESUME,
                ProviderVmParams {
                    provider: provider.to_string(),
                    vm_ref: vm.vm_ref.clone(),
                },
                None,
            )
            .await?;
        Ok(resumed.destination)
    }

    async fn undo_provisioned(&self, choice: &ProviderMachineChoice, machine_id: i64) {
        let stored = self.store().machine(machine_id);
        let created = match stored {
            Ok(m) => m.and_then(|m| {
                m.provider
                    .filter(|p| p.owned)
                    .map(|p| (p.vm_ref, m.destination))
            }),
            Err(e) => {
                eprintln!("asterismd: machine {machine_id}: reading it for cleanup failed: {e}");
                None
            }
        };
        let undo = Undo {
            pending: None,
            created,
            machine_id: Some(machine_id),
        };
        self.undo(choice, undo).await;
    }

    /// Best effort: the caller reports the original error, so cleanup failures are only logged.
    async fn undo(&self, choice: &ProviderMachineChoice, undo: Undo) {
        let provider = &choice.provider;
        if let Some(id) = undo.machine_id {
            if let Err(e) = self.machines().remove(id) {
                eprintln!(
                    "asterismd: removing machine {id} after a failed provisioning failed: {e}"
                );
            }
        }
        let mut deleted = true;
        if let Some((vm_ref, destination)) = undo.created {
            install::forget_host_key(
                &self.options.ssh_bin,
                &self.options.keygen_bin,
                &destination,
            )
            .await;
            if let Err(e) = self.delete_vm(provider, &vm_ref).await {
                eprintln!("asterismd: {provider}: deleting VM {vm_ref} after a failed provisioning failed: {e}");
                deleted = false;
            }
        }
        let store = self.store();
        if let Some(id) = undo.pending {
            if let Err(e) = store.remove_pending_vm(id) {
                eprintln!("asterismd: removing pending VM {id} failed: {e}");
            }
        }
        // Leaves the VM to the startup sweep, which deletes it by name.
        if let (false, ProviderVmChoice::New { name, .. }) = (deleted, &choice.vm) {
            if let Err(e) = store.add_pending_vm(provider, name.trim()) {
                eprintln!("asterismd: recording VM {name} for deletion failed: {e}");
            }
        }
    }

    /// Deletes VMs whose creation was interrupted by a crash; rows of unreachable providers are kept.
    pub async fn cleanup_pending_vms(self: Arc<Self>) {
        let pending = match self.store().pending_vms() {
            Ok(pending) => pending,
            Err(e) => {
                eprintln!("asterismd: reading pending VMs failed: {e}");
                return;
            }
        };
        for (id, provider, name) in pending {
            let vms = match self.provider_list(&provider).await {
                Ok(list) => list.vms,
                Err(e) => {
                    eprintln!("asterismd: {provider}: keeping pending VM {name}: {e}");
                    continue;
                }
            };
            let adopted: Vec<String> = match self.store().machines() {
                Ok(machines) => machines
                    .into_iter()
                    .filter_map(|m| m.provider)
                    .filter(|p| p.provider == provider)
                    .map(|p| p.vm_ref)
                    .collect(),
                Err(e) => {
                    eprintln!("asterismd: reading machines failed: {e}");
                    continue;
                }
            };
            let mut all_deleted = true;
            // A crash between adding the machine and clearing the row leaves a VM that is already a machine.
            for vm in vms
                .iter()
                .filter(|vm| vm.name == name && !adopted.contains(&vm.vm_ref))
            {
                if let Err(e) = self.delete_vm(&provider, &vm.vm_ref).await {
                    eprintln!("asterismd: {provider}: deleting pending VM {name} failed: {e}");
                    all_deleted = false;
                }
            }
            if all_deleted {
                if let Err(e) = self.store().remove_pending_vm(id) {
                    eprintln!("asterismd: removing pending VM {name} failed: {e}");
                }
            }
        }
    }
}
