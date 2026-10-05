use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{GithubRepo, GithubStatus, GithubTarget, Visibility};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::git;

const REPO_LIMIT: &str = "200";
const METADATA_TIMEOUT: Duration = Duration::from_secs(20);

fn run(gh: &Path, args: &[&str], extra: &git::GitEnv) -> Result<String> {
    let out = Command::new(gh)
        .args(args)
        .envs(git::clone_env(extra).iter().map(|(k, v)| (k, v)))
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .output()?;
    if out.status.success() {
        Ok(String::from_utf8_lossy(&out.stdout).into_owned())
    } else {
        Err(Error::new(ErrorKind::Git, git::truncate(&String::from_utf8_lossy(&out.stderr))))
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

fn wait_with_timeout(child: &mut Child, timeout: Duration) -> Result<Option<std::process::ExitStatus>> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(Some(status));
        }
        if Instant::now() >= deadline {
            return Ok(None);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn run_with_timeout(gh: &Path, args: &[&str], timeout: Duration) -> Result<String> {
    let mut child = Command::new(gh)
        .args(args)
        .envs(git::clone_env(&[]).iter().map(|(k, v)| (k, v)))
        .env("GH_PROMPT_DISABLED", "1")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let stdout = read_pipe(child.stdout.take());
    let stderr = read_pipe(child.stderr.take());
    let Some(status) = wait_with_timeout(&mut child, timeout)? else {
        let _ = child.kill();
        let _ = child.wait();
        return Err(Error::new(ErrorKind::Git, format!("gh timed out after {}s", timeout.as_secs())));
    };
    let (stdout, stderr) = (stdout.join().unwrap_or_default(), stderr.join().unwrap_or_default());
    if status.success() {
        Ok(stdout)
    } else {
        Err(Error::new(ErrorKind::Git, git::truncate(&stderr)))
    }
}

fn metadata(gh: &Path, args: &[&str]) -> Result<String> {
    run_with_timeout(gh, args, METADATA_TIMEOUT)
}

pub fn status(gh: &Path) -> GithubStatus {
    if metadata(gh, &["--version"]).is_err() {
        return GithubStatus { error: Some("the GitHub CLI (gh) is not installed".into()), ..Default::default() };
    }
    match metadata(gh, &["api", "user", "--jq", ".login"]) {
        Ok(login) => GithubStatus {
            available: true,
            logged_in: true,
            login: Some(login.trim().to_string()),
            orgs: metadata(gh, &["api", "user/orgs", "--paginate", "--jq", ".[].login"])
                .map(|out| out.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
                .unwrap_or_default(),
            error: None,
        },
        Err(e) => GithubStatus { available: true, error: Some(e.message), ..Default::default() },
    }
}

pub fn logged_in(gh: &Path) -> bool {
    metadata(gh, &["api", "user"]).is_ok()
}

pub fn repos(gh: &Path, owner: &str) -> Result<Vec<GithubRepo>> {
    let out = metadata(gh, &["repo", "list", owner, "--json", "nameWithOwner,description,isPrivate", "--limit", REPO_LIMIT])?;
    let items: Vec<Value> =
        serde_json::from_str(&out).map_err(|e| Error::new(ErrorKind::Internal, format!("unexpected gh output: {e}")))?;
    Ok(items
        .iter()
        .filter_map(|item| {
            Some(GithubRepo {
                name_with_owner: item["nameWithOwner"].as_str()?.to_string(),
                description: item["description"].as_str().filter(|d| !d.is_empty()).map(String::from),
                private: item["isPrivate"].as_bool().unwrap_or(false),
            })
        })
        .collect())
}

pub fn clone(gh: &Path, owner: &str, repo: &str, target: &Path, extra: &git::GitEnv) -> Result<()> {
    let name = format!("{owner}/{repo}");
    run(gh, &["repo", "clone", &name, &target.to_string_lossy()], extra).map(|_| ())
}

pub fn create(gh: &Path, target: &GithubTarget, owner: &str, name: &str, dir: &Path, extra: &git::GitEnv) -> Result<()> {
    let full = format!("{owner}/{name}");
    let visibility = match target.visibility {
        Visibility::Public => "--public",
        Visibility::Private => "--private",
        Visibility::Internal => "--internal",
    };
    let source = dir.to_string_lossy();
    run(gh, &["repo", "create", &full, visibility, "--source", &source, "--remote", "origin", "--push"], extra).map(|_| ())
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
    fn status_reports_login_and_orgs() {
        let dir = tempfile::tempdir().unwrap();
        let status = status(&fake_gh(dir.path(), true, false));
        assert!(status.available && status.logged_in);
        assert_eq!(status.login.as_deref(), Some("me"));
        assert_eq!(status.orgs, ["acme", "tools"]);

        let logged_out = super::status(&fake_gh(dir.path(), false, false));
        assert!(logged_out.available && !logged_out.logged_in);
        assert!(logged_out.error.unwrap().contains("not logged in"));

        let missing = super::status(Path::new("/definitely/not/gh"));
        assert!(!missing.available && !missing.logged_in);
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
    fn repos_are_parsed_from_json() {
        let dir = tempfile::tempdir().unwrap();
        let repos = repos(&fake_gh(dir.path(), true, false), "acme").unwrap();
        assert_eq!(repos, [GithubRepo { name_with_owner: "acme/api".into(), description: Some("API".into()), private: true }]);
    }

    #[test]
    fn create_passes_visibility_source_and_push() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake_gh(dir.path(), true, false);
        let target = GithubTarget { owner: "acme".into(), visibility: Visibility::Private };
        create(&gh, &target, &target.owner, "demo", dir.path(), &[]).unwrap();
        let log = std::fs::read_to_string(dir.path().join("gh.log")).unwrap();
        let line = log.lines().find(|l| l.starts_with("repo create")).unwrap();
        assert!(line.contains("acme/demo") && line.contains("--private") && line.contains("--remote origin") && line.contains("--push"), "{line}");

        let failing = fake_gh(dir.path(), true, true);
        let err = create(&failing, &target, &target.owner, "demo", dir.path(), &[]).unwrap_err();
        assert!(err.message.contains("already exists"), "{}", err.message);
    }
}
