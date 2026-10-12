use std::path::{Path, PathBuf};

use asterism_proto::rpc::ErrorKind;

use super::same_path;
use crate::error::{Error, Result};
use crate::git::{self, WorktreeEntry};

const MAX_SUFFIX: u32 = 100;

pub(super) struct Conflict {
    pub target: PathBuf,
    pub branch_taken: bool,
}

/// `creating` is false for "checkout branch", where an existing branch is the point.
pub(super) fn detect(repo: &Path, root: &Path, branch: &str, creating: bool) -> Option<Conflict> {
    let target = root.join(branch);
    let branch_taken = creating && git::branch_exists(repo, branch);
    (target.exists() || branch_taken).then_some(Conflict {
        target,
        branch_taken,
    })
}

impl Conflict {
    pub fn error(&self, branch: &str) -> Error {
        let message = if self.branch_taken {
            format!("branch `{branch}` already exists")
        } else {
            format!(
                "worktree directory `{}` already exists",
                self.target.display()
            )
        };
        Error::new(ErrorKind::BranchExists, message)
    }
}

pub(super) fn suffixed(repo: &Path, root: &Path, branch: &str) -> Result<String> {
    (2..=MAX_SUFFIX)
        .map(|n| format!("{branch}-{n}"))
        .find(|b| !git::branch_exists(repo, b) && !root.join(b).exists())
        .ok_or_else(|| {
            Error::new(
                ErrorKind::BranchExists,
                format!("no free name for `{branch}` up to `{branch}-{MAX_SUFFIX}`"),
            )
        })
}

pub(super) fn registered(repo: &Path, path: &Path) -> Result<Option<WorktreeEntry>> {
    let path = path.to_string_lossy();
    Ok(git::worktrees(repo)?
        .into_iter()
        .find(|w| !w.is_main && same_path(&w.path, &path)))
}

pub(super) fn foreign(path: &Path) -> Error {
    Error::new(
        ErrorKind::InvalidParams,
        format!(
            "{} exists but is not a worktree of this project; move it away first",
            path.display()
        ),
    )
}
