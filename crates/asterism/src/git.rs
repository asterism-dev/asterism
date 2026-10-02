use std::path::{Path, PathBuf};
use std::process::Command;

use asterism_proto::rpc::ErrorKind;

use crate::error::{Error, Result};

// ponytail: git runs synchronously on the calling (async) worker; move to spawn_blocking if big repos stall other requests.
fn git(dir: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(dir).args(args).output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(Error::new(ErrorKind::Git, String::from_utf8_lossy(&out.stderr).trim()))
    }
}

pub fn toplevel(path: &Path) -> Result<PathBuf> {
    git(path, &["rev-parse", "--show-toplevel"])
        .map(|out| PathBuf::from(out.trim()))
        .map_err(|e| {
            Error::new(
                ErrorKind::NotARepo,
                format!("{} is not inside a git repository: {}", path.display(), e.message),
            )
        })
}

pub fn base_ref(repo: &Path) -> Result<String> {
    let branch = git(repo, &["rev-parse", "--abbrev-ref", "HEAD"])?;
    if branch.trim() == "HEAD" {
        Ok(git(repo, &["rev-parse", "HEAD"])?.trim().to_string())
    } else {
        Ok(branch.trim().to_string())
    }
}

pub fn add_worktree(repo: &Path, branch: &str, path: &Path, base: &str) -> Result<()> {
    let path = path.to_string_lossy();
    git(repo, &["worktree", "add", "-q", "-b", branch, &path, base]).map(|_| ()).map_err(|e| {
        if e.message.contains("already exists") {
            Error::new(ErrorKind::BranchExists, e.message)
        } else {
            e
        }
    })
}

pub fn is_dirty(worktree: &Path) -> Result<bool> {
    Ok(!git(worktree, &["status", "--porcelain"])?.trim().is_empty())
}

pub fn remove_worktree(repo: &Path, worktree: &Path, force: bool) -> Result<()> {
    let worktree = worktree.to_string_lossy();
    let mut args = vec!["worktree", "remove"];
    if force {
        args.push("--force");
    }
    args.push(&worktree);
    git(repo, &args).map(|_| ())
}

pub fn diff(worktree: &Path, base: &str) -> Result<String> {
    let merge_base = git(worktree, &["merge-base", base, "HEAD"])?;
    let mut patch = git(worktree, &["diff", "--no-color", "--no-ext-diff", merge_base.trim()])?;
    let untracked = git(worktree, &["ls-files", "-z", "--others", "--exclude-standard"])?;
    for file in untracked.split('\0').filter(|f| !f.is_empty()) {
        // `git diff --no-index` exits 1 when the files differ, so its status is ignored.
        let out = Command::new("git")
            .arg("-C")
            .arg(worktree)
            .args(["diff", "--no-color", "--no-ext-diff", "--no-index", "--", "/dev/null", file])
            .output()?;
        patch.push_str(&String::from_utf8_lossy(&out.stdout));
    }
    Ok(patch)
}
