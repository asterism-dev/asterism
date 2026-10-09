use std::collections::{BTreeMap, HashMap};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::{Duration, Instant};

use asterism_plugin::protocol::{
    self, CheckForgeParams, ForgeCommentMode, PrRef, ReviewAddCommentForgeParams,
    ReviewCommentForgeParams, ReviewGetForgeParams, ReviewReplyForgeParams,
    ReviewResolveForgeParams, ReviewSubmitForgeParams, ReviewViewedForgeParams,
};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::*;
use serde::Serialize;

use super::Daemon;
use crate::error::{Error, Result};
use crate::store::StoredReviewComment;
use crate::{git, lock, pr_status, review};

const CACHE_TTL: Duration = Duration::from_secs(30);

/// Forge reviews per task; every write bumps the task's generation so a fetch started before it never caches stale data.
#[derive(Default)]
pub(super) struct ReviewCache(HashMap<i64, (u64, Option<(Instant, ForgeReview)>)>);

impl ReviewCache {
    fn get(&self, task_id: i64) -> (u64, Option<(Instant, ForgeReview)>) {
        self.0.get(&task_id).cloned().unwrap_or_default()
    }

    fn put(&mut self, task_id: i64, generation: u64, review: ForgeReview) {
        let entry = self.0.entry(task_id).or_default();
        if entry.0 == generation {
            entry.1 = Some((Instant::now(), review));
        }
    }

    fn invalidate(&mut self, task_id: i64) {
        let entry = self.0.entry(task_id).or_default();
        entry.0 += 1;
        entry.1 = None;
    }
}

impl Daemon {
    /// Runs blocking work (git, mostly) off the async runtime.
    async fn off_runtime<T: Send + 'static>(
        self: &Arc<Self>,
        f: impl FnOnce(&Daemon) -> Result<T> + Send + 'static,
    ) -> Result<T> {
        let daemon = self.clone();
        tokio::task::spawn_blocking(move || f(&daemon))
            .await
            .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?
    }

    /// The task's pull request and its forge, when the forge is installed.
    pub(super) async fn review_pr(self: &Arc<Self>, task: &Task) -> Result<Option<(PrRef, bool)>> {
        let Some(number) = lock(&self.pr_status)
            .entries
            .get(&task.id)
            .map(|e| e.pr.number)
        else {
            return Ok(None);
        };
        let repo = self.project_path(task.project_id)?;
        let lookup = repo.clone();
        let Some(forge) = self.off_runtime(move |d| Ok(d.pr_forge(&lookup))).await? else {
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
        let (generation, cached) = lock(&self.review_cache).get(task_id);
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
                lock(&self.review_cache).put(task_id, generation, review.clone());
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

    pub async fn review_get(self: &Arc<Self>, p: &ReviewGetParams) -> Result<ReviewResult> {
        let task = self.task(p.task_id)?;
        let pr = self.review_pr(&task).await?;
        let reviews_supported = pr.as_ref().is_some_and(|(_, r)| *r);
        let local_comments = self.store().review_comments(task.id)?;
        if let (ReviewSource::Pr, Some((pr, true))) = (p.source, pr.clone()) {
            let forge = self.forge_review(task.id, &pr).await?;
            let (head_sha, head_ref, base_sha) = (
                forge.head_sha.clone(),
                forge.head_ref.clone(),
                forge.base_sha.clone(),
            );
            let task = task.clone();
            let (patch, local_ahead) = self
                .off_runtime(move |d| {
                    let (repo, base) = pr_range(d, &task, &head_sha, &head_ref, &base_sha)?;
                    let patch = git::diff_range(&repo, &base, &head_sha)?;
                    let wt = PathBuf::from(&task.worktree_path);
                    let local_ahead =
                        git::is_dirty(&wt)? || !git::resolves_to(&wt, "HEAD", &head_sha);
                    Ok((patch, local_ahead))
                })
                .await?;
            let (title, body, author, url) = (forge.title, forge.body, forge.author, forge.url);
            let (checks, commit_checks) = (forge.checks, forge.commit_checks);
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
                title,
                body,
                author,
                url,
                checks,
                commit_checks,
            });
        }
        let task_id = task.id;
        let patch = self
            .off_runtime(move |d| Ok(d.diff(task_id)?.patch))
            .await?;
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
            title: String::new(),
            body: String::new(),
            author: String::new(),
            url: String::new(),
            checks: Vec::new(),
            commit_checks: BTreeMap::new(),
        })
    }

    pub async fn review_commits(
        self: &Arc<Self>,
        p: &ReviewCommitsParams,
    ) -> Result<ReviewCommitsResult> {
        let task = self.task(p.task_id)?;
        if let (ReviewSource::Pr, Some((pr, true))) = (p.source, self.review_pr(&task).await?) {
            let forge = self.forge_review(task.id, &pr).await?;
            return self
                .off_runtime(move |d| {
                    let (repo, base) =
                        pr_range(d, &task, &forge.head_sha, &forge.head_ref, &forge.base_sha)?;
                    Ok(ReviewCommitsResult {
                        commits: git::log_range(&repo, &base, &forge.head_sha)?,
                        uncommitted: false,
                    })
                })
                .await;
        }
        self.off_runtime(move |d| {
            let wt = PathBuf::from(&task.worktree_path);
            let base = d.effective_base(&d.project_path(task.project_id)?, &task);
            Ok(ReviewCommitsResult {
                commits: git::log_range(&wt, &base, "HEAD")?,
                uncommitted: git::is_dirty(&wt)?,
            })
        })
        .await
    }

    pub async fn review_commit_diff(
        self: &Arc<Self>,
        p: &ReviewCommitDiffParams,
    ) -> Result<TaskDiffResult> {
        let task = self.task(p.task_id)?;
        let wt = PathBuf::from(&task.worktree_path);
        let Some(sha) = p.sha.as_deref().map(str::to_ascii_lowercase) else {
            return self
                .off_runtime(move |_| {
                    Ok(TaskDiffResult {
                        patch: git::diff(&wt, "HEAD")?,
                    })
                })
                .await;
        };
        let invalid = || {
            Error::new(
                ErrorKind::InvalidParams,
                format!("{sha} is not a commit of this task's changes"),
            )
        };
        if !(7..=40).contains(&sha.len()) || !sha.bytes().all(|b| b.is_ascii_hexdigit()) {
            return Err(invalid());
        }
        let commits = self
            .review_commits(&ReviewCommitsParams {
                task_id: task.id,
                source: p.source,
            })
            .await?
            .commits;
        let full = commits
            .into_iter()
            .find(|c| c.sha.starts_with(&sha))
            .ok_or_else(invalid)?
            .sha;
        self.off_runtime(move |_| {
            Ok(TaskDiffResult {
                patch: git::show_patch(&wt, &full)?,
            })
        })
        .await
    }

    pub async fn review_set_viewed(self: &Arc<Self>, p: &ReviewViewedParams) -> Result<()> {
        if p.source == ReviewSource::Pr {
            let task = self.task(p.task_id)?;
            let pr = self.reviewable_pr(&task).await?;
            let params = ReviewViewedForgeParams {
                pr: pr.clone(),
                path: p.path.clone(),
                viewed: p.viewed,
            };
            return self
                .forge_mutate(
                    task.id,
                    &pr,
                    protocol::method::FORGE_REVIEW_SET_VIEWED,
                    params,
                )
                .await;
        }
        let task = self.task(p.task_id)?;
        let task_id = task.id;
        let patch = self
            .off_runtime(move |d| Ok(d.diff(task_id)?.patch))
            .await?;
        let hash = review::split_patch(&patch)
            .into_iter()
            .find(|f| f.path == p.path)
            .map(|f| review::hash(f.text));
        self.store()
            .set_review_viewed(task.id, &p.path, hash.as_deref().filter(|_| p.viewed))?;
        self.emit(Event::ReviewChanged { task_id: task.id });
        Ok(())
    }

    async fn reviewable_pr(self: &Arc<Self>, task: &Task) -> Result<PrRef> {
        match self.review_pr(task).await? {
            Some((pr, true)) => Ok(pr),
            Some((_, false)) => Err(Error::new(
                ErrorKind::InvalidParams,
                "this forge does not support reviews",
            )),
            None => Err(Error::new(
                ErrorKind::InvalidParams,
                "the task has no pull request",
            )),
        }
    }

    /// Calls the forge and drops the cached review whether or not the call succeeded.
    async fn forge_write<P: Serialize>(
        &self,
        task_id: i64,
        pr: &PrRef,
        method: &str,
        params: P,
    ) -> Result<()> {
        let result: Result<serde_json::Value> = self
            .forge_call(&pr.forge, method, params, Some(self.call_timeout()))
            .await;
        lock(&self.review_cache).invalidate(task_id);
        result.map(|_| ())
    }

    async fn forge_mutate<P: Serialize>(
        &self,
        task_id: i64,
        pr: &PrRef,
        method: &str,
        params: P,
    ) -> Result<()> {
        let result = self.forge_write(task_id, pr, method, params).await;
        self.changed(task_id);
        result
    }

    /// The unpublished comments of a local thread; published threads live on the forge now.
    fn local_thread(&self, task_id: i64, thread_id: &str) -> Result<Vec<StoredReviewComment>> {
        let rows: Vec<_> = self
            .store()
            .review_comments(task_id)?
            .into_iter()
            .filter(|c| c.thread_id == thread_id)
            .collect();
        if rows.is_empty() {
            return Err(Error::new(
                ErrorKind::NotFound,
                format!("thread {thread_id} not found"),
            ));
        }
        let open: Vec<_> = rows.into_iter().filter(|c| !c.published).collect();
        if open.is_empty() {
            return Err(Error::new(
                ErrorKind::InvalidParams,
                format!("thread {thread_id} was already published"),
            ));
        }
        Ok(open)
    }

    fn changed(&self, task_id: i64) {
        self.emit(Event::ReviewChanged { task_id });
    }

    pub async fn review_comment(self: &Arc<Self>, p: &ReviewCommentParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        let mode = match p.target {
            CommentTarget::Local => {
                let patch = self
                    .review_get(&ReviewGetParams {
                        task_id: task.id,
                        source: p.source,
                    })
                    .await?
                    .patch;
                let text = review::line_text(&patch, &p.path, p.side, p.line).unwrap_or_default();
                self.store()
                    .add_review_comment(task.id, None, &p.path, p.line, p.side, &text, &p.body)?;
                self.changed(task.id);
                return Ok(());
            }
            CommentTarget::Single => ForgeCommentMode::Single,
            CommentTarget::Review => ForgeCommentMode::Review,
        };
        let pr = self.reviewable_pr(&task).await?;
        let params = ReviewCommentForgeParams {
            pr: pr.clone(),
            path: p.path.clone(),
            line: p.line,
            side: p.side,
            body: p.body.clone(),
            mode,
        };
        self.forge_mutate(task.id, &pr, protocol::method::FORGE_REVIEW_COMMENT, params)
            .await
    }

    pub async fn review_reply(self: &Arc<Self>, p: &ReviewReplyParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        if p.thread_id.starts_with("local-") {
            let first = self.local_thread(task.id, &p.thread_id)?.remove(0);
            self.store().add_review_comment(
                task.id,
                Some(&p.thread_id),
                &first.path,
                first.line,
                first.side,
                &first.line_text,
                &p.body,
            )?;
            self.changed(task.id);
            return Ok(());
        }
        let pr = self.reviewable_pr(&task).await?;
        let params = ReviewReplyForgeParams {
            pr: pr.clone(),
            thread_id: p.thread_id.clone(),
            body: p.body.clone(),
        };
        self.forge_mutate(task.id, &pr, protocol::method::FORGE_REVIEW_REPLY, params)
            .await
    }

    pub async fn review_resolve(self: &Arc<Self>, p: &ReviewResolveParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        if p.thread_id.starts_with("local-") {
            self.local_thread(task.id, &p.thread_id)?;
            self.store()
                .set_review_thread_resolved(task.id, &p.thread_id, p.resolved)?;
            self.changed(task.id);
            return Ok(());
        }
        let pr = self.reviewable_pr(&task).await?;
        let params = ReviewResolveForgeParams {
            pr: pr.clone(),
            thread_id: p.thread_id.clone(),
            resolved: p.resolved,
        };
        self.forge_mutate(task.id, &pr, protocol::method::FORGE_REVIEW_RESOLVE, params)
            .await
    }

    pub async fn review_submit(self: &Arc<Self>, p: &ReviewSubmitParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        let pr = self.reviewable_pr(&task).await?;
        let params = ReviewSubmitForgeParams {
            pr: pr.clone(),
            event: p.event,
            body: p.body.clone(),
        };
        self.forge_mutate(task.id, &pr, protocol::method::FORGE_REVIEW_SUBMIT, params)
            .await
    }

    pub async fn review_add_comment(self: &Arc<Self>, p: &ReviewAddCommentParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        let pr = self.reviewable_pr(&task).await?;
        if p.body.trim().is_empty() {
            return Err(Error::new(ErrorKind::InvalidParams, "the comment is empty"));
        }
        let params = ReviewAddCommentForgeParams {
            pr: pr.clone(),
            body: p.body.clone(),
        };
        self.forge_mutate(
            task.id,
            &pr,
            protocol::method::FORGE_REVIEW_ADD_COMMENT,
            params,
        )
        .await
    }

    pub async fn review_check_log(self: &Arc<Self>, p: &ReviewCheckParams) -> Result<CheckLog> {
        let task = self.task(p.task_id)?;
        let pr = self.reviewable_pr(&task).await?;
        let params = CheckForgeParams {
            pr: pr.clone(),
            check_id: p.check_id.clone(),
        };
        self.forge_call(
            &pr.forge,
            protocol::method::FORGE_CHECKS_LOG,
            params,
            Some(self.call_timeout()),
        )
        .await
    }

    pub async fn review_check_rerun(self: &Arc<Self>, p: &ReviewCheckParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        let pr = self.reviewable_pr(&task).await?;
        let params = CheckForgeParams {
            pr: pr.clone(),
            check_id: p.check_id.clone(),
        };
        self.forge_mutate(task.id, &pr, protocol::method::FORGE_CHECKS_RERUN, params)
            .await
    }

    /// Posts a local thread to the forge as one comment: its replies are joined into the body.
    pub async fn review_publish(self: &Arc<Self>, p: &ReviewPublishParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        let comments = self.local_thread(task.id, &p.thread_id)?;
        let pr = self.reviewable_pr(&task).await?;
        let first = &comments[0];
        let params = ReviewCommentForgeParams {
            pr: pr.clone(),
            path: first.path.clone(),
            line: first.line,
            side: first.side,
            body: comments
                .iter()
                .map(|c| c.body.as_str())
                .collect::<Vec<_>>()
                .join("\n\n"),
            mode: if p.target == CommentTarget::Review {
                ForgeCommentMode::Review
            } else {
                ForgeCommentMode::Single
            },
        };
        let result = self
            .forge_write(task.id, &pr, protocol::method::FORGE_REVIEW_COMMENT, params)
            .await
            .and_then(|_| {
                self.store()
                    .set_review_thread_published(task.id, &p.thread_id)?;
                Ok(())
            });
        // Emitted once, after the thread is marked published, so clients never see it twice.
        self.changed(task.id);
        result
    }

    pub async fn review_prompt(
        self: &Arc<Self>,
        p: &ReviewPromptParams,
    ) -> Result<ReviewPromptResult> {
        let r = self
            .review_get(&ReviewGetParams {
                task_id: p.task_id,
                source: p.source,
            })
            .await?;
        let threads: Vec<&ReviewThread> = r
            .threads
            .iter()
            .filter(|t| match &p.thread_ids {
                Some(ids) => ids.contains(&t.id),
                None => p.include_resolved || !t.resolved,
            })
            .collect();
        Ok(ReviewPromptResult {
            prompt: review::prompt(
                &threads,
                &r.patch,
                &self.store().review_comments(p.task_id)?,
            ),
        })
    }
}

/// Fetches the PR head when it is missing and picks the diff base: the forge's base when it resolves, else the task's.
fn pr_range(
    d: &Daemon,
    task: &Task,
    head_sha: &str,
    head_ref: &str,
    base_sha: &str,
) -> Result<(PathBuf, String)> {
    let repo = d.project_path(task.project_id)?;
    if !git::resolves(&repo, head_sha) {
        git::fetch_branch(&repo, "origin", head_ref, &d.options.git_env)?;
    }
    let base = if !base_sha.is_empty() && git::resolves(&repo, base_sha) {
        base_sha.to_string()
    } else {
        d.effective_base(&repo, task)
    };
    Ok((repo, base))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn review(head: &str) -> ForgeReview {
        ForgeReview {
            head_sha: head.into(),
            base_sha: String::new(),
            head_ref: String::new(),
            threads: Vec::new(),
            conversation: Vec::new(),
            viewed_files: Vec::new(),
            pending_review: None,
            title: String::new(),
            body: String::new(),
            author: String::new(),
            url: String::new(),
            checks: Vec::new(),
            commit_checks: BTreeMap::new(),
        }
    }

    #[test]
    fn a_fetch_started_before_a_write_is_not_cached() {
        let mut cache = ReviewCache::default();
        let (generation, _) = cache.get(1);
        cache.invalidate(1);
        cache.put(1, generation, review("stale"));
        assert!(cache.get(1).1.is_none());
        let (generation, _) = cache.get(1);
        cache.put(1, generation, review("fresh"));
        assert_eq!(cache.get(1).1.unwrap().1.head_sha, "fresh");
    }
}
