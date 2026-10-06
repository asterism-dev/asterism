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
