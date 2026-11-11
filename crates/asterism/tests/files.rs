mod common;

use asterism_core::{files, git};
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::DiffStat;
use common::{init_repo, run_git};
use std::fs;

fn names(entries: &[asterism_proto::types::DirEntry]) -> Vec<&str> {
    entries.iter().map(|e| e.name.as_str()).collect()
}

#[test]
fn lists_dirs_first_and_hides_git() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    fs::write(repo.path().join("b.txt"), "x").unwrap();
    fs::create_dir(repo.path().join("a")).unwrap();
    let entries = files::list_dir(repo.path(), "", &[]).unwrap();
    assert_eq!(names(&entries), ["a", "b.txt", "README.md"]);
}

#[test]
fn marks_ignored_and_changed() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    fs::create_dir(root.join("src")).unwrap();
    fs::write(root.join("src/x.rs"), "a\n").unwrap();
    fs::write(root.join(".gitignore"), "dist/\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-q", "-m", "more"]);
    fs::create_dir(root.join("dist")).unwrap();
    fs::write(root.join("dist/o.js"), "x").unwrap();
    fs::write(root.join("src/x.rs"), "b\n").unwrap();
    fs::write(root.join("new.md"), "n\n").unwrap();

    let changed = git::changed_paths(root, "main").unwrap();
    let entries = files::list_dir(root, "", &changed).unwrap();
    let get = |n: &str| entries.iter().find(|e| e.name == n).unwrap();
    assert!(get("dist").ignored && !get("dist").changed);
    assert!(get("src").changed && !get("src").ignored);
    assert!(get("new.md").changed);
    assert!(!get("README.md").changed);

    let sub = files::list_dir(root, "src", &changed).unwrap();
    assert_eq!(sub[0].path, "src/x.rs");
    assert!(sub[0].changed);
}

#[cfg(unix)]
#[test]
fn rejects_paths_outside_worktree() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    std::os::unix::fs::symlink("/tmp", repo.path().join("out")).unwrap();
    for path in ["../", "/etc", "out"] {
        let err = files::list_dir(repo.path(), path, &[]).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidParams, "{path}");
    }
}

#[test]
fn diff_stat_counts_tracked_and_untracked() {
    let repo = tempfile::tempdir().unwrap();
    let root = repo.path();
    init_repo(root);
    assert_eq!(git::diff_stat(root, "main").unwrap(), DiffStat::default());

    fs::write(root.join("two.txt"), "a\nb\n").unwrap();
    run_git(root, &["add", "."]);
    run_git(root, &["commit", "-q", "-m", "two"]);
    fs::write(root.join("two.txt"), "c\nd\n").unwrap();
    fs::write(root.join("new.txt"), "1\n2\n3\n").unwrap();
    fs::write(root.join("huge.txt"), "x\n".repeat(1024 * 1024 + 1)).unwrap();
    let stat = git::diff_stat(root, "main").unwrap();
    assert_eq!(
        stat,
        DiffStat {
            added: 5,
            removed: 2
        }
    );
}
