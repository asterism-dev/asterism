use std::path::Path;

use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{
    method, ActivityKind, Event, IssueRef, Project, ProjectEnsureParams, TaskCreateParams,
    TaskCreateResult,
};

use super::{activity, issue_name, not_found, slugify, Daemon};
use crate::error::{Error, Result};
use crate::git;

fn trimmed(value: &Option<String>) -> Option<String> {
    value
        .as_deref()
        .map(str::trim)
        .filter(|v| !v.is_empty())
        .map(String::from)
}

impl Daemon {
    /// `task.create` for a machine: the machine clones origin, creates and pushes the branch; the hub keeps a linked row.
    pub(super) async fn create_remote_task(
        &self,
        machine_id: i64,
        params: TaskCreateParams,
    ) -> Result<TaskCreateResult> {
        if params.agent.is_some() {
            return Err(Error::new(
                ErrorKind::InvalidParams,
                "agents on machines are not supported yet",
            ));
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
        let id = self.store().insert_task(
            project.id,
            &title,
            params.prompt.as_deref(),
            base.as_deref().unwrap_or_default(),
        )?;
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
            &["Create worktree", "Push branch"],
        );
        let _guard = activity::ActivityGuard {
            daemon: self,
            id: activity,
        };
        self.set_activity_refs(activity, Some(project.id), Some(id));
        let remote = async {
            let remote_project: Project = self
                .machines()
                .call(
                    machine_id,
                    method::PROJECT_ENSURE,
                    ProjectEnsureParams { url: origin },
                )
                .await?;
            let (branch, checkout) = match checkout {
                Some(c) => (None, Some(c)),
                None => (Some(branch), None),
            };
            self.machines()
                .call::<_, TaskCreateResult>(
                    machine_id,
                    method::TASK_CREATE,
                    TaskCreateParams {
                        project_id: remote_project.id,
                        title: title.clone(),
                        base,
                        branch,
                        checkout,
                        push: true,
                        on_conflict: params.on_conflict,
                        ..Default::default()
                    },
                )
                .await
        }
        .await;
        let created = match remote {
            Ok(created) => created,
            Err(e) => {
                self.store().delete_task(id)?;
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
        drop(store);
        let task = self.task(id)?;
        self.emit(Event::TaskChanged(task.clone()));
        Ok(TaskCreateResult {
            task,
            session: None,
            warning: created.warning,
        })
    }
}
