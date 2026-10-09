use std::path::Path;
use std::time::{Duration, Instant};

use asterism_plugin::protocol::{self, PrRef, ReviewGetForgeParams};
use asterism_proto::types::*;

use super::Daemon;
use crate::error::Result;
use crate::{git, lock, pr_status, review};

const CACHE_TTL: Duration = Duration::from_secs(30);

impl Daemon {
    /// The task's pull request and its forge, when the forge is installed.
    pub(super) fn review_pr(&self, task: &Task) -> Result<Option<(PrRef, bool)>> {
        let Some(number) = lock(&self.pr_status)
            .entries
            .get(&task.id)
            .map(|e| e.pr.number)
        else {
            return Ok(None);
        };
        let repo = self.project_path(task.project_id)?;
        let Some(forge) = self.pr_forge(&repo) else {
            return Ok(None);
        };
        let reviews = self
            .plugin_set()
            .registry
            .forge(&forge)
            .is_some_and(|(_, decl)| decl.reviews);
        let pr = PrRef {
            forge,
            project_path: repo.display().to_string(),
            number,
        };
        Ok(Some((pr, reviews)))
    }

    pub(super) async fn forge_review(&self, task_id: i64, pr: &PrRef) -> Result<ForgeReview> {
        let cached = lock(&self.review_cache).get(&task_id).cloned();
        if let Some((at, review)) = &cached {
            if at.elapsed() < CACHE_TTL {
                return Ok(review.clone());
            }
        }
        let fetched: Result<ForgeReview> = self
            .forge_call(
                &pr.forge,
                protocol::method::FORGE_REVIEW_GET,
                ReviewGetForgeParams { pr: pr.clone() },
                Some(self.call_timeout()),
            )
            .await;
        match fetched {
            Ok(review) => {
                lock(&self.review_cache).insert(task_id, (Instant::now(), review.clone()));
                Ok(review)
            }
            // Rate limited: keep showing the last answer rather than an error.
            Err(e) if pr_status::is_rate_limit(&e.message) => match cached {
                Some((_, review)) => Ok(review),
                None => Err(e),
            },
            Err(e) => Err(e),
        }
    }

    pub async fn review_get(&self, p: &ReviewGetParams) -> Result<ReviewResult> {
        let task = self.task(p.task_id)?;
        let repo = self.project_path(task.project_id)?;
        let pr = self.review_pr(&task)?;
        let reviews_supported = pr.as_ref().is_some_and(|(_, r)| *r);
        let local_comments = self.store().review_comments(task.id)?;
        if let (ReviewSource::Pr, Some((pr, true))) = (p.source, pr.clone()) {
            let forge = self.forge_review(task.id, &pr).await?;
            if !git::resolves(&repo, &forge.head_sha) {
                git::fetch_branch(&repo, "origin", &forge.head_ref, &self.options.git_env)?;
            }
            let base = self.effective_base(&repo, &task);
            let patch = git::diff_range(&repo, &base, &forge.head_sha)?;
            let wt = Path::new(&task.worktree_path);
            let local_ahead = git::is_dirty(wt)? || !git::resolves_to(wt, "HEAD", &forge.head_sha);
            let mut threads = forge.threads;
            threads.extend(review::local_threads(&local_comments, &patch));
            return Ok(ReviewResult {
                source: ReviewSource::Pr,
                pr: Some(pr.number),
                reviews_supported,
                patch,
                threads,
                conversation: forge.conversation,
                viewed_files: forge.viewed_files,
                pending_review: forge.pending_review,
                local_ahead,
            });
        }
        let patch = self.diff(task.id)?.patch;
        let viewed = self.store().review_viewed(task.id)?;
        let files = review::split_patch(&patch);
        let viewed_files = viewed
            .into_iter()
            .filter(|(path, hash)| {
                files
                    .iter()
                    .any(|f| &f.path == path && review::hash(f.text) == *hash)
            })
            .map(|(path, _)| path)
            .collect();
        Ok(ReviewResult {
            source: ReviewSource::Local,
            pr: pr.as_ref().map(|(pr, _)| pr.number),
            reviews_supported,
            threads: review::local_threads(&local_comments, &patch),
            patch,
            conversation: Vec::new(),
            viewed_files,
            pending_review: None,
            local_ahead: false,
        })
    }

    pub async fn review_set_viewed(&self, p: &ReviewViewedParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        let patch = self.diff(task.id)?.patch;
        let hash = review::split_patch(&patch)
            .into_iter()
            .find(|f| f.path == p.path)
            .map(|f| review::hash(f.text));
        self.store()
            .set_review_viewed(task.id, &p.path, hash.as_deref().filter(|_| p.viewed))?;
        self.emit(Event::ReviewChanged { task_id: task.id });
        Ok(())
    }
}
