use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{
    method, ActivityKind, AgentConfigSetParams, Event, IssueRef, PluginNameParams,
    PluginSetSettingsParams, PluginSettings, Project, ProjectEnsureParams, Session, SessionKind,
    SessionListParams, SessionStartParams, SessionStatus, SessionWaitParams, SessionWaitResult,
    Task, TaskCreateParams, TaskCreateResult, TaskDeleteParams,
};

use super::{activity, issue_name, not_found, slugify, Daemon};
use crate::error::{Error, Result};
use crate::git;
use crate::plugins::settings;
use serde_json::Value;

fn trimmed(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
}

impl Daemon {
    /// Copies the hub's settings for `agent` to the machine, with its machine environment (or the local one, if machines use it) as the environment; secret plugin settings stay behind.
    pub async fn push_agent_settings(self: &Arc<Self>, machine_id: i64, agent: &str) -> Result<()> {
        let plugin = self
            .agent_infos()
            .into_iter()
            .find(|a| a.name == agent)
            .ok_or_else(|| {
                Error::new(
                    ErrorKind::NotFound,
                    format!("agent {agent} is not installed"),
                )
            })?
            .plugin;
        let hub_schema = self.plugin_schema(&plugin)?;
        let hub_values = settings::non_secret_values(self.paths(), &plugin, &hub_schema)?;
        let machine: PluginSettings = self
            .machines()
            .call(
                machine_id,
                method::PLUGIN_SETTINGS,
                PluginNameParams {
                    name: plugin.clone(),
                },
            )
            .await?;
        let values: BTreeMap<String, Value> =
            settings::settings_update(&hub_values, &hub_schema, &machine.schema)
                .into_iter()
                .collect();
        let mut config = self.agent_config(agent)?;
        let machine_env = std::mem::take(&mut config.machine_env);
        if !std::mem::take(&mut config.machine_env_from_local) {
            config.env = machine_env;
        }
        let params = AgentConfigSetParams {
            agent: agent.into(),
            config,
        };
        let () = self
            .machines()
            .call(machine_id, method::AGENT_CONFIG_SET, params)
            .await?;
        let params = PluginSetSettingsParams {
            name: plugin,
            values,
        };
        self.machines()
            .call(machine_id, method::PLUGIN_SET_SETTINGS, params)
            .await
    }

    /// The machine a task lives on and the task's id there; `None` for a local task.
    pub(super) fn remote_task(&self, task_id: i64) -> Result<Option<(i64, i64)>> {
        Ok(self.store().task_remote(task_id)?)
    }

    /// The machine a session runs on and the session's id there; `None` for a local session.
    pub(crate) fn remote_session(&self, session_id: i64) -> Result<Option<(i64, i64)>> {
        let store = self.store();
        let Some(remote_id) = store.session_remote(session_id)? else {
            return Ok(None);
        };
        let Some(stored) = store.session(session_id)? else {
            return Ok(None);
        };
        Ok(store
            .task_remote(stored.session.task_id)?
            .map(|(machine_id, _)| (machine_id, remote_id)))
    }

    /// Fails with `AgentUnavailable` unless the machine reported `agent` as available.
    fn ensure_machine_agent(&self, machine_id: i64, agent: &str) -> Result<()> {
        let machine = self
            .machines()
            .list()?
            .into_iter()
            .find(|m| m.id == machine_id)
            .ok_or_else(|| not_found("machine", machine_id))?;
        if machine
            .status
            .agents
            .iter()
            .any(|a| a.name == agent && a.available)
        {
            Ok(())
        } else {
            Err(Error::new(
                ErrorKind::AgentUnavailable,
                format!("agent {agent} is not available on {}", machine.name),
            ))
        }
    }

    pub(super) async fn start_remote_session(
        self: &Arc<Self>,
        machine_id: i64,
        remote_task: i64,
        task_id: i64,
        params: SessionStartParams,
    ) -> Result<Session> {
        if let SessionKind::Agent { name } = &params.kind {
            self.ensure_machine_agent(machine_id, name)?;
            self.push_agent_settings(machine_id, name).await?;
        }
        let kind = params.kind.clone();
        let remote: Session = self
            .machines()
            .call(
                machine_id,
                method::SESSION_START,
                SessionStartParams {
                    task_id: remote_task,
                    ..params
                },
            )
            .await?;
        let id = self
            .store()
            .insert_remote_session(task_id, remote.id, &kind, remote.status)?;
        // The pump drops status events that beat the insert; events after it are applied, so one re-read closes the gap.
        let now: Result<Vec<Session>> = self
            .machines()
            .call(
                machine_id,
                method::SESSION_LIST,
                SessionListParams {
                    task_id: Some(remote_task),
                },
            )
            .await;
        if let Some(status) = now
            .ok()
            .and_then(|list| list.into_iter().find(|s| s.id == remote.id))
            .map(|s| s.status)
            .filter(|status| *status != remote.status)
        {
            self.apply_session_status(id, status);
        }
        let session = self.session(id)?;
        self.emit(Event::SessionChanged(session.clone()));
        self.touch_task(task_id);
        Ok(session)
    }

    /// `session.wait` on a machine, ending a second before `machine_call_timeout` so the machine's own timeout is what callers see.
    pub(super) async fn wait_remote(
        &self,
        machine_id: i64,
        remote_id: i64,
        params: SessionWaitParams,
    ) -> Result<SessionStatus> {
        let cap = self
            .options
            .machine_call_timeout
            .saturating_sub(std::time::Duration::from_secs(1));
        let wait = params
            .timeout_ms
            .map_or(cap, |ms| std::time::Duration::from_millis(ms).min(cap));
        let result: SessionWaitResult = self
            .machines()
            .call(
                machine_id,
                method::SESSION_WAIT,
                SessionWaitParams {
                    session_id: remote_id,
                    until: params.until,
                    timeout_ms: Some(u64::try_from(wait.as_millis()).unwrap_or(u64::MAX)),
                },
            )
            .await?;
        Ok(result.status)
    }

    /// Drops a machine session's mirror; `session.remove` and the machine's own removal event race, so only the first emits.
    pub(crate) fn forget_remote_session(&self, id: i64) {
        let task_id = {
            let store = self.store();
            let Ok(Some(stored)) = store.session(id) else {
                return;
            };
            if store.delete_session(id).is_err() {
                return;
            }
            stored.session.task_id
        };
        self.track_waiting(id, task_id, SessionStatus::Exited);
        self.emit(Event::SessionRemoved { session_id: id });
    }

    /// `task.create` for a machine: the machine clones origin, creates and pushes the branch; the hub keeps a linked row.
    pub(super) async fn create_remote_task(
        self: &Arc<Self>,
        machine_id: i64,
        params: TaskCreateParams,
    ) -> Result<TaskCreateResult> {
        // A hibernated machine lists no agents until it is awake.
        self.machines().wake(machine_id).await?;
        if let Some(agent) = &params.agent {
            self.ensure_machine_agent(machine_id, agent)?;
        }
        let project = self
            .store()
            .project(params.project_id)?
            .ok_or_else(|| not_found("project", params.project_id))?;
        let origin = git::remote_url(Path::new(&project.path), "origin")
            .ok_or_else(|| Error::new(ErrorKind::InvalidParams, "project has no origin remote"))?;
        let checkout = trimmed(&params.checkout);
        let base = trimmed(&params.base);
        let explicit_branch = trimmed(&params.branch);
        if checkout.is_some() && (explicit_branch.is_some() || base.is_some()) {
            return Err(Error::new(
                ErrorKind::InvalidParams,
                "checkout cannot be combined with base or branch",
            ));
        }
        let issue_name = params.issue.as_ref().map(|i| issue_name(&i.key, &i.title));
        let title = match &issue_name {
            Some(name) if params.title.trim().is_empty() => name.clone(),
            _ => params.title.clone(),
        };
        let id = {
            let store = self.store();
            let id = store.insert_task(
                project.id,
                &title,
                params.prompt.as_deref(),
                base.as_deref().unwrap_or_default(),
            )?;
            // Linked to the machine from the start so `machine.remove` sees it; routing waits for the remote id.
            store.set_task_machine(id, machine_id)?;
            id
        };
        let branch = checkout.clone().or(explicit_branch).unwrap_or_else(|| {
            match (&params.issue, issue_name) {
                (Some(issue), Some(name)) => {
                    let name = if name.is_empty() {
                        id.to_string()
                    } else {
                        name
                    };
                    trimmed(&issue.branch).unwrap_or_else(|| format!("asterism/{name}"))
                }
                _ => format!("asterism/{}", slugify(id, &params.title)),
            }
        });

        let activity = self.start_activity(
            ActivityKind::CreateTask,
            title.clone(),
            params.activity_token.clone(),
            if params.agent.is_some() {
                &["Create worktree", "Push branch", "Start agent"]
            } else {
                &["Create worktree", "Push branch"]
            },
        );
        let _guard = activity::ActivityGuard {
            daemon: self,
            id: activity,
        };
        self.set_activity_refs(activity, Some(project.id), Some(id));
        let clone_timeout = self.options.machine_clone_timeout;
        let remote = async {
            let remote_project: Project = self
                .machines()
                .call_with_timeout(
                    machine_id,
                    clone_timeout,
                    method::PROJECT_ENSURE,
                    ProjectEnsureParams {
                        url: strip_credentials(&origin),
                    },
                )
                .await?;
            let (branch, checkout) = match checkout {
                Some(c) => (None, Some(c)),
                None => (Some(branch), None),
            };
            self.machines()
                .call_with_timeout::<_, TaskCreateResult>(
                    machine_id,
                    clone_timeout,
                    method::TASK_CREATE,
                    TaskCreateParams {
                        project_id: remote_project.id,
                        title: title.clone(),
                        base,
                        branch,
                        checkout,
                        push: true,
                        on_conflict: params.on_conflict,
                        issue: params.issue.clone(),
                        ..Default::default()
                    },
                )
                .await
        }
        .await;
        let created = match remote {
            Ok(created) => created,
            Err(e) => {
                // The remote error is what the dialog needs; a failing local cleanup must not hide it.
                if let Err(cleanup) = self.store().delete_task(id) {
                    eprintln!("asterismd: deleting task {id} after a failed remote create failed: {cleanup}");
                }
                self.fail_activity_step(activity, e.message.clone(), true);
                return Err(e);
            }
        };
        self.advance_activity(activity, None);
        match &created.warning {
            Some(warning) => self.fail_activity_step(activity, warning.clone(), false),
            None => self.advance_activity(activity, None),
        }

        let remote_task = &created.task;
        if let Err(e) = self.link_remote_task(id, machine_id, remote_task, &params) {
            let _ = self
                .machines()
                .call::<_, serde_json::Value>(
                    machine_id,
                    method::TASK_DELETE,
                    TaskDeleteParams {
                        task_id: remote_task.id,
                        delete_branch: trimmed(&params.checkout).is_none(),
                    },
                )
                .await;
            let _ = self.store().delete_task(id);
            self.fail_activity_step(activity, e.message.clone(), true);
            return Err(e);
        }
        let task = self.task(id)?;
        self.emit(Event::TaskChanged(task.clone()));
        let session = match params.agent {
            Some(name) => {
                let started = self
                    .start_remote_session(
                        machine_id,
                        remote_task.id,
                        id,
                        SessionStartParams {
                            task_id: id,
                            kind: SessionKind::Agent { name },
                            prompt: params.prompt,
                        },
                    )
                    .await;
                match started {
                    Ok(session) => {
                        self.advance_activity(activity, None);
                        Some(session)
                    }
                    Err(e) => {
                        self.fail_activity_step(activity, e.message.clone(), true);
                        return Err(e);
                    }
                }
            }
            None => None,
        };
        Ok(TaskCreateResult {
            task,
            session,
            warning: created.warning,
        })
    }

    fn link_remote_task(
        &self,
        id: i64,
        machine_id: i64,
        remote_task: &Task,
        params: &TaskCreateParams,
    ) -> Result<()> {
        let store = self.store();
        store.set_task_remote(id, machine_id, remote_task.id)?;
        store.set_task_base_branch(id, &remote_task.base_branch)?;
        store.set_task_location(
            id,
            &remote_task.slug,
            &remote_task.branch,
            &remote_task.worktree_path,
        )?;
        if let Some(issue) = &params.issue {
            store.set_task_issue(
                id,
                &IssueRef {
                    source: issue.source.clone(),
                    key: issue.key.clone(),
                    url: issue.url.clone(),
                },
            )?;
        }
        Ok(())
    }
}

/// The machine uses its own git credentials, so userinfo in an http(s) origin is never sent.
fn strip_credentials(url: &str) -> String {
    let Some(rest) = url
        .strip_prefix("https://")
        .or_else(|| url.strip_prefix("http://"))
    else {
        return url.to_string();
    };
    let scheme = &url[..url.len() - rest.len()];
    let authority_end = rest.find('/').unwrap_or(rest.len());
    match rest[..authority_end].rfind('@') {
        Some(at) => format!("{scheme}{}", &rest[at + 1..]),
        None => url.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::strip_credentials;

    #[test]
    fn credentials_are_stripped_from_http_origins() {
        for (url, want) in [
            (
                "https://user:token@github.com/acme/demo.git",
                "https://github.com/acme/demo.git",
            ),
            ("http://token@host:8080/a/b", "http://host:8080/a/b"),
            (
                "https://github.com/acme/demo.git",
                "https://github.com/acme/demo.git",
            ),
            ("https://host/a/b@c", "https://host/a/b@c"),
            (
                "git@github.com:acme/demo.git",
                "git@github.com:acme/demo.git",
            ),
            (
                "ssh://git@host/acme/demo.git",
                "ssh://git@host/acme/demo.git",
            ),
        ] {
            assert_eq!(strip_credentials(url), want, "{url}");
        }
    }
}
