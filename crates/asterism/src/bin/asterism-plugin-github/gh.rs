use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use asterism_plugin::{ErrorKind, RpcError};
use asterism_proto::types::{ForgeRepo, ForgeStatus, Visibility};
use serde_json::Value;

const REPO_LIMIT: &str = "200";
const METADATA_TIMEOUT: Duration = Duration::from_secs(20);
// Stays under the daemon's 20 s call cap so status answers instead of timing out.
const STATUS_BUDGET: Duration = Duration::from_secs(15);
const MESSAGE_LIMIT: usize = 2_000;

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

fn run_with_timeout(gh: &Path, args: &[&str], timeout: Duration) -> Result<String, RpcError> {
    let mut child = Command::new(gh)
        .args(args)
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RpcError::new(ErrorKind::Git, e.to_string()))?;
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
    run_with_timeout(gh, args, METADATA_TIMEOUT)
}

pub fn status(gh: &Path) -> ForgeStatus {
    let deadline = Instant::now() + STATUS_BUDGET;
    let call = |args: &[&str]| run_with_timeout(gh, args, deadline.saturating_duration_since(Instant::now()));
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

#[cfg(test)]
mod tests {
    use super::*;
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
        let err = run_with_timeout(&bin, &["api", "user"], Duration::from_millis(200)).unwrap_err();
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
}
