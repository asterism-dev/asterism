use std::fs;
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::UNIX_EPOCH;

use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{DirEntry, DirEntryKind, TaskFileResult};

use crate::error::{Error, Result};

pub(crate) const MAX_BYTES: u64 = 2 * 1024 * 1024;
pub(crate) const SNIFF_BYTES: usize = 8 * 1024;

/// Reads a text file for display; relative paths resolve against `worktree`, `~/` against `home`.
pub fn read(
    worktree: &Path,
    home: Option<&Path>,
    path: &str,
    known_mtime: Option<i64>,
) -> Result<TaskFileResult> {
    let full = fs::canonicalize(resolve(worktree, home, path)?).map_err(|e| io_error(path, e))?;
    let meta = fs::metadata(&full).map_err(|e| io_error(path, e))?;
    if !meta.is_file() {
        return Err(invalid(format!("{path} is not a regular file")));
    }
    if meta.len() > MAX_BYTES {
        return Err(invalid(format!("{path} is larger than 2 MB")));
    }
    let mtime = meta
        .modified()
        .ok()
        .and_then(|t| t.duration_since(UNIX_EPOCH).ok())
        .map_or(0, |d| d.as_millis() as i64);
    let shown = display(worktree, home, &full);
    if known_mtime == Some(mtime) {
        return Ok(TaskFileResult {
            path: shown,
            mtime,
            content: None,
        });
    }
    let mut bytes = Vec::new();
    fs::File::open(&full)
        .and_then(|f| f.take(MAX_BYTES + 1).read_to_end(&mut bytes))
        .map_err(|e| io_error(path, e))?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid(format!("{path} is larger than 2 MB")));
    }
    if bytes[..bytes.len().min(SNIFF_BYTES)].contains(&0) {
        return Err(invalid(format!("{path} is a binary file")));
    }
    let content =
        String::from_utf8(bytes).map_err(|_| invalid(format!("{path} is not valid UTF-8")))?;
    Ok(TaskFileResult {
        path: shown,
        mtime,
        content: Some(content),
    })
}

/// Lists one directory of the worktree, flagging gitignored and changed entries.
pub fn list_dir(worktree: &Path, rel: &str, changed: &[String]) -> Result<Vec<DirEntry>> {
    if Path::new(rel).is_absolute() {
        return Err(invalid(format!("{rel} is not a worktree-relative path")));
    }
    let root = fs::canonicalize(worktree).map_err(|e| io_error(rel, e))?;
    let dir = fs::canonicalize(root.join(rel)).map_err(|e| io_error(rel, e))?;
    if !dir.starts_with(&root) {
        return Err(invalid(format!("{rel} is outside the worktree")));
    }
    let prefix = dir
        .strip_prefix(&root)
        .map(|p| p.to_string_lossy().replace('\\', "/"))
        .unwrap_or_default();
    let mut entries = Vec::new();
    for item in fs::read_dir(&dir).map_err(|e| io_error(rel, e))? {
        let item = item.map_err(|e| io_error(rel, e))?;
        let name = item.file_name().to_string_lossy().into_owned();
        if name == ".git" {
            continue;
        }
        let is_dir = fs::metadata(item.path()).is_ok_and(|m| m.is_dir());
        let path = if prefix.is_empty() {
            name.clone()
        } else {
            format!("{prefix}/{name}")
        };
        let dir_prefix = format!("{path}/");
        entries.push(DirEntry {
            changed: changed
                .iter()
                .any(|c| *c == path || c.starts_with(&dir_prefix)),
            kind: if is_dir {
                DirEntryKind::Dir
            } else {
                DirEntryKind::File
            },
            ignored: false,
            name,
            path,
        });
    }
    let ignored = ignored_paths(&root, &entries);
    for entry in &mut entries {
        entry.ignored = ignored.contains(&entry.path);
    }
    entries.sort_by_cached_key(|e| (e.kind == DirEntryKind::File, e.name.to_lowercase()));
    Ok(entries)
}

/// Asks git which of `entries` are ignored; any failure (e.g. not a repo) means none are.
fn ignored_paths(root: &Path, entries: &[DirEntry]) -> std::collections::HashSet<String> {
    let mut input = Vec::new();
    for e in entries {
        input.extend_from_slice(e.path.as_bytes());
        input.push(0);
    }
    let child = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(["check-ignore", "--stdin", "-z"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn();
    let Ok(mut child) = child else {
        return Default::default();
    };
    // Written from a thread so a large listing cannot deadlock against the stdout pipe.
    let writer = child
        .stdin
        .take()
        .map(|mut stdin| std::thread::spawn(move || stdin.write_all(&input)));
    let out = child.wait_with_output();
    if let Some(writer) = writer {
        let _ = writer.join();
    }
    out.map(|o| {
        String::from_utf8_lossy(&o.stdout)
            .split('\0')
            .filter(|p| !p.is_empty())
            .map(String::from)
            .collect()
    })
    .unwrap_or_default()
}

fn resolve(worktree: &Path, home: Option<&Path>, path: &str) -> Result<PathBuf> {
    match path.strip_prefix("~/") {
        Some(rest) => Ok(home
            .ok_or_else(|| invalid("HOME is not set".into()))?
            .join(rest)),
        // `join` replaces the base when `path` is absolute.
        None => Ok(worktree.join(path)),
    }
}

fn display(worktree: &Path, home: Option<&Path>, full: &Path) -> String {
    let canonical = |p: &Path| fs::canonicalize(p).unwrap_or_else(|_| p.to_path_buf());
    if let Ok(rel) = full.strip_prefix(canonical(worktree)) {
        return rel.display().to_string();
    }
    match home.and_then(|h| full.strip_prefix(canonical(h)).ok()) {
        Some(rel) => format!("~/{}", rel.display()),
        None => full.display().to_string(),
    }
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidParams, message)
}

fn io_error(path: &str, e: io::Error) -> Error {
    match e.kind() {
        io::ErrorKind::NotFound => Error::new(ErrorKind::NotFound, format!("{path} not found")),
        _ => Error::new(ErrorKind::Internal, format!("{path}: {e}")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(dir: &Path, rel: &str, bytes: &[u8]) -> PathBuf {
        let file = dir.join(rel);
        fs::create_dir_all(file.parent().unwrap()).unwrap();
        fs::write(&file, bytes).unwrap();
        file
    }

    #[test]
    fn reads_relative_paths_against_the_worktree() {
        let worktree = tempfile::tempdir().unwrap();
        write(worktree.path(), "src/a.ts", b"let a = 1;\n");
        let file = read(worktree.path(), None, "src/a.ts", None).unwrap();
        assert_eq!(file.path, "src/a.ts");
        assert_eq!(file.content.as_deref(), Some("let a = 1;\n"));
    }

    #[cfg(unix)]
    #[test]
    fn shows_paths_relative_to_a_symlinked_worktree() {
        let real = tempfile::tempdir().unwrap();
        let links = tempfile::tempdir().unwrap();
        let worktree = links.path().join("wt");
        std::os::unix::fs::symlink(real.path(), &worktree).unwrap();
        write(real.path(), "a.rs", b"fn main() {}\n");
        assert_eq!(read(&worktree, None, "a.rs", None).unwrap().path, "a.rs");
    }

    #[test]
    fn reads_absolute_paths_outside_the_worktree() {
        let worktree = tempfile::tempdir().unwrap();
        let other = tempfile::tempdir().unwrap();
        let file = write(other.path(), "notes.md", b"# hi\n");
        let result = read(worktree.path(), None, file.to_str().unwrap(), None).unwrap();
        assert_eq!(
            result.path,
            fs::canonicalize(&file).unwrap().display().to_string()
        );
        assert_eq!(result.content.as_deref(), Some("# hi\n"));
    }

    #[test]
    fn expands_home_and_shows_it_as_tilde() {
        let worktree = tempfile::tempdir().unwrap();
        let home = tempfile::tempdir().unwrap();
        write(home.path(), ".zshrc", b"export A=1\n");
        let result = read(worktree.path(), Some(home.path()), "~/.zshrc", None).unwrap();
        assert_eq!(result.path, "~/.zshrc");
        assert_eq!(result.content.as_deref(), Some("export A=1\n"));
    }

    #[test]
    fn empty_files_have_empty_content() {
        let worktree = tempfile::tempdir().unwrap();
        write(worktree.path(), "empty.txt", b"");
        assert_eq!(
            read(worktree.path(), None, "empty.txt", None)
                .unwrap()
                .content
                .as_deref(),
            Some("")
        );
    }

    #[test]
    fn rejects_directories_large_binary_and_non_utf8_files() {
        let worktree = tempfile::tempdir().unwrap();
        write(worktree.path(), "bin.dat", &[0x7f, 0x45, 0x00, 0x01]);
        write(worktree.path(), "latin1.txt", &[0x66, 0xe9, 0x0a]);
        write(
            worktree.path(),
            "big.txt",
            &vec![b'a'; MAX_BYTES as usize + 1],
        );
        fs::create_dir(worktree.path().join("dir")).unwrap();
        for path in ["bin.dat", "latin1.txt", "big.txt", "dir"] {
            let err = read(worktree.path(), None, path, None).unwrap_err();
            assert_eq!(err.kind, ErrorKind::InvalidParams, "{path}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn rejects_device_files() {
        let worktree = tempfile::tempdir().unwrap();
        assert_eq!(
            read(worktree.path(), None, "/dev/null", None)
                .unwrap_err()
                .kind,
            ErrorKind::InvalidParams
        );
    }

    #[test]
    fn missing_files_are_not_found() {
        let worktree = tempfile::tempdir().unwrap();
        assert_eq!(
            read(worktree.path(), None, "nope.ts", None)
                .unwrap_err()
                .kind,
            ErrorKind::NotFound
        );
    }

    #[test]
    fn skips_the_content_when_the_mtime_is_unchanged() {
        let worktree = tempfile::tempdir().unwrap();
        write(worktree.path(), "a.ts", b"x\n");
        let first = read(worktree.path(), None, "a.ts", None).unwrap();
        assert_eq!(
            read(worktree.path(), None, "a.ts", Some(first.mtime))
                .unwrap()
                .content,
            None
        );
        let stale = read(worktree.path(), None, "a.ts", Some(first.mtime - 1)).unwrap();
        assert_eq!(stale.content.as_deref(), Some("x\n"));
    }
}
