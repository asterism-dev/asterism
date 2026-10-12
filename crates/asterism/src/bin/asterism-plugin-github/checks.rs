use std::path::Path;
use std::time::Instant;

use asterism_plugin::{ErrorKind, RpcError};
use asterism_proto::types::CheckLog;

use crate::gh::{origin_repo, run_with_timeout, ISSUE_TIMEOUT};

const LOG_LIMIT: usize = 1_000_000;

/// The last `limit` bytes of `text`, starting on a char boundary, and whether anything was cut.
pub fn tail(text: &str, limit: usize) -> (&str, bool) {
    if text.len() <= limit {
        return (text, false);
    }
    let mut start = text.len() - limit;
    while !text.is_char_boundary(start) {
        start += 1;
    }
    (&text[start..], true)
}

/// Only Actions jobs have logs and re-runs; their check id is the numeric job id.
fn job_id(check_id: &str) -> Result<&str, RpcError> {
    if !check_id.is_empty() && check_id.bytes().all(|b| b.is_ascii_digit()) {
        Ok(check_id)
    } else {
        Err(RpcError::new(
            ErrorKind::InvalidParams,
            format!("check {check_id} has no log"),
        ))
    }
}

/// The job's run id and web URL.
fn job(
    gh: &Path,
    project: &Path,
    repo: &str,
    id: &str,
    deadline: Instant,
) -> Result<(String, String), RpcError> {
    let path = format!("repos/{repo}/actions/jobs/{id}");
    let out = run_with_timeout(
        gh,
        &["api", &path, "--jq", ".run_id, .html_url"],
        deadline.saturating_duration_since(Instant::now()),
        Some(project),
    )?;
    let mut parts = out.split_whitespace();
    match (parts.next(), parts.next()) {
        (Some(run), Some(url)) => Ok((run.to_string(), url.to_string())),
        _ => Err(RpcError::new(
            ErrorKind::Internal,
            format!("unexpected gh output for job {id}"),
        )),
    }
}

pub fn log(gh: &Path, project: &Path, check_id: &str) -> Result<CheckLog, RpcError> {
    let id = job_id(check_id)?;
    let repo = origin_repo(project)?;
    let deadline = Instant::now() + ISSUE_TIMEOUT;
    let (_, url) = job(gh, project, &repo, id, deadline)?;
    let path = format!("repos/{repo}/actions/jobs/{id}/logs");
    let text = run_with_timeout(
        gh,
        &["api", &path],
        deadline.saturating_duration_since(Instant::now()),
        Some(project),
    )?;
    let (text, truncated) = tail(&text, LOG_LIMIT);
    Ok(CheckLog {
        text: text.to_string(),
        truncated,
        url,
    })
}

pub fn rerun(gh: &Path, project: &Path, check_id: &str) -> Result<(), RpcError> {
    let id = job_id(check_id)?;
    let repo = origin_repo(project)?;
    let deadline = Instant::now() + ISSUE_TIMEOUT;
    let (run, _) = job(gh, project, &repo, id, deadline)?;
    let path = format!("repos/{repo}/actions/runs/{run}/rerun-failed-jobs");
    run_with_timeout(
        gh,
        &["api", "-X", "POST", &path],
        deadline.saturating_duration_since(Instant::now()),
        Some(project),
    )
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    /// A fake gh that logs each call's arguments and answers job lookups and log downloads.
    fn fake_gh(dir: &Path) -> PathBuf {
        let log = dir.join("gh.log");
        let script = format!(
            r#"#!/bin/sh
echo "$@" >> "{log}"
case "$*" in
  *"/actions/jobs/4242/logs"*) printf 'line1\nline2\n' ;;
  *"/actions/jobs/4242 "*|*"/actions/jobs/4242") echo '9 https://github.com/acme/api/actions/runs/9/job/4242' ;;
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

    fn project(dir: &Path) -> PathBuf {
        let p = dir.join("repo");
        std::fs::create_dir_all(&p).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["remote", "add", "origin", "https://github.com/acme/api.git"],
        ] {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&p)
                .args(&args)
                .status()
                .unwrap()
                .success());
        }
        p
    }

    #[test]
    fn logs_are_fetched_per_job_with_the_job_url() {
        let dir = tempfile::tempdir().unwrap();
        let log = log(&fake_gh(dir.path()), &project(dir.path()), "4242").unwrap();
        assert_eq!(log.text, "line1\nline2\n");
        assert!(!log.truncated);
        assert_eq!(
            log.url,
            "https://github.com/acme/api/actions/runs/9/job/4242"
        );
    }

    #[test]
    fn rerun_restarts_the_failed_jobs_of_the_jobs_run() {
        let dir = tempfile::tempdir().unwrap();
        rerun(&fake_gh(dir.path()), &project(dir.path()), "4242").unwrap();
        let calls = std::fs::read_to_string(dir.path().join("gh.log")).unwrap();
        assert!(
            calls.contains("-X POST repos/acme/api/actions/runs/9/rerun-failed-jobs"),
            "{calls}"
        );
    }

    #[test]
    fn checks_without_a_job_id_are_rejected_before_calling_gh() {
        let dir = tempfile::tempdir().unwrap();
        let err = log(
            &fake_gh(dir.path()),
            &project(dir.path()),
            "status:ci/external",
        )
        .unwrap_err();
        assert!(err.message.contains("no log"), "{}", err.message);
        assert!(!dir.path().join("gh.log").exists());
    }

    #[test]
    fn tails_keep_the_end_on_a_char_boundary() {
        assert_eq!(tail("abcdef", 10), ("abcdef", false));
        assert_eq!(tail("abcdef", 3), ("def", true));
        // "é" is two bytes; cutting inside it moves the start forward.
        assert_eq!(tail("aéb", 2), ("b", true));
    }
}
