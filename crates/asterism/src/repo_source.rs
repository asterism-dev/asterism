use asterism_proto::rpc::ErrorKind;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSource {
    pub owner: String,
    pub repo: String,
    /// `None` for the GitHub `owner/repo` shorthand.
    pub url: Option<String>,
}

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidParams, message)
}

/// Names become path segments, so only a safe character set is allowed.
pub fn check_name(what: &str, name: &str) -> Result<()> {
    let safe = !name.is_empty()
        && name != "."
        && name != ".."
        && !name.starts_with('-')
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'));
    if safe {
        Ok(())
    } else {
        Err(invalid(format!("invalid {what} {name:?}: use letters, digits, '.', '_' or '-'")))
    }
}

/// The path part of a git URL: after the host for `scheme://`, after the first ':' for scp-style.
fn url_path(url: &str) -> Option<&str> {
    if let Some((_, rest)) = url.split_once("://") {
        rest.find('/').map(|i| &rest[i..])
    } else if let Some((host, path)) = url.split_once(':') {
        (!host.contains('/') && !host.is_empty()).then_some(path)
    } else {
        None
    }
}

pub fn owner_repo_from_url(url: &str) -> Option<(String, String)> {
    let path = url_path(url.trim())?;
    let path = path.trim_end_matches('/');
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut segments = path.rsplit('/').filter(|s| !s.is_empty());
    let repo = segments.next()?;
    let owner = segments.next()?;
    Some((owner.to_string(), repo.to_string()))
}

pub fn parse_source(source: &str) -> Result<RepoSource> {
    let source = source.trim();
    if source.contains("://") || source.contains(':') {
        let (owner, repo) =
            owner_repo_from_url(source).ok_or_else(|| invalid(format!("cannot read owner/repo from {source:?}")))?;
        check_name("owner", &owner)?;
        check_name("repository name", &repo)?;
        return Ok(RepoSource { owner, repo, url: Some(source.to_string()) });
    }
    let (owner, repo) = source
        .split_once('/')
        .ok_or_else(|| invalid(format!("expected owner/repo or a git URL, got {source:?}")))?;
    check_name("owner", owner)?;
    check_name("repository name", repo)?;
    Ok(RepoSource { owner: owner.to_string(), repo: repo.to_string(), url: None })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn src(owner: &str, repo: &str, url: Option<&str>) -> RepoSource {
        RepoSource { owner: owner.into(), repo: repo.into(), url: url.map(String::from) }
    }

    #[test]
    fn parses_shorthand_and_urls() {
        assert_eq!(parse_source(" acme/api ").unwrap(), src("acme", "api", None));
        assert_eq!(
            parse_source("https://github.com/acme/api.git").unwrap(),
            src("acme", "api", Some("https://github.com/acme/api.git"))
        );
        assert_eq!(
            parse_source("git@github.com:acme/api.git").unwrap(),
            src("acme", "api", Some("git@github.com:acme/api.git"))
        );
        assert_eq!(
            parse_source("ssh://git@gitlab.com/group/sub/tool").unwrap(),
            src("sub", "tool", Some("ssh://git@gitlab.com/group/sub/tool"))
        );
        assert_eq!(
            parse_source("file:///tmp/remotes/demo.git/").unwrap(),
            src("remotes", "demo", Some("file:///tmp/remotes/demo.git/"))
        );
    }

    #[test]
    fn rejects_traversal_and_bad_names() {
        for bad in ["", "acme", "acme/", "/abs/path", "acme/..", "../x", "a/b/c", "acme/-rf", "https://host/only", "acme/a b"] {
            assert!(parse_source(bad).is_err(), "{bad}");
        }
        assert!(check_name("repository name", "ok.name_1-x").is_ok());
        for bad in ["", ".", "..", "-x", "a/b", "a b", "ä"] {
            assert!(check_name("repository name", bad).is_err(), "{bad}");
        }
    }

    #[test]
    fn owner_and_repo_from_remote_urls() {
        assert_eq!(owner_repo_from_url("git@github.com:acme/api.git"), Some(("acme".into(), "api".into())));
        assert_eq!(owner_repo_from_url("https://github.com/acme/api"), Some(("acme".into(), "api".into())));
        assert_eq!(owner_repo_from_url("/local/bare.git"), None);
        assert_eq!(owner_repo_from_url("garbage"), None);
    }
}
