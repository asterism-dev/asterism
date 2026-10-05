use std::collections::BTreeMap;
use std::io;
use std::path::Path;

use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::SessionKind;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};

/// Inherited variables no session receives unless an agent's `set` adds them back.
pub const DEFAULT_REMOVE: &[&str] = &["CLAUDE*", "ANTHROPIC_*"];

#[derive(Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default, skip_serializing_if = "PathsConfig::is_empty")]
    pub paths: PathsConfig,
    #[serde(default)]
    pub agents: BTreeMap<String, AgentConfig>,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct PathsConfig {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repos: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub worktrees: Option<String>,
}

impl PathsConfig {
    pub fn is_empty(&self) -> bool {
        self.repos.is_none() && self.worktrees.is_none()
    }
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct AgentConfig {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub args: Vec<String>,
    #[serde(default)]
    pub env: EnvPolicy,
}

#[derive(Debug, Default, Clone, Deserialize, Serialize, PartialEq, Eq)]
#[serde(deny_unknown_fields)]
pub struct EnvPolicy {
    /// Exact names, or prefixes ending in `*`, removed in addition to `DEFAULT_REMOVE`.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub remove: Vec<String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub set: BTreeMap<String, String>,
}

impl Config {
    /// Reads the node config; a missing file means no customisation.
    pub fn load(path: &Path) -> Result<Self> {
        match std::fs::read_to_string(path) {
            Ok(text) => toml::from_str(&text)
                .map_err(|e| Error::new(ErrorKind::InvalidParams, format!("{}: {e}", path.display()))),
            Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(Self::default()),
            Err(e) => Err(e.into()),
        }
    }

    pub fn env_policy(&self, agent: &str) -> EnvPolicy {
        self.agents.get(agent).map(|a| a.env.clone()).unwrap_or_default()
    }
}

pub fn agent_key(kind: &SessionKind) -> &str {
    match kind {
        SessionKind::Agent { name } => name,
        SessionKind::Shell => "shell",
        SessionKind::Command { .. } => "command",
    }
}

pub fn pattern_matches(pattern: &str, name: &str) -> bool {
    match pattern.strip_suffix('*') {
        Some(prefix) => name.starts_with(prefix),
        None => name == pattern,
    }
}

pub fn removed_by_default(name: &str) -> bool {
    DEFAULT_REMOVE.iter().any(|p| pattern_matches(p, name))
}

/// A session's complete environment: inherited minus removals, then `set`, then `fixed`; later entries win.
pub fn session_env(
    inherited: impl IntoIterator<Item = (String, String)>,
    policy: &EnvPolicy,
    fixed: &[(String, String)],
) -> Vec<(String, String)> {
    let removed = |name: &str| removed_by_default(name) || policy.remove.iter().any(|p| pattern_matches(p, name));
    let mut env: BTreeMap<String, String> = inherited.into_iter().filter(|(name, _)| !removed(name)).collect();
    env.extend(policy.set.clone());
    env.extend(fixed.iter().cloned());
    env.into_iter().collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn vars(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
        pairs.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect()
    }

    fn policy(toml_text: &str, agent: &str) -> EnvPolicy {
        toml::from_str::<Config>(toml_text).unwrap().env_policy(agent)
    }

    #[test]
    fn patterns_match_exact_names_or_prefixes() {
        assert!(pattern_matches("CLAUDE*", "CLAUDECODE"));
        assert!(pattern_matches("CLAUDE*", "CLAUDE_CODE_USE_BEDROCK"));
        assert!(pattern_matches("AWS_PROFILE", "AWS_PROFILE"));
        assert!(!pattern_matches("AWS_PROFILE", "AWS_PROFILE_X"));
        assert!(!pattern_matches("ANTHROPIC_*", "ANTHROPIC"));
    }

    #[test]
    fn claude_and_anthropic_variables_are_removed_by_default() {
        let inherited = vars(&[
            ("PATH", "/bin"),
            ("CLAUDECODE", "1"),
            ("CLAUDE_PID", "42"),
            ("CLAUDE_CODE_USE_BEDROCK", "1"),
            ("ANTHROPIC_API_KEY", "secret"),
        ]);
        let env = session_env(inherited, &EnvPolicy::default(), &[]);
        assert_eq!(env, vars(&[("PATH", "/bin")]));
    }

    #[test]
    fn agent_policy_removes_and_sets_with_asterism_variables_winning() {
        let policy = policy(
            r#"
            [agents.claude.env]
            remove = ["AWS_*"]
            set = { CLAUDE_CODE_USE_BEDROCK = "1", ASTERISM_TASK = "spoofed" }
            "#,
            "claude",
        );
        let inherited = vars(&[("PATH", "/bin"), ("AWS_PROFILE", "dev"), ("CLAUDECODE", "1")]);
        let fixed = vars(&[("ASTERISM_TASK", "7")]);
        let env = session_env(inherited, &policy, &fixed);
        assert_eq!(env, vars(&[("ASTERISM_TASK", "7"), ("CLAUDE_CODE_USE_BEDROCK", "1"), ("PATH", "/bin")]));
    }

    #[test]
    fn policies_are_per_agent() {
        let text = "[agents.shell.env]\nset = { FOO = \"bar\" }\n";
        assert_eq!(policy(text, "shell").set.get("FOO").map(String::as_str), Some("bar"));
        assert_eq!(policy(text, "claude"), EnvPolicy::default());
    }

    #[test]
    fn agent_keys_name_shells_and_commands() {
        assert_eq!(agent_key(&SessionKind::Agent { name: "claude".into() }), "claude");
        assert_eq!(agent_key(&SessionKind::Shell), "shell");
        assert_eq!(agent_key(&SessionKind::Command { argv: vec!["ls".into()] }), "command");
    }

    #[test]
    fn missing_file_is_an_empty_config_and_broken_files_name_the_path() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        assert_eq!(Config::load(&path).unwrap(), Config::default());

        std::fs::write(&path, "[agents.claude.envv]\nset = {}\n").unwrap();
        let err = Config::load(&path).unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidParams);
        assert!(err.message.contains("config.toml"), "{}", err.message);
    }
}
