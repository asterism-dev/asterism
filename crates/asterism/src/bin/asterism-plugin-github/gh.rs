use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use asterism_plugin::protocol::BranchPr;
use asterism_plugin::{ErrorKind, RpcError};
use asterism_proto::types::{ChecksState, ForgeRepo, ForgeStatus, Issue, IssueHit, PrChecks, PrState, PullRequest, ReviewState, Visibility};
use serde_json::Value;

const REPO_LIMIT: &str = "200";
const METADATA_TIMEOUT: Duration = Duration::from_secs(20);
// Stays under the daemon's 20 s call cap so status answers instead of timing out.
const STATUS_BUDGET: Duration = Duration::from_secs(15);
const MESSAGE_LIMIT: usize = 2_000;
const ISSUE_LIMIT: &str = "50";
// Stays under the daemon's 20 s call cap.
const ISSUE_TIMEOUT: Duration = Duration::from_secs(15);

fn truncate(message: &str) -> String {
    let trimmed = message.trim();
    if trimmed.chars().count() <= MESSAGE_LIMIT {
        trimmed.to_string()
    } else {
        format!("{}…", trimmed.chars().take(MESSAGE_LIMIT).collect::<String>())
    }
}

fn run(gh: &Path, args: &[&str], env: &[(String, String)]) -> Result<String, RpcError> {
    let mut cmd = Command::new(gh);
    cmd.args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null());
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd.output().map_err(|e| RpcError::new(ErrorKind::Git, e.to_string()))?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(RpcError::new(ErrorKind::Git, truncate(&String::from_utf8_lossy(&out.stderr))))
    }
}

fn read_pipe(pipe: Option<impl Read + Send + 'static>) -> std::thread::JoinHandle<String> {
    std::thread::spawn(move || {
        let mut buf = String::new();
        if let Some(mut pipe) = pipe {
            let mut bytes = Vec::new();
            let _ = pipe.read_to_end(&mut bytes);
            buf = String::from_utf8_lossy(&bytes).into_owned();
        }
        buf
    })
}

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<Option<std::process::ExitStatus>, RpcError> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait().map_err(|e| RpcError::new(ErrorKind::Git, e.to_string()))? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn run_with_timeout(gh: &Path, args: &[&str], timeout: Duration, dir: Option<&Path>) -> Result<String, RpcError> {
    let mut cmd = Command::new(gh);
    cmd.args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    let mut child = cmd.spawn().map_err(|e| match e.kind() {
        std::io::ErrorKind::NotFound => RpcError::new(ErrorKind::Git, "the GitHub CLI (gh) is not installed"),
        _ => RpcError::new(ErrorKind::Git, e.to_string()),
    })?;
    let stdout = read_pipe(child.stdout.take());
    let stderr = read_pipe(child.stderr.take());
    let Some(status) = wait_with_timeout(&mut child, timeout)? else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(RpcError::new(ErrorKind::Git, format!("gh timed out after {}s", timeout.as_secs())));
    };
    let (stdout, stderr) = (stdout.join().unwrap_or_default(), stderr.join().unwrap_or_default());
    if status.success() {
        Ok(stdout)
    } else {
        Err(RpcError::new(ErrorKind::Git, truncate(&stderr)))
    }
}

fn metadata(gh: &Path, args: &[&str]) -> Result<String, RpcError> {
    run_with_timeout(gh, args, METADATA_TIMEOUT, None)
}

pub fn status(gh: &Path) -> ForgeStatus {
    let deadline = Instant::now() + STATUS_BUDGET;
    let call = |args: &[&str]| run_with_timeout(gh, args, deadline.saturating_duration_since(Instant::now()), None);
    if call(&["--version"]).is_err() {
        return ForgeStatus { error: Some("the GitHub CLI (gh) is not installed".into()), ..Default::default() };
    }
    match call(&["api", "user", "--jq", ".login"]) {
        Ok(login) => ForgeStatus {
            available: true,
            authenticated: true,
            account: Some(login.trim().to_string()),
            owners: call(&["api", "user/orgs", "--paginate", "--jq", ".[].login"])
                .map(|out| out.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
                .unwrap_or_default(),
            error: None,
        },
        Err(e) => ForgeStatus { available: true, error: Some(e.message), ..Default::default() },
    }
}

pub fn repos(gh: &Path, owner: &str) -> Result<Vec<ForgeRepo>, RpcError> {
    let out = metadata(gh, &["repo", "list", owner, "--json", "nameWithOwner,description,isPrivate", "--limit", REPO_LIMIT])?;
    let items: Vec<Value> =
        serde_json::from_str(&out).map_err(|e| RpcError::new(ErrorKind::Internal, format!("unexpected gh output: {e}")))?;
    Ok(items
        .iter()
        .filter_map(|item| {
            let (owner, name) = item["nameWithOwner"].as_str()?.split_once('/')?;
            Some(ForgeRepo {
                owner: owner.to_string(),
                name: name.to_string(),
                description: item["description"].as_str().filter(|d| !d.is_empty()).map(String::from),
                private: item["isPrivate"].as_bool().unwrap_or(false),
            })
        })
        .collect())
}

pub fn resolve_owner(status: &ForgeStatus, owner: &str, visibility: Visibility) -> Result<String, RpcError> {
    let invalid = |message: String| RpcError::new(ErrorKind::InvalidParams, message);
    if !status.authenticated {
        return Err(invalid("the GitHub CLI is not logged in; run `gh auth login`".into()));
    }
    let user = status.account.as_deref().filter(|login| login.eq_ignore_ascii_case(owner));
    let Some(canonical) = user.or_else(|| status.owners.iter().map(String::as_str).find(|org| org.eq_ignore_ascii_case(owner))) else {
        return Err(invalid(format!("{owner} is not you or one of your organizations")));
    };
    if user.is_some() && visibility == Visibility::Internal {
        return Err(invalid("internal visibility needs an organization owner".into()));
    }
    Ok(canonical.to_string())
}

pub fn clone(gh: &Path, owner: &str, repo: &str, target: &Path, env: &[(String, String)]) -> Result<(), RpcError> {
    run(gh, &["repo", "clone", &format!("{owner}/{repo}"), &target.to_string_lossy()], env).map(|_| ())
}

pub fn create(gh: &Path, owner: &str, name: &str, visibility: Visibility, dir: &Path, env: &[(String, String)]) -> Result<(), RpcError> {
    let flag = match visibility {
        Visibility::Public => "--public",
        Visibility::Private => "--private",
        Visibility::Internal => "--internal",
    };
    let source = dir.to_string_lossy();
    run(gh, &["repo", "create", &format!("{owner}/{name}"), flag, "--source", &source, "--remote", "origin", "--push"], env).map(|_| ())
}

pub fn github_repo_from_url(url: &str) -> Option<String> {
    let url = url.trim();
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let (authority, path) = rest.split_once(['/', ':'])?;
    if authority.rsplit('@').next()? != "github.com" {
        return None;
    }
    let mut parts = path.trim_matches('/').trim_end_matches(".git").split('/');
    let owner = parts.next().filter(|s| !s.is_empty())?;
    let repo = parts.next().filter(|s| !s.is_empty())?;
    Some(format!("{owner}/{repo}"))
}

pub fn origin_repo(project: &Path) -> Result<String, RpcError> {
    let out = Command::new("git").arg("-C").arg(project).args(["remote", "get-url", "origin"]).output();
    let url = out.ok().filter(|o| o.status.success()).map(|o| String::from_utf8_lossy(&o.stdout).into_owned());
    url.as_deref()
        .and_then(github_repo_from_url)
        .ok_or_else(|| RpcError::new(ErrorKind::NotFound, "project has no GitHub repository"))
}

fn parse<T: serde::de::DeserializeOwned>(out: &str) -> Result<T, RpcError> {
    serde_json::from_str(out).map_err(|e| RpcError::new(ErrorKind::Internal, format!("unexpected gh output: {e}")))
}

pub fn search_issues(gh: &Path, project: &Path, query: &str, assigned_to_me: bool) -> Result<Vec<IssueHit>, RpcError> {
    let repo = origin_repo(project)?;
    let search = format!("{} sort:updated-desc", query.trim()).trim().to_string();
    let mut args = vec!["issue", "list", "-R", &repo, "--state", "open", "--limit", ISSUE_LIMIT, "--json", "number,title,url,state,assignees,updatedAt", "--search", &search];
    if assigned_to_me {
        args.extend(["--assignee", "@me"]);
    }
    let items: Vec<Value> = parse(&run_with_timeout(gh, &args, ISSUE_TIMEOUT, Some(project))?)?;
    Ok(items
        .iter()
        .filter_map(|item| {
            Some(IssueHit {
                key: format!("#{}", item["number"].as_u64()?),
                title: item["title"].as_str()?.to_string(),
                url: item["url"].as_str()?.to_string(),
                state: item["state"].as_str().unwrap_or("open").to_lowercase(),
                assignee: item["assignees"][0]["login"].as_str().map(String::from),
                updated_at: item["updatedAt"].as_str().map(String::from),
            })
        })
        .collect())
}

pub fn get_issue(gh: &Path, project: &Path, key: &str) -> Result<Issue, RpcError> {
    let number = key.trim_start_matches('#');
    if number.is_empty() || !number.bytes().all(|b| b.is_ascii_digit()) {
        return Err(RpcError::new(ErrorKind::InvalidParams, format!("{key:?} is not a GitHub issue number")));
    }
    let repo = origin_repo(project)?;
    let item: Value = parse(&run_with_timeout(gh, &["issue", "view", number, "-R", &repo, "--json", "number,title,url,body"], ISSUE_TIMEOUT, Some(project))?)?;
    let field = |name: &str| item[name].as_str().map(String::from).ok_or_else(|| RpcError::new(ErrorKind::Internal, format!("gh issue view has no {name}")));
    Ok(Issue { key: format!("#{number}"), title: field("title")?, url: field("url")?, description: item["body"].as_str().unwrap_or_default().to_string(), branch: None })
}

// ponytail: only the 200 most recent PRs are searched; query per branch if old task branches go missing.
const PR_LIMIT: &str = "200";
const PR_FIELDS: &str = "headRefName,number,url,title,state,isDraft,reviewDecision,statusCheckRollup,updatedAt,isCrossRepository";
const FAILED_RUNS: &[&str] = &["FAILURE", "TIMED_OUT", "CANCELLED", "ACTION_REQUIRED", "STARTUP_FAILURE"];

pub fn checks_of(rollup: &Value) -> PrChecks {
    let mut failing = Vec::new();
    let mut pending = false;
    let items = rollup.as_array().map(Vec::as_slice).unwrap_or_default();
    for item in items {
        if item["__typename"] == "StatusContext" {
            match item["state"].as_str().unwrap_or_default() {
                "FAILURE" | "ERROR" => failing.push(item["context"].as_str().unwrap_or_default().to_string()),
                "PENDING" | "EXPECTED" => pending = true,
                _ => {}
            }
        } else if item["status"].as_str() != Some("COMPLETED") {
            pending = true;
        } else if FAILED_RUNS.contains(&item["conclusion"].as_str().unwrap_or_default()) {
            failing.push(item["name"].as_str().unwrap_or_default().to_string());
        }
    }
    let state = if !failing.is_empty() {
        ChecksState::Failure
    } else if pending {
        ChecksState::Pending
    } else if items.is_empty() {
        ChecksState::None
    } else {
        ChecksState::Success
    };
    PrChecks { state, failing }
}

fn pr_of(item: &Value) -> Option<PullRequest> {
    let state = match (item["state"].as_str()?, item["isDraft"].as_bool().unwrap_or(false)) {
        ("OPEN", true) => PrState::Draft,
        ("OPEN", false) => PrState::Open,
        ("MERGED", _) => PrState::Merged,
        _ => PrState::Closed,
    };
    let review = match item["reviewDecision"].as_str().unwrap_or_default() {
        "APPROVED" => ReviewState::Approved,
        "CHANGES_REQUESTED" => ReviewState::ChangesRequested,
        "REVIEW_REQUIRED" => ReviewState::ReviewRequired,
        _ => ReviewState::None,
    };
    Some(PullRequest {
        number: item["number"].as_u64()?,
        url: item["url"].as_str()?.to_string(),
        title: item["title"].as_str().unwrap_or_default().to_string(),
        state,
        review,
        checks: checks_of(&item["statusCheckRollup"]),
    })
}

pub fn pick_prs(items: &[Value], branches: &[String]) -> Vec<BranchPr> {
    branches
        .iter()
        .filter_map(|branch| {
            let best = items
                .iter()
                .filter(|i| i["headRefName"].as_str() == Some(branch.as_str()) && i["isCrossRepository"] != true)
                .max_by_key(|i| (i["state"] == "OPEN", i["updatedAt"].as_str().unwrap_or_default().to_string()))?;
            Some(BranchPr { branch: branch.clone(), pr: pr_of(best)? })
        })
        .collect()
}

pub fn pull_requests(gh: &Path, project: &Path, branches: &[String]) -> Result<Vec<BranchPr>, RpcError> {
    let repo = origin_repo(project)?;
    let args = ["pr", "list", "-R", &repo, "--state", "all", "--limit", PR_LIMIT, "--json", PR_FIELDS];
    let items: Vec<Value> = parse(&run_with_timeout(gh, &args, ISSUE_TIMEOUT, Some(project))?)?;
    Ok(pick_prs(&items, branches))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_missing_gh_says_it_is_not_installed() {
        let err = run_with_timeout(Path::new("/nonexistent/gh"), &["--version"], Duration::from_secs(1), None).unwrap_err();
        assert_eq!(err.message, "the GitHub CLI (gh) is not installed");
    }
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// A fake gh that logs its arguments and answers the calls these tests make.
    fn fake_gh(dir: &Path, logged_in: bool, create_fails: bool) -> PathBuf {
        let log = dir.join("gh.log");
        let script = format!(
            r#"#!/bin/sh
echo "$@" >> "{log}"
case "$1 $2" in
  "--version "*) echo "gh version 9.9.9"; exit 0 ;;
  "api user") {user} ;;
  "api user/orgs") echo acme; echo tools; exit 0 ;;
  "repo list") echo '[{{"nameWithOwner":"acme/api","description":"API","isPrivate":true}}]'; exit 0 ;;
  "repo create") {create} ;;
esac
exit 0
"#,
            log = log.display(),
            user = if logged_in { "echo me; exit 0" } else { "echo 'not logged in' >&2; exit 1" },
            create = if create_fails { "echo 'name already exists' >&2; exit 1" } else { "exit 0" },
        );
        let bin = dir.join("gh");
        std::fs::write(&bin, script).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    fn status_reports_account_and_owners() {
        let dir = tempfile::tempdir().unwrap();
        let ok = status(&fake_gh(dir.path(), true, false));
        assert!(ok.available && ok.authenticated);
        assert_eq!((ok.account.as_deref(), ok.owners.as_slice()), (Some("me"), &["acme".to_string(), "tools".to_string()][..]));
        let logged_out = status(&fake_gh(dir.path(), false, false));
        assert!(logged_out.available && !logged_out.authenticated);
        assert!(logged_out.error.unwrap().contains("not logged in"));
        assert!(!status(Path::new("/definitely/not/gh")).available);
    }

    #[test]
    fn slow_gh_is_killed_on_timeout() {
        let dir = tempfile::tempdir().unwrap();
        let bin = dir.path().join("gh");
        std::fs::write(&bin, "#!/bin/sh\nexec sleep 30\n").unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        let started = Instant::now();
        let err = run_with_timeout(&bin, &["api", "user"], Duration::from_millis(200), None).unwrap_err();
        assert!(err.message.starts_with("gh timed out after"), "{}", err.message);
        assert!(started.elapsed() < Duration::from_secs(5));
    }

    #[test]
    fn repos_are_split_into_owner_and_name() {
        let dir = tempfile::tempdir().unwrap();
        let repos = repos(&fake_gh(dir.path(), true, false), "acme").unwrap();
        assert_eq!(repos, [ForgeRepo { owner: "acme".into(), name: "api".into(), description: Some("API".into()), private: true }]);
    }

    #[test]
    fn create_passes_visibility_source_and_push() {
        let dir = tempfile::tempdir().unwrap();
        create(&fake_gh(dir.path(), true, false), "acme", "demo", Visibility::Private, dir.path(), &[]).unwrap();
        let log = std::fs::read_to_string(dir.path().join("gh.log")).unwrap();
        let line = log.lines().find(|l| l.starts_with("repo create")).unwrap();
        assert!(line.contains("acme/demo") && line.contains("--private") && line.contains("--remote origin") && line.contains("--push"), "{line}");
        let err = create(&fake_gh(dir.path(), true, true), "acme", "demo", Visibility::Private, dir.path(), &[]).unwrap_err();
        assert!(err.message.contains("already exists"), "{}", err.message);
    }

    #[test]
    fn owners_resolve_case_insensitively_to_their_canonical_spelling() {
        let status = ForgeStatus { available: true, authenticated: true, account: Some("Me".into()), owners: vec!["Acme".into()], error: None };
        assert_eq!(resolve_owner(&status, "acme", Visibility::Internal).unwrap(), "Acme");
        assert_eq!(resolve_owner(&status, "ME", Visibility::Private).unwrap(), "Me");
        assert!(resolve_owner(&status, "me", Visibility::Internal).unwrap_err().message.contains("organization owner"));
        assert!(resolve_owner(&status, "evil", Visibility::Public).unwrap_err().message.contains("not you or one of your organizations"));
        let logged_out = ForgeStatus { available: true, ..Default::default() };
        assert!(resolve_owner(&logged_out, "acme", Visibility::Public).unwrap_err().message.contains("gh auth login"));
    }

    #[test]
    fn github_repos_are_read_from_remote_urls() {
        assert_eq!(github_repo_from_url("git@github.com:acme/api.git").as_deref(), Some("acme/api"));
        assert_eq!(github_repo_from_url("https://github.com/acme/api").as_deref(), Some("acme/api"));
        assert_eq!(github_repo_from_url("ssh://git@github.com/acme/api.git").as_deref(), Some("acme/api"));
        assert_eq!(github_repo_from_url("https://gitlab.com/acme/api.git"), None);
        assert_eq!(github_repo_from_url("https://github.com/acme"), None);
    }

    fn repo_with_origin(dir: &Path, origin: Option<&str>) -> PathBuf {
        let repo = dir.join("repo");
        std::fs::create_dir_all(&repo).unwrap();
        Command::new("git").args(["init", "-q"]).current_dir(&repo).status().unwrap();
        if let Some(url) = origin {
            Command::new("git").args(["remote", "add", "origin", url]).current_dir(&repo).status().unwrap();
        }
        repo
    }

    #[test]
    fn projects_without_a_github_origin_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let none = repo_with_origin(&dir.path().join("a"), None);
        assert!(origin_repo(&none).unwrap_err().message.contains("no GitHub repository"));
        let gitlab = repo_with_origin(&dir.path().join("b"), Some("https://gitlab.com/acme/api.git"));
        assert!(origin_repo(&gitlab).is_err());
        let github = repo_with_origin(&dir.path().join("c"), Some("git@github.com:acme/api.git"));
        assert_eq!(origin_repo(&github).unwrap(), "acme/api");
    }

    fn fake_issue_gh(dir: &Path) -> PathBuf {
        let log = dir.join("gh.log");
        let script = format!(
            r#"#!/bin/sh
echo "$@" >> "{log}"
case "$1 $2" in
  "issue list") echo '[{{"number":42,"title":"Fix login","url":"https://github.com/acme/api/issues/42","state":"OPEN","assignees":[{{"login":"me"}}],"updatedAt":"2026-10-01T00:00:00Z"}},{{"number":7,"title":"Old","url":"u7","state":"OPEN","assignees":[],"updatedAt":"2026-09-01T00:00:00Z"}}]' ;;
  "issue view") echo '{{"number":42,"title":"Fix login","url":"https://github.com/acme/api/issues/42","body":"Body text"}}' ;;
esac
exit 0
"#,
            log = log.display()
        );
        let bin = dir.join("gh");
        std::fs::write(&bin, script).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    #[test]
    fn issues_are_searched_and_fetched_through_gh() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake_issue_gh(dir.path());
        let repo = repo_with_origin(dir.path(), Some("https://github.com/acme/api.git"));
        let hits = search_issues(&gh, &repo, "login", true).unwrap();
        assert_eq!(hits[0].key, "#42");
        assert_eq!((hits[0].state.as_str(), hits[0].assignee.as_deref()), ("open", Some("me")));
        assert_eq!(hits[1].assignee, None);
        let issue = get_issue(&gh, &repo, "#42").unwrap();
        assert_eq!((issue.key.as_str(), issue.description.as_str(), issue.branch.clone()), ("#42", "Body text", None));
        let log = std::fs::read_to_string(dir.path().join("gh.log")).unwrap();
        assert!(log.contains("issue list -R acme/api --state open --limit 50"), "{log}");
        assert!(log.contains("--assignee @me") && log.contains("login sort:updated-desc"), "{log}");
        assert!(log.contains("issue view 42 -R acme/api"), "{log}");
        assert_eq!(get_issue(&gh, &repo, "#x").unwrap_err().kind(), ErrorKind::InvalidParams);
    }

    fn item(branch: &str, number: u64, state: &str, draft: bool, updated: &str, rollup: Value, review: Value) -> Value {
        serde_json::json!({"headRefName": branch, "number": number, "url": format!("https://github.com/acme/api/pull/{number}"),
            "title": format!("PR {number}"), "state": state, "isDraft": draft, "reviewDecision": review,
            "statusCheckRollup": rollup, "updatedAt": updated})
    }

    #[test]
    fn checks_combine_check_runs_and_status_contexts() {
        use serde_json::json;
        assert_eq!(checks_of(&Value::Null).state, ChecksState::None);
        assert_eq!(checks_of(&json!([])).state, ChecksState::None);
        let ok = json!([{"__typename": "CheckRun", "name": "test", "status": "COMPLETED", "conclusion": "SUCCESS"},
                        {"__typename": "CheckRun", "name": "docs", "status": "COMPLETED", "conclusion": "SKIPPED"},
                        {"__typename": "StatusContext", "context": "ci/legacy", "state": "SUCCESS"}]);
        assert_eq!(checks_of(&ok), PrChecks { state: ChecksState::Success, failing: vec![] });
        let pending = json!([{"__typename": "CheckRun", "name": "test", "status": "IN_PROGRESS", "conclusion": null},
                             {"__typename": "StatusContext", "context": "ci/legacy", "state": "PENDING"}]);
        assert_eq!(checks_of(&pending).state, ChecksState::Pending);
        let failed = json!([{"__typename": "CheckRun", "name": "lint", "status": "COMPLETED", "conclusion": "FAILURE"},
                            {"__typename": "CheckRun", "name": "test", "status": "QUEUED", "conclusion": null},
                            {"__typename": "StatusContext", "context": "deploy", "state": "ERROR"}]);
        assert_eq!(checks_of(&failed), PrChecks { state: ChecksState::Failure, failing: vec!["lint".into(), "deploy".into()] });
    }

    #[test]
    fn the_open_pr_wins_else_the_newest() {
        use serde_json::json;
        let items = vec![
            item("a", 1, "CLOSED", false, "2026-10-05T00:00:00Z", Value::Null, Value::Null),
            item("a", 2, "OPEN", true, "2026-10-01T00:00:00Z", Value::Null, json!("REVIEW_REQUIRED")),
            item("b", 3, "MERGED", false, "2026-09-01T00:00:00Z", Value::Null, json!("APPROVED")),
            item("b", 4, "CLOSED", false, "2026-10-02T00:00:00Z", Value::Null, json!("CHANGES_REQUESTED")),
            item("c", 5, "OPEN", false, "2026-10-02T00:00:00Z", Value::Null, Value::Null),
            {
                let mut fork = item("a", 6, "OPEN", false, "2026-10-06T00:00:00Z", Value::Null, Value::Null);
                fork["isCrossRepository"] = json!(true);
                fork
            },
        ];
        let picked = pick_prs(&items, &["a".into(), "b".into(), "d".into()]);
        let summary: Vec<_> = picked.iter().map(|b| (b.branch.as_str(), b.pr.number, b.pr.state, b.pr.review)).collect();
        assert_eq!(summary, vec![("a", 2, PrState::Draft, ReviewState::ReviewRequired), ("b", 4, PrState::Closed, ReviewState::ChangesRequested)]);
    }

    #[test]
    fn pull_requests_are_listed_once_per_repository() {
        let dir = tempfile::tempdir().unwrap();
        let log = dir.path().join("gh.log");
        let script = format!(
            "#!/bin/sh\necho \"$@\" >> \"{}\"\necho '[{{\"headRefName\":\"a\",\"number\":9,\"url\":\"u\",\"title\":\"t\",\"state\":\"OPEN\",\"isDraft\":false,\"reviewDecision\":\"\",\"statusCheckRollup\":[],\"updatedAt\":\"2026-10-01T00:00:00Z\"}}]'\n",
            log.display()
        );
        let gh = dir.path().join("gh");
        std::fs::write(&gh, script).unwrap();
        std::fs::set_permissions(&gh, std::fs::Permissions::from_mode(0o755)).unwrap();
        let repo = repo_with_origin(dir.path(), Some("https://github.com/acme/api.git"));
        let prs = pull_requests(&gh, &repo, &["a".into()]).unwrap();
        assert_eq!((prs[0].pr.number, prs[0].pr.review, prs[0].pr.checks.state), (9, ReviewState::None, ChecksState::None));
        let logged = std::fs::read_to_string(log).unwrap();
        assert!(logged.contains("pr list -R acme/api --state all --limit 200 --json headRefName,number,url,title,state,isDraft,reviewDecision,statusCheckRollup,updatedAt,isCrossRepository"), "{logged}");
    }
}
