use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use asterism_proto::rpc::ErrorKind;

use super::catalog::{parse_index, safe_relative, StoreIndex};
use crate::error::{Error, Result};
use crate::git::{clone_env, truncate, GitEnv};

pub const NETWORK_TIMEOUT: Duration = Duration::from_secs(120);
pub const README_LIMIT: usize = 64 * 1024;
const INDEX_LIMIT: u64 = 4 * 1024 * 1024;

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidParams, message)
}

fn read_pipe(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<Vec<u8>> {
    std::thread::spawn(move || {
        let mut bytes = Vec::new();
        if let Some(mut pipe) = pipe {
            let _ = pipe.read_to_end(&mut bytes);
        }
        bytes
    })
}

/// Runs git with asterism's non-interactive environment, killing it after `timeout`.
fn git_timed(dir: Option<&Path>, args: &[&str], env: &GitEnv, timeout: Duration) -> Result<String> {
    let mut cmd = Command::new("git");
    if let Some(dir) = dir {
        cmd.arg("-C").arg(dir);
    }
    let mut child = cmd
        .args(args)
        .envs(clone_env(env).iter().map(|(k, v)| (k, v)))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let (stdout, stderr) = (read_pipe(child.stdout.take()), read_pipe(child.stderr.take()));
    let deadline = Instant::now() + timeout;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {}
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(e.into());
            }
        }
        if Instant::now() >= deadline {
            let _ = child.kill();
            let _ = child.wait();
            return Err(Error::new(ErrorKind::Timeout, format!("git {} timed out after {}s", args[0], timeout.as_secs())));
        }
        std::thread::sleep(Duration::from_millis(50));
    };
    let (stdout, stderr) = (stdout.join().unwrap_or_default(), stderr.join().unwrap_or_default());
    if status.success() {
        Ok(String::from_utf8_lossy(&stdout).into_owned())
    } else {
        Err(Error::new(ErrorKind::Git, truncate(&String::from_utf8_lossy(&stderr))))
    }
}

fn git_local(dir: &Path, args: &[&str], env: &GitEnv) -> Result<String> {
    git_timed(Some(dir), args, env, NETWORK_TIMEOUT)
}

pub fn clone_store(source: &str, target: &Path, env: &GitEnv) -> Result<()> {
    git_timed(None, &["clone", "-q", "--depth", "1", "--", source, &target.to_string_lossy()], env, NETWORK_TIMEOUT).map(|_| ())
}

/// Moves the checkout to the remote's current HEAD; a failure leaves the old checkout untouched.
pub fn refresh_store(dir: &Path, env: &GitEnv) -> Result<()> {
    git_local(dir, &["fetch", "-q", "--depth", "1", "origin", "HEAD"], env)?;
    git_local(dir, &["reset", "-q", "--hard", "FETCH_HEAD"], env).map(|_| ())
}

/// Undoes the last `refresh_store` reset.
pub fn restore_store(dir: &Path, env: &GitEnv) -> Result<()> {
    git_local(dir, &["reset", "-q", "--hard", "ORIG_HEAD"], env).map(|_| ())
}

/// Reads at most `limit` bytes of a regular file; symlinks and special files are refused.
fn read_regular(path: &Path, limit: u64) -> Option<Vec<u8>> {
    if !std::fs::symlink_metadata(path).ok()?.is_file() {
        return None;
    }
    let mut bytes = Vec::new();
    std::fs::File::open(path).ok()?.take(limit).read_to_end(&mut bytes).ok()?;
    Some(bytes)
}

pub fn read_index(dir: &Path) -> Result<StoreIndex> {
    let path = dir.join("store.json");
    if !std::fs::symlink_metadata(&path).map(|m| m.is_file()).unwrap_or(false) {
        return Err(invalid("store.json must be a regular file".to_string()));
    }
    let bytes = read_regular(&path, INDEX_LIMIT + 1).ok_or_else(|| invalid(format!("cannot read {}", path.display())))?;
    if bytes.len() as u64 > INDEX_LIMIT {
        return Err(invalid("store.json is too large".to_string()));
    }
    let text = String::from_utf8(bytes).map_err(|_| invalid("store.json is not valid UTF-8".to_string()))?;
    parse_index(&text).map_err(|e| invalid(format!("{}: {e}", path.display())))
}

fn cache_key(url: &str) -> String {
    // ponytail: DefaultHasher is not stable across Rust releases; a new key only costs a re-clone.
    let mut hasher = DefaultHasher::new();
    url.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// A full clone (not shallow) so tags and commit SHAs resolve the same way for any remote.
pub fn checkout_git(cache: &Path, url: &str, git_ref: &str, env: &GitEnv) -> Result<PathBuf> {
    if git_ref.starts_with('-') {
        return Err(invalid(format!("invalid ref {git_ref:?}")));
    }
    let dir = cache.join(cache_key(url));
    let reusable = dir.join(".git").exists()
        && git_local(&dir, &["config", "--get", "remote.origin.url"], env).map(|u| u.trim() == url).unwrap_or(false);
    if reusable {
        git_local(&dir, &["fetch", "-q", "--tags", "--force", "--prune", "origin"], env)?;
    } else {
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(cache)?;
        git_timed(None, &["clone", "-q", "--no-checkout", "--", url, &dir.to_string_lossy()], env, NETWORK_TIMEOUT)?;
    }
    let target = if is_full_sha(git_ref) {
        format!("{git_ref}^{{commit}}")
    } else if git_local(&dir, &["show-ref", "--verify", "--quiet", &format!("refs/tags/{git_ref}")], env).is_ok() {
        format!("refs/tags/{git_ref}^{{commit}}")
    } else if git_local(&dir, &["show-ref", "--verify", "--quiet", &format!("refs/remotes/origin/{git_ref}")], env).is_ok() {
        return Err(invalid(format!("ref {git_ref} is a branch; store entries must pin a tag or commit")));
    } else {
        return Err(invalid(format!("ref {git_ref} must be a tag or a full commit SHA")));
    };
    let commit = git_local(&dir, &["rev-parse", "--verify", "--quiet", &target], env)
        .map_err(|_| invalid(format!("unknown ref {git_ref} in {url}")))?;
    if is_full_sha(git_ref) && commit.trim() != git_ref {
        return Err(invalid(format!("ref {git_ref} does not resolve to that commit")));
    }
    git_local(&dir, &["checkout", "-q", "--force", "--detach", commit.trim()], env)?;
    Ok(dir)
}

fn is_full_sha(s: &str) -> bool {
    s.len() == 40 && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// `rel` inside `root`, with symlinks resolved, so an entry can never point outside its checkout.
pub fn plugin_dir(root: &Path, rel: &str) -> Result<PathBuf> {
    if !safe_relative(rel) {
        return Err(invalid(format!("plugin path {rel:?} must be relative and stay inside the store")));
    }
    if rel.split('/').any(|c| c == ".git") {
        return Err(invalid(format!("plugin path {rel:?} must not point into .git")));
    }
    let root = root.canonicalize()?;
    let dir = root.join(rel).canonicalize().map_err(|e| invalid(format!("plugin path {rel:?}: {e}")))?;
    if !dir.starts_with(&root) || !dir.is_dir() {
        return Err(invalid(format!("plugin path {rel:?} is not a directory inside the store")));
    }
    Ok(dir)
}

/// Copies a plugin without `.git`; symlinks are refused because they could point anywhere.
pub fn copy_tree(from: &Path, to: &Path) -> Result<()> {
    std::fs::create_dir_all(to)?;
    for entry in std::fs::read_dir(from)? {
        let entry = entry?;
        if entry.file_name() == ".git" {
            continue;
        }
        let kind = entry.file_type()?;
        let target = to.join(entry.file_name());
        if kind.is_symlink() {
            return Err(invalid(format!("{} is a symlink; plugins may not contain symlinks", entry.path().display())));
        } else if kind.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else {
            std::fs::copy(entry.path(), &target)?;
        }
    }
    Ok(())
}

pub fn readme(dir: &Path) -> Option<String> {
    let bytes = read_regular(&dir.join("README.md"), README_LIMIT as u64 + 4)?;
    let mut text = String::from_utf8_lossy(&bytes).into_owned();
    if text.len() > README_LIMIT {
        let mut cut = README_LIMIT;
        while !text.is_char_boundary(cut) {
            cut -= 1;
        }
        text.truncate(cut);
    }
    Some(text)
}
