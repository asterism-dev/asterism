use std::path::PathBuf;

use asterism_proto::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{NodeConfig, NodeConfigInfo, PathSettings};

use crate::agent_settings::{write_atomic, SAVE_LOCK};
use crate::config::{Config, PathsConfig};
use crate::error::{Error, Result};
use crate::repo_source;

pub const LOCAL_OWNER: &str = "local";

fn invalid(message: String) -> Error {
    Error::new(ErrorKind::InvalidParams, message)
}

/// Expands a leading `~`/`~/` to the daemon user's home; the result must be absolute.
pub fn expand_home(value: &str) -> Result<PathBuf> {
    if value.is_empty() || value.contains('\0') {
        return Err(invalid(format!("invalid path {value:?}")));
    }
    let home = || std::env::var_os("HOME").map(PathBuf::from).ok_or_else(|| invalid("HOME is not set".into()));
    let path = match value.strip_prefix('~') {
        Some("") => home()?,
        Some(rest) if rest.starts_with('/') => home()?.join(&rest[1..]),
        Some(_) => return Err(invalid(format!("only ~/ paths are supported, got {value:?}"))),
        None => PathBuf::from(value),
    };
    if path.is_absolute() {
        Ok(path)
    } else {
        Err(invalid(format!("path must be absolute, got {value:?}")))
    }
}

pub fn defaults(paths: &Paths) -> PathSettings {
    PathSettings {
        repos: paths.home.join("repos").display().to_string(),
        worktrees: paths.worktrees().display().to_string(),
    }
}

pub fn load(paths: &Paths) -> Result<NodeConfigInfo> {
    let stored = Config::load(&paths.config())?.paths;
    let defaults = defaults(paths);
    let config = NodeConfig {
        paths: PathSettings {
            repos: stored.repos.unwrap_or_else(|| defaults.repos.clone()),
            worktrees: stored.worktrees.unwrap_or_else(|| defaults.worktrees.clone()),
        },
    };
    Ok(NodeConfigInfo { config, defaults })
}

pub fn validate(config: &NodeConfig) -> Result<()> {
    for (label, value) in [("repositories", &config.paths.repos), ("worktrees", &config.paths.worktrees)] {
        let path = expand_home(value).map_err(|e| invalid(format!("{label}: {}", e.message)))?;
        std::fs::create_dir_all(&path).map_err(|e| invalid(format!("{label}: cannot create {}: {e}", path.display())))?;
    }
    Ok(())
}

pub fn save(paths: &Paths, config: &NodeConfig) -> Result<()> {
    validate(config)?;
    let defaults = defaults(paths);
    let _guard = crate::lock(&SAVE_LOCK);
    let mut file = Config::load(&paths.config())?;
    let keep = |value: &String, default: &String| (value != default).then(|| value.clone());
    file.paths = PathsConfig {
        repos: keep(&config.paths.repos, &defaults.repos),
        worktrees: keep(&config.paths.worktrees, &defaults.worktrees),
    };
    let text = toml::to_string(&file).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
    write_atomic(&paths.config(), &text)
}

pub fn repos_dir(paths: &Paths) -> Result<PathBuf> {
    expand_home(&load(paths)?.config.paths.repos)
}

pub fn worktrees_dir(paths: &Paths) -> Result<PathBuf> {
    expand_home(&load(paths)?.config.paths.worktrees)
}

/// The `<owner>/<repo>` grouping for a project: its origin remote, else `local` and a sanitized name.
pub fn layout_owner_repo(origin: Option<&str>, project_name: &str) -> (String, String) {
    if let Some((owner, repo)) = origin.and_then(repo_source::owner_repo_from_url) {
        if repo_source::check_name("owner", &owner).is_ok() && repo_source::check_name("repository name", &repo).is_ok() {
            return (owner, repo);
        }
    }
    let sanitized: String = project_name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-') { c } else { '-' })
        .collect();
    let name = sanitized.trim_start_matches(['-', '.']);
    (LOCAL_OWNER.to_string(), if name.is_empty() { "project".to_string() } else { name.to_string() })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_paths() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths { home: dir.path().join("h") };
        paths.ensure_dirs().unwrap();
        (dir, paths)
    }

    #[test]
    fn defaults_live_under_asterism_home() {
        let (_dir, paths) = temp_paths();
        let info = load(&paths).unwrap();
        assert_eq!(info.config.paths, info.defaults);
        assert_eq!(repos_dir(&paths).unwrap(), paths.home.join("repos"));
        assert_eq!(worktrees_dir(&paths).unwrap(), paths.home.join("worktrees"));
    }

    #[test]
    fn save_roundtrips_and_keeps_agent_settings() {
        let (dir, paths) = temp_paths();
        std::fs::write(paths.config(), "[agents.shell.env]\nset = { FOO = \"1\" }\n").unwrap();
        let repos = dir.path().join("code").display().to_string();
        let config = NodeConfig { paths: PathSettings { repos: repos.clone(), worktrees: defaults(&paths).worktrees } };
        save(&paths, &config).unwrap();
        assert_eq!(load(&paths).unwrap().config, config);
        assert_eq!(repos_dir(&paths).unwrap(), dir.path().join("code"));
        let text = std::fs::read_to_string(paths.config()).unwrap();
        assert!(text.contains("FOO"), "{text}");
        assert!(!text.contains("worktrees"), "default values are not stored: {text}");
    }

    #[test]
    fn invalid_paths_change_nothing() {
        let (_dir, paths) = temp_paths();
        let before = std::fs::read_to_string(paths.config()).ok();
        for bad in ["relative/dir", "", "/tmp/a\0b", "~nobody/x"] {
            let config = NodeConfig { paths: PathSettings { repos: bad.into(), worktrees: defaults(&paths).worktrees } };
            assert_eq!(save(&paths, &config).unwrap_err().kind, ErrorKind::InvalidParams, "{bad:?}");
        }
        assert_eq!(std::fs::read_to_string(paths.config()).ok(), before);
    }

    #[test]
    fn tilde_expands_to_home() {
        let home = std::env::var("HOME").unwrap();
        assert_eq!(expand_home("~/x").unwrap(), std::path::Path::new(&home).join("x"));
        assert_eq!(expand_home("~").unwrap(), std::path::PathBuf::from(&home));
        assert_eq!(expand_home("/abs").unwrap(), std::path::PathBuf::from("/abs"));
    }

    #[test]
    fn layout_uses_origin_owner_or_local() {
        assert_eq!(layout_owner_repo(Some("git@github.com:acme/api.git"), "api"), ("acme".into(), "api".into()));
        assert_eq!(layout_owner_repo(None, "notes"), ("local".into(), "notes".into()));
        assert_eq!(layout_owner_repo(Some("/srv/bare.git"), "my repo"), ("local".into(), "my-repo".into()));
    }
}
