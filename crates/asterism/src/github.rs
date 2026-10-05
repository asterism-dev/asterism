use std::path::Path;
use std::process::{Command, Stdio};

use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{GithubRepo, GithubStatus, GithubTarget, Visibility};
use serde_json::Value;

use crate::error::{Error, Result};
use crate::git;

const REPO_LIMIT: &str = "200";

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

pub fn status(gh: &Path) -> GithubStatus {
    if run(gh, &["--version"], &[]).is_err() {
        return GithubStatus { error: Some("the GitHub CLI (gh) is not installed".into()), ..Default::default() };
    }
    match run(gh, &["api", "user", "--jq", ".login"], &[]) {
        Ok(login) => GithubStatus {
            available: true,
            logged_in: true,
            login: Some(login.trim().to_string()),
            orgs: run(gh, &["api", "user/orgs", "--jq", ".[].login"], &[])
                .map(|out| out.lines().map(str::trim).filter(|l| !l.is_empty()).map(String::from).collect())
                .unwrap_or_default(),
            error: None,
        },
        Err(e) => GithubStatus { available: true, error: Some(e.message), ..Default::default() },
    }
}

pub fn repos(gh: &Path, owner: &str) -> Result<Vec<GithubRepo>> {
    let out = run(gh, &["repo", "list", owner, "--json", "nameWithOwner,description,isPrivate", "--limit", REPO_LIMIT], &[])?;
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

pub fn create(gh: &Path, target: &GithubTarget, name: &str, dir: &Path, extra: &git::GitEnv) -> Result<()> {
    let full = format!("{}/{name}", target.owner);
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
        create(&gh, &target, "demo", dir.path(), &[]).unwrap();
        let log = std::fs::read_to_string(dir.path().join("gh.log")).unwrap();
        let line = log.lines().find(|l| l.starts_with("repo create")).unwrap();
        assert!(line.contains("acme/demo") && line.contains("--private") && line.contains("--remote origin") && line.contains("--push"), "{line}");

        let failing = fake_gh(dir.path(), true, true);
        let err = create(&failing, &target, "demo", dir.path(), &[]).unwrap_err();
        assert!(err.message.contains("already exists"), "{}", err.message);
    }
}
