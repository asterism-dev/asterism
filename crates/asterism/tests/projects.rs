mod common;

use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use asterism_core::daemon::{Daemon, DaemonOptions};
use asterism_core::git;
use asterism_core::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::*;
use common::{init_repo, run_git};
use tempfile::TempDir;

fn git_env() -> Vec<(String, String)> {
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

fn fake_gh(dir: &Path, create_fails: bool) -> PathBuf {
    let script = format!(
        "#!/bin/sh\ncase \"$1 $2\" in\n  \"--version \"*) exit 0 ;;\n  \"api user\") echo me; exit 0 ;;\n  \"api user/orgs\") echo acme; exit 0 ;;\n  \"repo create\") {} ;;\nesac\nexit 0\n",
        if create_fails { "echo 'boom from github' >&2; exit 1" } else { "exit 0" }
    );
    let bin = dir.join("gh");
    std::fs::write(&bin, script).unwrap();
    std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    bin
}

struct Env {
    home: TempDir,
    daemon: Arc<Daemon>,
}

fn setup(create_fails: bool) -> Env {
    let home = tempfile::tempdir().unwrap();
    let gh = fake_gh(home.path(), create_fails);
    let daemon = Daemon::with_options(
        Paths { home: home.path().join("h") },
        DaemonOptions { gh_bin: gh, git_env: git_env() },
    )
    .unwrap();
    Env { home, daemon }
}

fn bare_origin(dir: &Path) -> String {
    let work = dir.join("work");
    std::fs::create_dir_all(&work).unwrap();
    init_repo(&work);
    let bare = dir.join("remotes/acme/demo.git");
    std::fs::create_dir_all(bare.parent().unwrap()).unwrap();
    run_git(dir, &["clone", "-q", "--bare", &work.display().to_string(), &bare.display().to_string()]);
    format!("file://{}", bare.display())
}

#[tokio::test]
async fn clone_places_repo_under_owner_and_registers_it() {
    let env = setup(false);
    let url = bare_origin(env.home.path());
    let project = env.daemon.clone_project(&url).unwrap();
    let expected = env.home.path().join("h/repos/acme/demo");
    assert_eq!(Path::new(&project.path).canonicalize().unwrap(), expected.canonicalize().unwrap());
    assert!(expected.join("README.md").exists());
    assert_eq!(env.daemon.clone_project(&url).unwrap_err().kind, ErrorKind::InvalidParams);
}

#[tokio::test]
async fn failed_clone_removes_partial_target() {
    let env = setup(false);
    let err = env.daemon.clone_project("file:///definitely/missing/acme/nothing.git").unwrap_err();
    assert_eq!(err.kind, ErrorKind::Git);
    assert!(!env.home.path().join("h/repos/acme/nothing").exists());
}

#[tokio::test]
async fn create_local_repository() {
    let env = setup(false);
    let result = env.daemon.create_project(&ProjectCreateParams { name: "notes".into(), github: None }).unwrap();
    assert!(result.github_error.is_none());
    let dir = env.home.path().join("h/repos/local/notes");
    assert_eq!(std::fs::read_to_string(dir.join("README.md")).unwrap(), "# notes\n");
    assert_eq!(result.project.name, "notes");
    let err = env.daemon.create_project(&ProjectCreateParams { name: "../x".into(), github: None }).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    let err = env.daemon.create_project(&ProjectCreateParams { name: "notes".into(), github: None }).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
}

#[tokio::test]
async fn github_creation_validates_owner_and_visibility() {
    let env = setup(false);
    let target = |owner: &str, visibility| Some(GithubTarget { owner: owner.into(), visibility });
    let ok = env.daemon.create_project(&ProjectCreateParams { name: "svc".into(), github: target("acme", Visibility::Internal) }).unwrap();
    assert!(ok.github_error.is_none());
    assert!(env.home.path().join("h/repos/acme/svc/README.md").exists());
    let cased = env.daemon.create_project(&ProjectCreateParams { name: "svc2".into(), github: target("ACME", Visibility::Private) }).unwrap();
    assert!(cased.github_error.is_none());
    assert!(env.home.path().join("h/repos/acme/svc2/README.md").exists());
    let owners: Vec<_> = std::fs::read_dir(env.home.path().join("h/repos")).unwrap().map(|e| e.unwrap().file_name()).collect();
    assert!(!owners.iter().any(|o| o == "ACME"), "{owners:?}");
    for (owner, visibility) in [("stranger", Visibility::Private), ("me", Visibility::Internal)] {
        let err = env.daemon.create_project(&ProjectCreateParams { name: "x".into(), github: target(owner, visibility) }).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidParams, "{owner}");
    }
}

#[tokio::test]
async fn github_failure_keeps_local_repo() {
    let env = setup(true);
    let result = env
        .daemon
        .create_project(&ProjectCreateParams {
            name: "svc".into(),
            github: Some(GithubTarget { owner: "me".into(), visibility: Visibility::Private }),
        })
        .unwrap();
    assert!(result.github_error.unwrap().contains("boom from github"));
    assert!(env.home.path().join("h/repos/me/svc/README.md").exists());
    assert!(env.daemon.projects().unwrap().iter().any(|p| p.name == "svc"));
}

#[tokio::test]
async fn worktrees_are_grouped_by_owner_and_root_changes_keep_existing_tasks() {
    let env = setup(false);
    let url = bare_origin(env.home.path());
    let project = env.daemon.clone_project(&url).unwrap();
    let create = |title: &str| {
        env.daemon
            .create_task(TaskCreateParams { project_id: project.id, title: title.into(), prompt: None, agent: None, base: None })
            .unwrap()
            .task
    };
    let first = create("first");
    assert!(Path::new(&first.worktree_path).starts_with(env.home.path().join("h/worktrees/acme/demo")), "{}", first.worktree_path);

    let elsewhere = env.home.path().join("other-trees");
    let mut config = env.daemon.node_config().unwrap().config;
    config.paths.worktrees = elsewhere.display().to_string();
    env.daemon.set_node_config(&config).unwrap();
    let second = create("second");
    assert!(Path::new(&second.worktree_path).starts_with(elsewhere.join("acme/demo")), "{}", second.worktree_path);

    std::fs::write(Path::new(&first.worktree_path).join("new.txt"), "x\n").unwrap();
    assert!(env.daemon.diff(first.id).unwrap().patch.contains("+x"));
    env.daemon.delete_task(first.id, false).unwrap();
    assert!(!Path::new(&first.worktree_path).exists());
}

#[tokio::test]
async fn local_projects_without_remote_use_local_owner() {
    let env = setup(false);
    let repo = env.home.path().join("plain");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let project = env.daemon.add_project(&repo.display().to_string()).unwrap();
    let task = env
        .daemon
        .create_task(TaskCreateParams { project_id: project.id, title: "t".into(), prompt: None, agent: None, base: None })
        .unwrap()
        .task;
    assert!(Path::new(&task.worktree_path).starts_with(env.home.path().join("h/worktrees/local/plain")), "{}", task.worktree_path);
}

#[tokio::test]
async fn github_status_comes_from_gh() {
    let env = setup(false);
    let status = env.daemon.github_status();
    assert_eq!(status.login.as_deref(), Some("me"));
    assert_eq!(status.orgs, ["acme"]);
}

#[tokio::test]
async fn broken_config_leaves_no_orphan_task() {
    let env = setup(false);
    let repo = env.home.path().join("plain");
    std::fs::create_dir_all(&repo).unwrap();
    init_repo(&repo);
    let project = env.daemon.add_project(&repo.display().to_string()).unwrap();
    std::fs::write(env.home.path().join("h/config.toml"), "[paths\n").unwrap();
    let params = TaskCreateParams { project_id: project.id, title: "t".into(), prompt: None, agent: None, base: None };
    assert!(env.daemon.create_task(params).is_err());
    assert!(env.daemon.tasks(TaskListParams { project_id: Some(project.id), include_archived: true }).unwrap().is_empty());
}

#[tokio::test]
async fn existing_target_directory_is_left_untouched() {
    let env = setup(false);
    let dir = env.home.path().join("h/repos/local/notes");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("marker"), "mine").unwrap();
    let err = env.daemon.create_project(&ProjectCreateParams { name: "notes".into(), github: None }).unwrap_err();
    assert_eq!(err.kind, ErrorKind::InvalidParams);
    assert_eq!(std::fs::read_to_string(dir.join("marker")).unwrap(), "mine");
}

fn push_commit(origin: &str, dir: &Path, file: &str) {
    let work = dir.join(format!("pusher-{file}"));
    run_git(dir, &["clone", "-q", origin, &work.display().to_string()]);
    std::fs::write(work.join(file), "new\n").unwrap();
    run_git(&work, &["add", "."]);
    run_git(&work, &["commit", "-q", "-m", file]);
    run_git(&work, &["push", "-q", "origin", "HEAD:main"]);
}

fn create(env: &Env, project_id: i64, title: &str, base: Option<&str>) -> asterism_core::error::Result<TaskCreateResult> {
    env.daemon.create_task(TaskCreateParams {
        project_id,
        title: title.into(),
        prompt: None,
        agent: None,
        base: base.map(Into::into),
    })
}

fn has_upstream(repo: &Path, branch: &str) -> bool {
    std::process::Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(["rev-parse", "--abbrev-ref", &format!("{branch}@{{upstream}}")])
        .output()
        .unwrap()
        .status
        .success()
}

#[tokio::test]
async fn task_starts_from_fetched_origin_without_touching_local_main() {
    let env = setup(false);
    let url = bare_origin(env.home.path());
    let project = env.daemon.clone_project(&url).unwrap();
    let repo = Path::new(&project.path);
    push_commit(&url, env.home.path(), "remote.txt");
    let local_main = run_git(repo, &["rev-parse", "main"]);

    let branches = env.daemon.project_branches(project.id).unwrap();
    assert_eq!(branches.fetch_error, None);
    assert_eq!(branches.default.as_deref(), Some("origin/main"));
    assert_eq!(branches.automatic.as_deref(), Some("origin/main"));
    assert!(branches.branches.contains(&"main".to_string()), "{:?}", branches.branches);
    assert!(branches.branches.contains(&"origin/main".to_string()), "{:?}", branches.branches);
    assert!(!branches.branches.iter().any(|b| b.ends_with("HEAD")), "{:?}", branches.branches);

    let task = create(&env, project.id, "t", None).unwrap().task;
    assert_eq!(task.base_branch, "origin/main");
    assert!(Path::new(&task.worktree_path).join("remote.txt").exists());
    assert_eq!(run_git(repo, &["rev-parse", "main"]), local_main);
    assert!(!has_upstream(repo, &task.branch));
}

#[tokio::test]
async fn explicit_base_is_used_and_bad_bases_leave_no_task() {
    let env = setup(false);
    let url = bare_origin(env.home.path());
    let project = env.daemon.clone_project(&url).unwrap();
    let repo = Path::new(&project.path);
    run_git(repo, &["checkout", "-q", "-b", "feature"]);
    std::fs::write(repo.join("feature.txt"), "f\n").unwrap();
    run_git(repo, &["add", "."]);
    run_git(repo, &["commit", "-q", "-m", "feature"]);
    run_git(repo, &["checkout", "-q", "main"]);

    let task = create(&env, project.id, "on feature", Some("feature")).unwrap().task;
    assert_eq!(task.base_branch, "feature");
    assert!(Path::new(&task.worktree_path).join("feature.txt").exists());

    let blank = create(&env, project.id, "blank base", Some("  ")).unwrap().task;
    assert_eq!(blank.base_branch, "origin/main");

    for bad in ["nope", "--orphan"] {
        let err = create(&env, project.id, "bad", Some(bad)).unwrap_err();
        assert_eq!(err.kind, ErrorKind::Git, "{bad}");
        assert!(err.message.contains(bad), "{}", err.message);
    }
    let tasks = env.daemon.tasks(TaskListParams { project_id: Some(project.id), include_archived: true }).unwrap();
    assert_eq!(tasks.len(), 2);
}

#[tokio::test]
async fn empty_repository_gives_clear_error_and_no_task() {
    let env = setup(false);
    let repo = env.home.path().join("empty");
    std::fs::create_dir_all(&repo).unwrap();
    run_git(&repo, &["init", "-q", "-b", "main"]);
    let project = env.daemon.add_project(&repo.display().to_string()).unwrap();

    let branches = env.daemon.project_branches(project.id).unwrap();
    assert_eq!((branches.default, branches.fetch_error), (None, None));
    assert!(branches.branches.is_empty());

    let err = create(&env, project.id, "t", None).unwrap_err();
    assert_eq!(err.kind, ErrorKind::Git);
    assert!(err.message.contains("no commits yet"), "{}", err.message);
    assert!(env.daemon.tasks(TaskListParams { project_id: Some(project.id), include_archived: true }).unwrap().is_empty());
}

#[tokio::test]
async fn configured_default_base_is_used_until_it_disappears() {
    let env = setup(false);
    let url = bare_origin(env.home.path());
    let project = env.daemon.clone_project(&url).unwrap();
    let repo = Path::new(&project.path);
    run_git(repo, &["branch", "develop"]);

    let updated = env.daemon.update_project(ProjectUpdateParams { project_id: project.id, default_base: Some("develop".into()) }).unwrap();
    assert_eq!(updated.default_base.as_deref(), Some("develop"));
    let branches = env.daemon.project_branches(project.id).unwrap();
    assert_eq!((branches.default.as_deref(), branches.configured.as_deref()), (Some("develop"), Some("develop")));
    assert_eq!(branches.automatic.as_deref(), Some("origin/main"));
    assert_eq!(create(&env, project.id, "t", None).unwrap().task.base_branch, "develop");

    run_git(repo, &["branch", "-D", "develop"]);
    let branches = env.daemon.project_branches(project.id).unwrap();
    assert_eq!((branches.default.as_deref(), branches.configured.as_deref()), (Some("origin/main"), Some("develop")));
    assert_eq!(create(&env, project.id, "t2", None).unwrap().task.base_branch, "origin/main");

    let cleared = env.daemon.update_project(ProjectUpdateParams { project_id: project.id, default_base: None }).unwrap();
    assert_eq!(cleared.default_base, None);
}

#[tokio::test]
async fn fetch_failure_is_reported_and_missing_origin_head_falls_back() {
    let env = setup(false);
    let url = bare_origin(env.home.path());
    let project = env.daemon.clone_project(&url).unwrap();
    let repo = Path::new(&project.path);
    run_git(repo, &["remote", "set-head", "origin", "-d"]);
    run_git(repo, &["remote", "set-url", "origin", "file:///definitely/missing/repo.git"]);

    let branches = env.daemon.project_branches(project.id).unwrap();
    assert!(branches.fetch_error.is_some());
    assert_eq!(branches.default.as_deref(), Some("origin/main"));
    assert!(!branches.branches.is_empty());
}

#[tokio::test]
async fn pruned_base_still_counts_unmerged_commits() {
    let env = setup(false);
    let url = bare_origin(env.home.path());
    let project = env.daemon.clone_project(&url).unwrap();
    let pusher = env.home.path().join("pusher");
    run_git(env.home.path(), &["clone", "-q", &url, &pusher.display().to_string()]);
    run_git(&pusher, &["push", "-q", "origin", "HEAD:feature"]);
    env.daemon.project_branches(project.id).unwrap();

    let task = create(&env, project.id, "on feature", Some("origin/feature")).unwrap().task;
    let worktree = Path::new(&task.worktree_path);
    std::fs::write(worktree.join("work.txt"), "w\n").unwrap();
    run_git(worktree, &["add", "."]);
    run_git(worktree, &["commit", "-q", "-m", "work"]);

    run_git(&pusher, &["push", "-q", "origin", "--delete", "feature"]);
    env.daemon.project_branches(project.id).unwrap();
    assert!(!git::resolves(Path::new(&project.path), "origin/feature"));

    assert_eq!(env.daemon.delete_check(task.id).unwrap().unmerged_commits, 1);
    assert!(env.daemon.diff(task.id).is_ok());
}

#[tokio::test]
async fn relative_bases_are_stored_as_commits() {
    let env = setup(false);
    let url = bare_origin(env.home.path());
    let project = env.daemon.clone_project(&url).unwrap();
    let head = run_git(Path::new(&project.path), &["rev-parse", "HEAD"]).trim().to_string();
    assert_eq!(create(&env, project.id, "from head", Some("HEAD")).unwrap().task.base_branch, head);
    assert_eq!(create(&env, project.id, "from main", Some("main")).unwrap().task.base_branch, "main");
}
