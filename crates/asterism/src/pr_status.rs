use std::collections::{BTreeMap, HashMap, HashSet};
use std::sync::Mutex;

use asterism_plugin::protocol::BranchPr;
use asterism_proto::types::{PrState, PullRequest};

use crate::lock;

pub const ACTIVE_INTERVAL: i64 = 60;
pub const IDLE_INTERVAL: i64 = 600;
pub const ACTIVE_WINDOW: i64 = 86_400;

pub fn is_due(now: i64, last_polled: Option<i64>, last_activity: i64, rate_limited: bool) -> bool {
    let Some(last) = last_polled else { return true };
    let interval = if rate_limited || now - last_activity > ACTIVE_WINDOW { IDLE_INTERVAL } else { ACTIVE_INTERVAL };
    now - last >= interval
}

// ponytail: forges report rate limits only in prose; add an error kind if a forge offers one.
pub fn is_rate_limit(message: &str) -> bool {
    message.to_ascii_lowercase().contains("rate limit")
}

pub struct ForgeCandidate<'a> {
    pub id: &'a str,
    pub hosts: &'a [String],
    pub pull_requests: bool,
}

pub fn forge_for_host(forges: &[ForgeCandidate], host: &str) -> Option<String> {
    forges
        .iter()
        .find(|f| f.pull_requests && f.hosts.iter().any(|h| h.eq_ignore_ascii_case(host)))
        .map(|f| f.id.to_string())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    pub project_id: i64,
    pub branch: String,
    pub pr: PullRequest,
}

fn finished(pr: &PullRequest) -> bool {
    matches!(pr.state, PrState::Merged | PrState::Closed)
}

/// Branches of `tasks` (task id, branch) whose PR may still change.
pub fn branches_to_ask(tasks: &[(i64, String)], known: &HashMap<i64, Entry>) -> Vec<String> {
    tasks
        .iter()
        .filter(|(id, branch)| !known.get(id).is_some_and(|e| e.branch == *branch && finished(&e.pr)))
        .map(|(_, branch)| branch.clone())
        .collect()
}

/// Applies one project's poll to `known`; returns the task ids whose PR changed, in id order.
pub fn merge(project_id: i64, tasks: &[(i64, String)], asked: &[String], reply: Vec<BranchPr>, known: &mut HashMap<i64, Entry>) -> Vec<(i64, Option<PullRequest>)> {
    let mut reply: HashMap<String, PullRequest> = reply.into_iter().map(|b| (b.branch, b.pr)).collect();
    let mut changes = Vec::new();
    let current: HashSet<i64> = tasks.iter().map(|(id, _)| *id).collect();
    let mut gone: Vec<i64> = known.iter().filter(|(id, e)| e.project_id == project_id && !current.contains(id)).map(|(id, _)| *id).collect();
    gone.sort_unstable();
    for id in gone {
        known.remove(&id);
        changes.push((id, None));
    }
    for (id, branch) in tasks {
        if !asked.contains(branch) {
            continue;
        }
        let new = reply.remove(branch);
        let old = known.get(id).filter(|e| e.branch == *branch).map(|e| e.pr.clone());
        if new == old {
            continue;
        }
        match &new {
            Some(pr) => {
                known.insert(*id, Entry { project_id, branch: branch.clone(), pr: pr.clone() });
            }
            None => {
                known.remove(id);
            }
        }
        changes.push((*id, new));
    }
    changes.sort_by_key(|(id, _)| *id);
    changes
}

#[derive(Debug, Default)]
pub struct PrStatus {
    pub entries: HashMap<i64, Entry>,
    pub errors: BTreeMap<i64, String>,
    pub polled: HashMap<i64, i64>,
    pub rate_limited: HashSet<i64>,
    pub in_flight: HashSet<i64>,
}

/// Marks a project as being polled; dropping it (also when a request is cancelled) clears the mark.
pub struct InFlight<'a> {
    status: &'a Mutex<PrStatus>,
    project_id: i64,
}

impl<'a> InFlight<'a> {
    pub fn start(status: &'a Mutex<PrStatus>, project_id: i64) -> Option<Self> {
        lock(status).in_flight.insert(project_id).then(|| Self { status, project_id })
    }
}

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        lock(self.status).in_flight.remove(&self.project_id);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use asterism_proto::types::{ChecksState, PrChecks, PrState, ReviewState};

    fn pr(number: u64, state: PrState) -> PullRequest {
        PullRequest {
            number,
            url: format!("https://x/{number}"),
            title: "t".into(),
            state,
            review: ReviewState::None,
            checks: PrChecks { state: ChecksState::Success, failing: vec![] },
        }
    }

    #[test]
    fn cadence_depends_on_activity_and_rate_limits() {
        let now = 1_000_000;
        assert!(is_due(now, None, 0, false));
        assert!(!is_due(now, Some(now - 30), now, false));
        assert!(is_due(now, Some(now - 60), now, false));
        assert!(!is_due(now, Some(now - 120), now - ACTIVE_WINDOW - 1, false));
        assert!(is_due(now, Some(now - 600), now - ACTIVE_WINDOW - 1, false));
        assert!(!is_due(now, Some(now - 120), now, true));
        assert!(is_rate_limit("API rate limit exceeded for user") && is_rate_limit("Rate Limit") && !is_rate_limit("not logged in"));
    }

    #[test]
    fn only_flagged_forges_match_the_host() {
        let gh = vec!["github.com".to_string()];
        let forges = [
            ForgeCandidate { id: "plain", hosts: &gh, pull_requests: false },
            ForgeCandidate { id: "github", hosts: &gh, pull_requests: true },
        ];
        assert_eq!(forge_for_host(&forges, "GitHub.com").as_deref(), Some("github"));
        assert_eq!(forge_for_host(&forges[..1], "github.com"), None);
        assert_eq!(forge_for_host(&forges, "gitlab.com"), None);
    }

    #[test]
    fn finished_prs_are_kept_and_not_asked_again() {
        let mut known = HashMap::new();
        let tasks = vec![(1, "a".to_string()), (2, "b".to_string())];
        let changes = merge(9, &tasks, &["a".into(), "b".into()], vec![BranchPr { branch: "a".into(), pr: pr(5, PrState::Open) }], &mut known);
        assert_eq!(changes, vec![(1, Some(pr(5, PrState::Open)))]);
        assert!(merge(9, &tasks, &["a".into(), "b".into()], vec![BranchPr { branch: "a".into(), pr: pr(5, PrState::Open) }], &mut known).is_empty());

        merge(9, &tasks, &["a".into(), "b".into()], vec![BranchPr { branch: "a".into(), pr: pr(5, PrState::Merged) }], &mut known);
        assert_eq!(branches_to_ask(&tasks, &known), vec!["b".to_string()]);
        let changes = merge(9, &tasks, &["b".into()], vec![], &mut known);
        assert!(changes.is_empty(), "the merged PR of task 1 stays");
        assert_eq!(known[&1].pr.state, PrState::Merged);
    }

    #[test]
    fn prs_vanish_with_their_task_or_from_the_reply() {
        let mut known = HashMap::new();
        let tasks = vec![(1, "a".to_string()), (2, "b".to_string())];
        let both = vec![BranchPr { branch: "a".into(), pr: pr(5, PrState::Open) }, BranchPr { branch: "b".into(), pr: pr(6, PrState::Open) }];
        merge(9, &tasks, &["a".into(), "b".into()], both, &mut known);
        known.insert(3, Entry { project_id: 4, branch: "other".into(), pr: pr(1, PrState::Open) });
        let changes = merge(9, &tasks[..1], &["a".into()], vec![], &mut known);
        assert_eq!(changes, vec![(1, None), (2, None)]);
        assert!(known.contains_key(&3), "other projects are untouched");
    }

    #[test]
    fn in_flight_guards_release_on_drop() {
        let status = Mutex::new(PrStatus::default());
        let guard = InFlight::start(&status, 1).unwrap();
        assert!(InFlight::start(&status, 1).is_none());
        drop(guard);
        assert!(InFlight::start(&status, 1).is_some());
    }
}
