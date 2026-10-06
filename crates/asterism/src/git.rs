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

/// Rejects names git would refuse or expand (`@{-1}`) and names that read as options.
pub fn check_branch_name(repo: &Path, name: &str) -> Result<()> {
    if name.starts_with('-') || name.contains("@{") || git(repo, &["check-ref-format", "--branch", name]).is_err() {
        return Err(Error::new(ErrorKind::InvalidParams, format!("invalid branch name {name:?}")));
    }
    Ok(())
}

pub fn branch_exists(repo: &Path, branch: &str) -> bool {
    git(repo, &["rev-parse", "--verify", "--quiet", &format!("refs/heads/{branch}")]).is_ok()
}

/// Commits on `branch` that are not on `base`; 0 when either is missing.
pub fn unmerged_commits(repo: &Path, base: &str, branch: &str) -> u32 {
    git(repo, &["rev-list", "--count", &format!("{base}..{branch}")])
        .ok()
        .and_then(|out| out.trim().parse().ok())
        .unwrap_or(0)
}

pub fn delete_branch(repo: &Path, branch: &str) -> Result<()> {
    git(repo, &["branch", "-D", branch]).map(|_| ())
}

pub fn add_existing_worktree(repo: &Path, path: &Path, branch: &str) -> Result<()> {
    git(repo, &["worktree", "add", "-q", &path.to_string_lossy(), branch]).map(|_| ())
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

pub type GitEnv = [(String, String)];

const MESSAGE_LIMIT: usize = 2_000;

pub fn truncate(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.chars().count() <= MESSAGE_LIMIT {
        trimmed.to_string()
    } else {
        format!("{}…", trimmed.chars().take(MESSAGE_LIMIT).collect::<String>())
    }
}

const DEFAULT_SSH_COMMAND: &str = "ssh -o BatchMode=yes -o ConnectTimeout=30 -o ServerAliveInterval=15 -o ServerAliveCountMax=4";

fn has_ssh_command_config(extra: &GitEnv) -> bool {
    Command::new("git")
        .args(["config", "--get", "core.sshCommand"])
        .envs(extra.iter().map(|(k, v)| (k, v)))
        .stdin(std::process::Stdio::null())
        .output()
        .map(|out| out.status.success())
        .unwrap_or(false)
}

/// Clones must never wait for a password prompt nobody can answer, nor hang on a stalled network.
pub fn clone_env(extra: &GitEnv) -> Vec<(String, String)> {
    let mut env = vec![
        ("GIT_TERMINAL_PROMPT".to_string(), "0".to_string()),
        ("GCM_INTERACTIVE".to_string(), "never".to_string()),
        ("GIT_HTTP_LOW_SPEED_LIMIT".to_string(), "1000".to_string()),
        ("GIT_HTTP_LOW_SPEED_TIME".to_string(), "60".to_string()),
    ];
    let user_ssh = extra.iter().any(|(k, _)| k == "GIT_SSH_COMMAND") || std::env::var_os("GIT_SSH_COMMAND").is_some();
    if !user_ssh && !has_ssh_command_config(extra) {
        env.push(("GIT_SSH_COMMAND".to_string(), DEFAULT_SSH_COMMAND.to_string()));
    }
    env.extend(extra.iter().cloned());
    env
}

fn run_with_env(dir: Option<&Path>, args: &[&str], env: &GitEnv) -> Result<String> {
    let mut cmd = Command::new("git");
    if let Some(dir) = dir {
        cmd.arg("-C").arg(dir);
    }
    let out = cmd.args(args).envs(env.iter().map(|(k, v)| (k, v))).stdin(std::process::Stdio::null()).output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(Error::new(ErrorKind::Git, truncate(&String::from_utf8_lossy(&out.stderr))))
    }
}

pub fn remote_url(repo: &Path, remote: &str) -> Option<String> {
    git(repo, &["remote", "get-url", remote]).ok().map(|url| url.trim().to_string()).filter(|url| !url.is_empty())
}

pub fn clone_url(url: &str, target: &Path, extra: &GitEnv) -> Result<()> {
    let target = target.to_string_lossy();
    run_with_env(None, &["clone", "-q", "--", url, &target], &clone_env(extra)).map(|_| ())
}

pub fn init_with_readme(dir: &Path, name: &str, extra: &GitEnv) -> Result<()> {
    std::fs::create_dir_all(dir)?;
    run_with_env(Some(dir), &["init", "-q", "-b", "main"], extra)?;
    std::fs::write(dir.join("README.md"), format!("# {name}\n"))?;
    run_with_env(Some(dir), &["add", "README.md"], extra)?;
    run_with_env(Some(dir), &["commit", "-q", "-m", "Initial commit"], extra).map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn isolated() -> Vec<(String, String)> {
        [
            ("GIT_CONFIG_GLOBAL", "/dev/null"),
            ("GIT_CONFIG_NOSYSTEM", "1"),
            ("GIT_AUTHOR_NAME", "t"),
            ("GIT_AUTHOR_EMAIL", "t@example.com"),
            ("GIT_COMMITTER_NAME", "t"),
            ("GIT_COMMITTER_EMAIL", "t@example.com"),
        ]
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .to_vec()
    }

    #[test]
    fn clone_env_is_non_interactive() {
        let env = clone_env(&[]);
        for pair in [("GIT_TERMINAL_PROMPT", "0"), ("GCM_INTERACTIVE", "never"), ("GIT_HTTP_LOW_SPEED_LIMIT", "1000"), ("GIT_HTTP_LOW_SPEED_TIME", "60")] {
            assert!(env.contains(&(pair.0.into(), pair.1.into())), "{pair:?}");
        }
        let ssh = env.iter().find(|(k, _)| k == "GIT_SSH_COMMAND").map(|(_, v)| v.as_str());
        match std::env::var_os("GIT_SSH_COMMAND") {
            None => {
                let ssh = ssh.unwrap_or_default();
                assert!(ssh.contains("BatchMode=yes") && ssh.contains("ConnectTimeout=30") && ssh.contains("ServerAliveCountMax=4"));
            }
            Some(_) => assert_eq!(ssh, None),
        }
    }

    #[test]
    fn clone_env_respects_core_ssh_command() {
        let dir = tempfile::tempdir().unwrap();
        let config = dir.path().join("gitconfig");
        std::fs::write(&config, "[core]\n\tsshCommand = ssh -i /key\n").unwrap();
        let mut extra = isolated();
        extra.retain(|(k, _)| k != "GIT_CONFIG_GLOBAL");
        extra.push(("GIT_CONFIG_GLOBAL".into(), config.display().to_string()));
        assert!(!clone_env(&extra).iter().any(|(k, _)| k == "GIT_SSH_COMMAND"));
    }

    #[test]
    fn init_clone_and_remote_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let origin = dir.path().join("origin");
        init_with_readme(&origin, "demo", &isolated()).unwrap();
        assert_eq!(std::fs::read_to_string(origin.join("README.md")).unwrap(), "# demo\n");
        assert_eq!(base_ref(&origin).unwrap(), "main");

        let target = dir.path().join("clone");
        clone_url(&format!("file://{}", origin.display()), &target, &isolated()).unwrap();
        assert!(target.join("README.md").exists());
        assert!(remote_url(&target, "origin").unwrap().ends_with("origin"));
        assert_eq!(remote_url(&origin, "origin"), None);

        let err = clone_url("file:///definitely/missing.git", &dir.path().join("x"), &isolated()).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Git);
    }

    #[test]
    fn long_messages_are_truncated() {
        assert_eq!(truncate(&"x".repeat(5_000)).chars().count(), 2_001);
        assert_eq!(truncate("short"), "short");
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WorktreeEntry {
    pub path: String,
    pub head: String,
    pub branch: Option<String>,
    pub is_main: bool,
    pub locked: bool,
    pub prunable: bool,
}

/// Parses `git worktree list --porcelain`; the first entry is the main checkout.
pub fn parse_worktrees(text: &str) -> Vec<WorktreeEntry> {
    let mut list: Vec<WorktreeEntry> = Vec::new();
    for line in text.lines() {
        let (key, value) = line.split_once(' ').unwrap_or((line, ""));
        if key == "worktree" {
            list.push(WorktreeEntry {
                path: value.to_string(),
                head: String::new(),
                branch: None,
                is_main: list.is_empty(),
                locked: false,
                prunable: false,
            });
            continue;
        }
        let Some(entry) = list.last_mut() else { continue };
        match key {
            "HEAD" => entry.head = value.to_string(),
            "branch" => entry.branch = Some(value.strip_prefix("refs/heads/").unwrap_or(value).to_string()),
            "locked" => entry.locked = true,
            "prunable" => entry.prunable = true,
            _ => {}
        }
    }
    list
}

pub fn worktrees(repo: &Path) -> Result<Vec<WorktreeEntry>> {
    Ok(parse_worktrees(&git(repo, &["worktree", "list", "--porcelain"])?))
}

pub fn prune_worktrees(repo: &Path) -> Result<()> {
    git(repo, &["worktree", "prune"]).map(|_| ())
}

/// Bytes below `path` without following symlinks; skips the main checkout's `.git` object store.
pub fn dir_size(path: &Path) -> u64 {
    fn walk(path: &Path, top: bool) -> u64 {
        let Ok(entries) = std::fs::read_dir(path) else { return 0 };
        entries
            .filter_map(|e| e.ok())
            .map(|entry| match entry.file_type() {
                Ok(t) if t.is_dir() => {
                    if top && entry.file_name() == ".git" { 0 } else { walk(&entry.path(), false) }
                }
                Ok(t) if t.is_file() => entry.metadata().map(|m| m.len()).unwrap_or(0),
                _ => 0,
            })
            .sum()
    }
    walk(path, true)
}

#[cfg(test)]
mod worktree_tests {
    use super::*;

    #[test]
    fn parses_porcelain_worktree_list() {
        let text = "worktree /repo\nHEAD 1111111111\nbranch refs/heads/main\n\nworktree /wt/a\nHEAD 2222222222\nbranch refs/heads/asterism/1-a\nlocked reason\n\nworktree /wt/b\nHEAD 3333333333\ndetached\nprunable gitdir file points to non-existent location\n\n";
        let list = parse_worktrees(text);
        assert_eq!(list.len(), 3);
        assert_eq!((list[0].path.as_str(), list[0].branch.as_deref(), list[0].is_main), ("/repo", Some("main"), true));
        assert_eq!((list[1].branch.as_deref(), list[1].locked, list[1].is_main), (Some("asterism/1-a"), true, false));
        assert_eq!((list[2].branch.as_deref(), list[2].prunable, list[2].head.as_str()), (None, true, "3333333333"));
    }

    #[test]
    fn dir_size_counts_files_but_not_symlinks_or_the_main_git_dir() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("a"), vec![0u8; 100]).unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub/b"), vec![0u8; 50]).unwrap();
        std::fs::create_dir(dir.path().join(".git")).unwrap();
        std::fs::write(dir.path().join(".git/objects"), vec![0u8; 1000]).unwrap();
        std::os::unix::fs::symlink(dir.path().join("a"), dir.path().join("link")).unwrap();
        assert_eq!(dir_size(dir.path()), 150);
        assert_eq!(dir_size(&dir.path().join("missing")), 0);
    }
}
