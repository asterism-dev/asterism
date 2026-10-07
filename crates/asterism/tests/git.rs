mod common;

use asterism_core::git;
use asterism_proto::rpc::ErrorKind;
use common::{init_repo, run_git};

#[test]
fn toplevel_finds_repo_root_and_rejects_plain_dirs() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    std::fs::create_dir(repo.path().join("sub")).unwrap();
    let root = git::toplevel(&repo.path().join("sub")).unwrap();
    assert_eq!(root.canonicalize().unwrap(), repo.path().canonicalize().unwrap());

    let plain = tempfile::tempdir().unwrap();
    assert_eq!(git::toplevel(plain.path()).unwrap_err().kind, ErrorKind::NotARepo);
}

#[test]
fn base_ref_is_branch_or_sha_when_detached() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    assert_eq!(git::base_ref(repo.path()).unwrap(), "main");

    run_git(repo.path(), &["checkout", "-q", "--detach"]);
    let sha = git::base_ref(repo.path()).unwrap();
    assert_eq!(sha.len(), 40);
}

#[test]
fn worktree_lifecycle_and_diff() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let wt_parent = tempfile::tempdir().unwrap();
    let wt = wt_parent.path().join("nested/1-fix");

    git::add_worktree(repo.path(), "asterism/1-fix", &wt, "main").unwrap();
    assert!(wt.join("README.md").exists());
    assert!(!git::is_dirty(&wt).unwrap());

    std::fs::write(wt.join("README.md"), "changed\n").unwrap();
    std::fs::write(wt.join("new file.txt"), "fresh\n").unwrap();
    assert!(git::is_dirty(&wt).unwrap());

    let patch = git::diff(&wt, "main").unwrap();
    assert!(patch.contains("+changed"), "{patch}");
    assert!(patch.contains("+fresh"), "{patch}");

    let err = git::add_worktree(repo.path(), "asterism/1-fix", &wt_parent.path().join("other"), "main")
        .unwrap_err();
    assert_eq!(err.kind, ErrorKind::BranchExists);

    assert!(git::remove_worktree(repo.path(), &wt, false).is_err());
    git::remove_worktree(repo.path(), &wt, true).unwrap();
    assert!(!wt.exists());
}

#[test]
fn resolves_rejects_options_and_missing_refs() {
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    assert!(git::resolves(repo.path(), "main"));
    assert!(!git::resolves(repo.path(), "nope"));
    assert!(!git::resolves(repo.path(), "--all"));
}

/// A repo whose bare origin has `main` and `feature/pr`; the local remote-tracking ref of `feature/pr` is removed.
fn repo_with_origin() -> (tempfile::TempDir, tempfile::TempDir) {
    let origin = tempfile::tempdir().unwrap();
    run_git(origin.path(), &["init", "-q", "--bare", "-b", "main"]);
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    run_git(repo.path(), &["remote", "add", "origin", &origin.path().display().to_string()]);
    run_git(repo.path(), &["push", "-q", "origin", "main", "main:feature/pr"]);
    run_git(repo.path(), &["update-ref", "-d", "refs/remotes/origin/feature/pr"]);
    (origin, repo)
}

#[test]
fn origin_only_branches_check_out_with_upstream() {
    let (_origin, repo) = repo_with_origin();
    assert!(!git::remote_branch_exists(repo.path(), "origin", "feature/pr"));
    git::fetch_branch(repo.path(), "origin", "feature/pr", &[]).unwrap();
    assert!(git::remote_branch_exists(repo.path(), "origin", "feature/pr"));
    assert!(!git::branch_exists(repo.path(), "feature/pr"));
    let parent = tempfile::tempdir().unwrap();
    let wt = parent.path().join("pr");
    git::add_tracking_worktree(repo.path(), &wt, "feature/pr", "origin").unwrap();
    assert_eq!(run_git(&wt, &["rev-parse", "--abbrev-ref", "@{upstream}"]).trim(), "origin/feature/pr");
    assert!(git::fetch_branch(repo.path(), "origin", "missing", &[]).unwrap_err().message.contains("couldn't find remote ref"));
}

#[test]
fn push_upstream_publishes_and_tracks_the_branch() {
    let (origin, repo) = repo_with_origin();
    let parent = tempfile::tempdir().unwrap();
    let wt = parent.path().join("x");
    git::add_worktree(repo.path(), "asterism/x", &wt, "main").unwrap();
    git::push_upstream(&wt, "origin", "asterism/x", &[]).unwrap();
    run_git(origin.path(), &["rev-parse", "--verify", "refs/heads/asterism/x"]);
    assert_eq!(run_git(&wt, &["rev-parse", "--abbrev-ref", "@{upstream}"]).trim(), "origin/asterism/x");
    run_git(repo.path(), &["remote", "set-url", "origin", "/nonexistent/origin.git"]);
    assert!(git::push_upstream(&wt, "origin", "asterism/x", &[]).is_err());
}

#[test]
fn branches_split_into_local_and_one_remote() {
    let (_origin, repo) = repo_with_origin();
    git::fetch(repo.path(), "origin", &[]).unwrap();
    run_git(repo.path(), &["branch", "feature/local"]);
    run_git(repo.path(), &["remote", "add", "upstream", "/nonexistent"]);
    run_git(repo.path(), &["update-ref", "refs/remotes/upstream/main", "HEAD"]);
    run_git(repo.path(), &["symbolic-ref", "refs/remotes/origin/HEAD", "refs/remotes/origin/main"]);
    assert_eq!(git::local_branches(repo.path()).unwrap(), ["feature/local", "main"]);
    assert_eq!(git::remote_branches(repo.path(), "origin").unwrap(), ["origin/feature/pr", "origin/main"]);
}

#[test]
fn push_upstream_skips_pre_push_hooks() {
    use std::os::unix::fs::PermissionsExt;
    let (origin, repo) = repo_with_origin();
    let hook = repo.path().join(".git/hooks/pre-push");
    std::fs::write(&hook, "#!/bin/sh\nexit 1\n").unwrap();
    std::fs::set_permissions(&hook, std::fs::Permissions::from_mode(0o755)).unwrap();
    let parent = tempfile::tempdir().unwrap();
    let wt = parent.path().join("h");
    git::add_worktree(repo.path(), "asterism/h", &wt, "main").unwrap();
    git::push_upstream(&wt, "origin", "asterism/h", &[]).unwrap();
    run_git(origin.path(), &["rev-parse", "--verify", "refs/heads/asterism/h"]);
}
