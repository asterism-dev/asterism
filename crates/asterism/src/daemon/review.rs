use std::path::PathBuf;
use std::time::{Duration, Instant};

use asterism_plugin::protocol::{
    self, ForgeCommentMode, PrRef, ReviewCommentForgeParams, ReviewGetForgeParams,
    ReviewReplyForgeParams, ReviewResolveForgeParams, ReviewSubmitForgeParams,
    ReviewViewedForgeParams,
};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::*;
use serde::Serialize;

use super::Daemon;
use crate::error::{Error, Result};
use crate::store::StoredReviewComment;
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
            let fallback_base = self.effective_base(&repo, &task);
            let env = self.options.git_env.clone();
            let (head_sha, head_ref, base_sha) = (
                forge.head_sha.clone(),
                forge.head_ref.clone(),
                forge.base_sha.clone(),
            );
            let wt = PathBuf::from(&task.worktree_path);
            let (patch, local_ahead) = tokio::task::spawn_blocking(move || -> Result<_> {
                if !git::resolves(&repo, &head_sha) {
                    git::fetch_branch(&repo, "origin", &head_ref, &env)?;
                }
                let base = if !base_sha.is_empty() && git::resolves(&repo, &base_sha) {
                    base_sha
                } else {
                    fallback_base
                };
                let patch = git::diff_range(&repo, &base, &head_sha)?;
                let local_ahead = git::is_dirty(&wt)? || !git::resolves_to(&wt, "HEAD", &head_sha);
                Ok((patch, local_ahead))
            })
            .await
            .map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))??;
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
        if p.source == ReviewSource::Pr {
            let task = self.task(p.task_id)?;
            let pr = self.reviewable_pr(&task)?;
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

    fn reviewable_pr(&self, task: &Task) -> Result<PrRef> {
        match self.review_pr(task)? {
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
        lock(&self.review_cache).remove(&task_id);
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

    pub async fn review_comment(&self, p: &ReviewCommentParams) -> Result<()> {
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
        let pr = self.reviewable_pr(&task)?;
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

    pub async fn review_reply(&self, p: &ReviewReplyParams) -> Result<()> {
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
        let pr = self.reviewable_pr(&task)?;
        let params = ReviewReplyForgeParams {
            pr: pr.clone(),
            thread_id: p.thread_id.clone(),
            body: p.body.clone(),
        };
        self.forge_mutate(task.id, &pr, protocol::method::FORGE_REVIEW_REPLY, params)
            .await
    }

    pub async fn review_resolve(&self, p: &ReviewResolveParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        if p.thread_id.starts_with("local-") {
            self.local_thread(task.id, &p.thread_id)?;
            self.store()
                .set_review_thread_resolved(task.id, &p.thread_id, p.resolved)?;
            self.changed(task.id);
            return Ok(());
        }
        let pr = self.reviewable_pr(&task)?;
        let params = ReviewResolveForgeParams {
            pr: pr.clone(),
            thread_id: p.thread_id.clone(),
            resolved: p.resolved,
        };
        self.forge_mutate(task.id, &pr, protocol::method::FORGE_REVIEW_RESOLVE, params)
            .await
    }

    pub async fn review_submit(&self, p: &ReviewSubmitParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        let pr = self.reviewable_pr(&task)?;
        let params = ReviewSubmitForgeParams {
            pr: pr.clone(),
            event: p.event,
            body: p.body.clone(),
        };
        self.forge_mutate(task.id, &pr, protocol::method::FORGE_REVIEW_SUBMIT, params)
            .await
    }

    /// Posts a local thread to the forge as one comment: its replies are joined into the body.
    pub async fn review_publish(&self, p: &ReviewPublishParams) -> Result<()> {
        let task = self.task(p.task_id)?;
        let comments = self.local_thread(task.id, &p.thread_id)?;
        let pr = self.reviewable_pr(&task)?;
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

    pub async fn review_prompt(&self, p: &ReviewPromptParams) -> Result<ReviewPromptResult> {
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
            prompt: review::prompt(&threads, &r.patch),
        })
    }
}
