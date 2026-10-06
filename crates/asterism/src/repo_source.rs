use asterism_proto::rpc::ErrorKind;

use crate::error::{Error, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoSource {
    pub owner: String,
    pub repo: String,
    /// `None` for the `owner/repo` shorthand, which is cloned through the default forge.
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

// ponytail: GitHub web-URL normalization stays here; move it into forge plugins once another forge needs URL rules.
fn is_github(url: &str) -> bool {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', ':']).next().unwrap_or("");
    authority.rsplit('@').next() == Some("github.com")
}

pub fn owner_repo_from_url(url: &str) -> Option<(String, String)> {
    let url = url.trim();
    let path = url_path(url)?;
    let path = path.trim_end_matches('/');
    if is_github(url) {
        // GitHub web URLs like /acme/api/tree/main: owner and repo are the first two segments.
        let mut segments = path.split('/').filter(|s| !s.is_empty());
        let owner = segments.next()?;
        let repo = segments.next()?;
        return Some((owner.to_string(), repo.strip_suffix(".git").unwrap_or(repo).to_string()));
    }
    let path = path.strip_suffix(".git").unwrap_or(path);
    let mut segments = path.rsplit('/').filter(|s| !s.is_empty());
    let repo = segments.next()?;
    let owner = segments.next()?;
    Some((owner.to_string(), repo.to_string()))
}

pub fn parse_source(source: &str) -> Result<RepoSource> {
    let source = source.trim();
    if source.starts_with('-') || source.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(invalid(format!("invalid source {source:?}")));
    }
    if source.contains("://") || source.contains(':') {
        let (owner, repo) =
            owner_repo_from_url(source).ok_or_else(|| invalid(format!("cannot read owner/repo from {source:?}")))?;
        check_name("owner", &owner)?;
        check_name("repository name", &repo)?;
        let url = if is_github(source) && url_path(source).is_some_and(|p| p.trim_matches('/').split('/').count() > 2) {
            format!("https://github.com/{owner}/{repo}.git")
        } else {
            source.to_string()
        };
        return Ok(RepoSource { owner, repo, url: Some(url) });
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
            parse_source("https://github.com/acme/api/tree/main").unwrap(),
            src("acme", "api", Some("https://github.com/acme/api.git"))
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
        for bad in ["", "acme", "acme/", "/abs/path", "acme/..", "../x", "a/b/c", "acme/-rf", "https://host/only", "acme/a b",
            "--upload-pack=evil:a/b", "-x:a/b", "git@host:a/b c", "https://github.com/a\tx/b"] {
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
        assert_eq!(owner_repo_from_url("https://github.com/acme/api/tree/main"), Some(("acme".into(), "api".into())));
        assert_eq!(owner_repo_from_url("ssh://git@github.com/acme/api.git"), Some(("acme".into(), "api".into())));
        assert_eq!(owner_repo_from_url("ssh://git@gitlab.com/group/sub/tool"), Some(("sub".into(), "tool".into())));
        assert_eq!(owner_repo_from_url("/local/bare.git"), None);
        assert_eq!(owner_repo_from_url("garbage"), None);
    }
}
