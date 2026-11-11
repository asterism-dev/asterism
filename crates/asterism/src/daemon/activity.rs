use std::sync::atomic::Ordering;

use asterism_proto::types::*;

use super::{unix_now, Daemon};
use crate::lock;

const MAX_ACTIVITIES: usize = 50;

impl Daemon {
    #[doc(hidden)]
    pub fn start_activity(
        &self,
        kind: ActivityKind,
        title: String,
        token: Option<String>,
        steps: &[&str],
    ) -> i64 {
        let id = self.next_activity_id.fetch_add(1, Ordering::Relaxed);
        let steps = steps
            .iter()
            .enumerate()
            .map(|(i, label)| ActivityStep {
                label: (*label).into(),
                status: if i == 0 {
                    ActivityStatus::Running
                } else {
                    ActivityStatus::Pending
                },
                detail: None,
            })
            .collect();
        let activity = Activity {
            id,
            kind,
            title,
            token,
            project_id: None,
            task_id: None,
            started_at: unix_now(),
            finished_at: None,
            status: ActivityStatus::Running,
            steps,
        };
        {
            let mut log = lock(&self.activities);
            log.push_front(activity.clone());
            log.truncate(MAX_ACTIVITIES);
        }
        self.emit(Event::ActivityChanged(activity));
        id
    }

    #[doc(hidden)]
    pub fn advance_activity(&self, id: i64, detail: Option<String>) {
        self.update_activity(id, |a| {
            if a.status != ActivityStatus::Running {
                return;
            }
            if let Some(step) = current_step(a) {
                step.status = ActivityStatus::Done;
                step.detail = detail;
            }
            move_to_next_step(a);
        });
    }

    #[doc(hidden)]
    pub fn fail_activity_step(&self, id: i64, error: String, fatal: bool) {
        self.update_activity(id, |a| {
            if a.status != ActivityStatus::Running {
                return;
            }
            if let Some(step) = current_step(a) {
                step.status = ActivityStatus::Failed;
                step.detail = Some(error);
            }
            if fatal {
                a.status = ActivityStatus::Failed;
                a.finished_at = Some(unix_now());
            } else {
                move_to_next_step(a);
            }
        });
    }

    #[doc(hidden)]
    pub fn set_activity_refs(&self, id: i64, project_id: Option<i64>, task_id: Option<i64>) {
        self.update_activity(id, |a| {
            a.project_id = project_id;
            a.task_id = task_id;
        });
    }

    pub fn activities(&self) -> Vec<Activity> {
        lock(&self.activities).iter().cloned().collect()
    }

    fn update_activity(&self, id: i64, change: impl FnOnce(&mut Activity)) {
        let updated = {
            let mut log = lock(&self.activities);
            log.iter_mut().find(|a| a.id == id).map(|a| {
                change(a);
                a.clone()
            })
        };
        if let Some(activity) = updated {
            self.emit(Event::ActivityChanged(activity));
        }
    }
}

/// Fails a still-running activity when its future is dropped mid-flight; a no-op once finished.
#[doc(hidden)]
pub struct ActivityGuard<'a> {
    pub daemon: &'a Daemon,
    pub id: i64,
}

impl Drop for ActivityGuard<'_> {
    fn drop(&mut self) {
        self.daemon
            .fail_activity_step(self.id, "Interrupted".into(), true);
    }
}

fn current_step(a: &mut Activity) -> Option<&mut ActivityStep> {
    a.steps
        .iter_mut()
        .find(|s| s.status == ActivityStatus::Running)
}

fn move_to_next_step(a: &mut Activity) {
    match a
        .steps
        .iter_mut()
        .find(|s| s.status == ActivityStatus::Pending)
    {
        Some(next) => next.status = ActivityStatus::Running,
        None => {
            a.status = ActivityStatus::Done;
            a.finished_at = Some(unix_now());
        }
    }
}
