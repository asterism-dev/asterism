use std::io;
use std::path::Path;

use asterism_proto::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_proto::types::{AgentConfig, AgentConfigRaw, EnvSettings, SessionKind};
use serde_json::Value;

use crate::agents::{self, AgentProfile, Launch};
use crate::config::{Config, EnvPolicy};
use crate::error::{Error, Result};

/// Session kinds that only take environment settings.
pub const BASE_AGENTS: &[&str] = &["shell", "command"];

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidParams, message)
}

/// Only known agent keys may name settings files, which also rules out path traversal.
pub fn check_agent(agent: &str) -> Result<()> {
    if BASE_AGENTS.contains(&agent) || agents::profile(agent).is_some() {
        Ok(())
    } else {
        Err(invalid(format!("unknown agent {agent:?}")))
    }
}

fn read_text(path: &Path) -> Result<Option<String>> {
    match std::fs::read_to_string(path) {
        Ok(text) => Ok(Some(text)),
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(e.into()),
    }
}

fn read_json(path: &Path) -> Result<Option<Value>> {
    match read_text(path)? {
        Some(text) => serde_json::from_str(&text).map(Some).map_err(|e| invalid(format!("{}: {e}", path.display()))),
        None => Ok(None),
    }
}

pub fn load(paths: &Paths, agent: &str) -> Result<AgentConfig> {
    check_agent(agent)?;
    let entry = Config::load(&paths.config())?.agents.remove(agent).unwrap_or_default();
    Ok(AgentConfig {
        args: entry.args,
        env: EnvSettings { remove: entry.env.remove, set: entry.env.set },
        mcp: read_json(&paths.agent_mcp(agent))?,
        hooks: read_json(&paths.agent_hooks(agent))?,
    })
}

pub fn load_raw(paths: &Paths, agent: &str) -> Result<AgentConfigRaw> {
    check_agent(agent)?;
    Ok(AgentConfigRaw {
        mcp_text: read_text(&paths.agent_mcp(agent))?,
        hooks_text: read_text(&paths.agent_hooks(agent))?,
    })
}

pub fn validate(agent: &str, config: &AgentConfig) -> Result<()> {
    check_agent(agent)?;
    if BASE_AGENTS.contains(&agent) && (!config.args.is_empty() || config.mcp.is_some() || config.hooks.is_some()) {
        return Err(invalid(format!("{agent} sessions only support environment settings")));
    }
    if config.args.iter().any(|arg| arg.contains('\0')) {
        return Err(invalid("parameters must not contain NUL characters"));
    }
    if let Some(name) = config.env.set.keys().find(|name| name.is_empty() || name.contains(['=', '\0'])) {
        return Err(invalid(format!("invalid environment variable name {name:?}")));
    }
    if config.env.remove.iter().any(|pattern| pattern.is_empty() || pattern.contains('\0')) {
        return Err(invalid("remove patterns must not be empty"));
    }
    if let Some(mcp) = &config.mcp {
        if !mcp.get("mcpServers").is_some_and(Value::is_object) {
            return Err(invalid("MCP servers must be an object with an \"mcpServers\" object"));
        }
    }
    if let Some(hooks) = &config.hooks {
        validate_hooks(hooks)?;
    }
    Ok(())
}

fn validate_hooks(hooks: &Value) -> Result<()> {
    let events = hooks
        .get("hooks")
        .and_then(Value::as_object)
        .ok_or_else(|| invalid("hooks must be an object with a \"hooks\" object"))?;
    for (event, entries) in events {
        let entries = entries.as_array().ok_or_else(|| invalid(format!("hooks.{event} must be a list")))?;
        if entries.iter().any(|entry| !entry.get("hooks").is_some_and(Value::is_array)) {
            return Err(invalid(format!("each hooks.{event} entry needs a \"hooks\" list")));
        }
    }
    Ok(())
}

pub fn save(paths: &Paths, agent: &str, config: &AgentConfig) -> Result<()> {
    validate(agent, config)?;
    let mut file = Config::load(&paths.config())?;
    let entry = file.agents.entry(agent.to_string()).or_default();
    entry.args = config.args.clone();
    entry.env = EnvPolicy { remove: config.env.remove.clone(), set: config.env.set.clone() };
    // ponytail: rewriting config.toml drops hand-written comments; switch to toml_edit if people hand-edit it.
    let text = toml::to_string(&file).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
    write_atomic(&paths.config(), &text)?;
    write_or_remove(&paths.agent_mcp(agent), config.mcp.as_ref())?;
    write_or_remove(&paths.agent_hooks(agent), config.hooks.as_ref())?;
    if agents::profile(agent).is_some() {
        write_claude_settings(paths)?;
    }
    Ok(())
}

fn write_or_remove(path: &Path, value: Option<&Value>) -> Result<()> {
    match value {
        Some(value) => {
            let text = serde_json::to_string_pretty(value).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
            write_atomic(path, &text)
        }
        None => match std::fs::remove_file(path) {
            Err(e) if e.kind() != io::ErrorKind::NotFound => Err(e.into()),
            _ => Ok(()),
        },
    }
}

/// Writes via a temp file in the same directory and a rename, so readers never see half a file.
pub fn write_atomic(path: &Path, contents: &str) -> Result<()> {
    let dir = path.parent().ok_or_else(|| Error::new(ErrorKind::Internal, "path has no parent"))?;
    std::fs::create_dir_all(dir)?;
    let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
    let temp = dir.join(format!(".{name}.tmp"));
    std::fs::write(&temp, contents)?;
    std::fs::rename(&temp, path)?;
    Ok(())
}

/// asterism's status hooks followed by the user's hooks, per event.
pub fn merged_claude_settings(user_hooks: Option<&Value>) -> Value {
    let mut settings = agents::claude_settings();
    let extra = user_hooks.and_then(|h| h.get("hooks")).and_then(Value::as_object);
    if let (Some(target), Some(extra)) = (settings["hooks"].as_object_mut(), extra) {
        for (event, entries) in extra {
            let list = target.entry(event.clone()).or_insert_with(|| Value::Array(Vec::new()));
            if let (Some(list), Some(entries)) = (list.as_array_mut(), entries.as_array()) {
                list.extend(entries.iter().cloned());
            }
        }
    }
    settings
}

/// Rewrites the `--settings` file; a broken hooks.json falls back to the status hooks alone.
pub fn write_claude_settings(paths: &Paths) -> Result<()> {
    let user = read_json(&paths.agent_hooks(agents::CLAUDE.name)).unwrap_or_else(|e| {
        eprintln!("asterismd: ignoring custom Claude hooks: {}", e.message);
        None
    });
    write_atomic(&paths.claude_settings(), &merged_claude_settings(user.as_ref()).to_string())
}

pub fn launch(paths: &Paths, profile: &AgentProfile) -> Result<Launch> {
    let args = Config::load(&paths.config())?.agents.remove(profile.name).map(|a| a.args).unwrap_or_default();
    let mcp = paths.agent_mcp(profile.name);
    Ok(Launch { settings: paths.claude_settings(), mcp_config: mcp.exists().then_some(mcp), args })
}

pub fn start_argv(paths: &Paths, profile: &AgentProfile, prompt: Option<&str>) -> Result<Vec<String>> {
    Ok(profile.start_argv(&launch(paths, profile)?, prompt))
}

/// The resume command for an agent session with a stored agent reference; `None` when not resumable.
pub fn resume_argv(paths: &Paths, kind: &SessionKind, agent_ref: Option<&str>) -> Result<Option<Vec<String>>> {
    let (SessionKind::Agent { name }, Some(agent_ref)) = (kind, agent_ref) else { return Ok(None) };
    let Some(profile) = agents::profile(name) else { return Ok(None) };
    Ok(Some(profile.resume_argv(&launch(paths, profile)?, agent_ref)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn temp_paths() -> (tempfile::TempDir, Paths) {
        let dir = tempfile::tempdir().unwrap();
        let paths = Paths { home: dir.path().join("h") };
        paths.ensure_dirs().unwrap();
        (dir, paths)
    }

    fn claude_config() -> AgentConfig {
        AgentConfig {
            args: vec!["--model".into(), "opus".into()],
            env: EnvSettings {
                remove: vec!["AWS_*".into()],
                set: [("FOO".to_string(), "bar".to_string())].into(),
            },
            mcp: Some(json!({"mcpServers": {"fs": {"command": "npx", "args": ["fs-mcp"]}}})),
            hooks: Some(json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}]}})),
        }
    }

    #[test]
    fn save_and_load_roundtrip() {
        let (_dir, paths) = temp_paths();
        save(&paths, "claude", &claude_config()).unwrap();
        assert_eq!(load(&paths, "claude").unwrap(), claude_config());
        assert!(paths.agent_mcp("claude").exists());

        let mut cleared = claude_config();
        cleared.mcp = None;
        save(&paths, "claude", &cleared).unwrap();
        assert!(!paths.agent_mcp("claude").exists());
        assert_eq!(load(&paths, "claude").unwrap().mcp, None);
    }

    #[test]
    fn missing_files_load_as_defaults() {
        let (_dir, paths) = temp_paths();
        assert_eq!(load(&paths, "claude").unwrap(), AgentConfig::default());
        assert_eq!(load(&paths, "shell").unwrap(), AgentConfig::default());
    }

    #[test]
    fn unknown_agents_are_rejected() {
        let (_dir, paths) = temp_paths();
        for agent in ["../../x", "nope", ""] {
            assert_eq!(load(&paths, agent).unwrap_err().kind, ErrorKind::InvalidParams);
            assert_eq!(save(&paths, agent, &AgentConfig::default()).unwrap_err().kind, ErrorKind::InvalidParams);
        }
    }

    #[test]
    fn invalid_settings_change_nothing() {
        let (_dir, paths) = temp_paths();
        save(&paths, "claude", &claude_config()).unwrap();
        let config_before = std::fs::read_to_string(paths.config()).unwrap();
        let mcp_before = std::fs::read_to_string(paths.agent_mcp("claude")).unwrap();

        let invalid = [
            AgentConfig { mcp: Some(json!({"servers": {}})), ..claude_config() },
            AgentConfig { hooks: Some(json!({"hooks": {"Stop": {"not": "a list"}}})), ..claude_config() },
            AgentConfig { hooks: Some(json!({"hooks": {"Stop": [{"command": "x"}]}})), ..claude_config() },
            AgentConfig { args: vec!["bad\0arg".into()], ..claude_config() },
            AgentConfig {
                env: EnvSettings { set: [("A=B".to_string(), "x".to_string())].into(), ..Default::default() },
                ..claude_config()
            },
            AgentConfig { env: EnvSettings { remove: vec![String::new()], ..Default::default() }, ..claude_config() },
        ];
        for config in invalid {
            assert_eq!(save(&paths, "claude", &config).unwrap_err().kind, ErrorKind::InvalidParams, "{config:?}");
        }
        assert_eq!(std::fs::read_to_string(paths.config()).unwrap(), config_before);
        assert_eq!(std::fs::read_to_string(paths.agent_mcp("claude")).unwrap(), mcp_before);
    }

    #[test]
    fn base_agents_only_take_environment() {
        let (_dir, paths) = temp_paths();
        let env_only = AgentConfig {
            env: EnvSettings { set: [("FOO".to_string(), "1".to_string())].into(), ..Default::default() },
            ..Default::default()
        };
        save(&paths, "shell", &env_only).unwrap();
        assert_eq!(load(&paths, "shell").unwrap(), env_only);
        for config in [
            AgentConfig { args: vec!["-l".into()], ..Default::default() },
            AgentConfig { mcp: Some(json!({"mcpServers": {}})), ..Default::default() },
            AgentConfig { hooks: Some(json!({"hooks": {}})), ..Default::default() },
        ] {
            assert_eq!(save(&paths, "command", &config).unwrap_err().kind, ErrorKind::InvalidParams);
        }
    }

    #[test]
    fn saving_keeps_other_agents_settings() {
        let (_dir, paths) = temp_paths();
        save(&paths, "claude", &claude_config()).unwrap();
        save(
            &paths,
            "shell",
            &AgentConfig {
                env: EnvSettings { set: [("X".to_string(), "1".to_string())].into(), ..Default::default() },
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(load(&paths, "claude").unwrap().args, ["--model", "opus"]);
    }

    #[test]
    fn merged_settings_append_user_hooks_after_status_hooks() {
        let user = json!({"hooks": {
            "Stop": [{"hooks": [{"type": "command", "command": "say done"}]}],
            "SessionStart": [{"hooks": [{"type": "command", "command": "echo hi"}]}]
        }});
        let merged = merged_claude_settings(Some(&user));
        let stop = merged["hooks"]["Stop"].as_array().unwrap();
        assert_eq!(stop.len(), 2);
        assert!(stop[0]["hooks"][0]["command"].as_str().unwrap().contains("hook stop"));
        assert_eq!(stop[1]["hooks"][0]["command"], "say done");
        assert_eq!(merged["hooks"]["SessionStart"][0]["hooks"][0]["command"], "echo hi");
        assert_eq!(merged_claude_settings(None), agents::claude_settings());
    }

    #[test]
    fn broken_hooks_file_falls_back_to_status_hooks() {
        let (_dir, paths) = temp_paths();
        std::fs::create_dir_all(paths.agent_dir("claude")).unwrap();
        std::fs::write(paths.agent_hooks("claude"), "{ not json").unwrap();
        write_claude_settings(&paths).unwrap();
        let written: Value = serde_json::from_str(&std::fs::read_to_string(paths.claude_settings()).unwrap()).unwrap();
        assert_eq!(written, agents::claude_settings());

        let err = load(&paths, "claude").unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidParams);
        assert!(err.message.contains("hooks.json"), "{}", err.message);
        assert_eq!(load_raw(&paths, "claude").unwrap().hooks_text.as_deref(), Some("{ not json"));
    }

    #[test]
    fn save_rewrites_the_merged_claude_settings() {
        let (_dir, paths) = temp_paths();
        save(&paths, "claude", &claude_config()).unwrap();
        let written = std::fs::read_to_string(paths.claude_settings()).unwrap();
        assert!(written.contains("say done"), "{written}");
    }

    #[test]
    fn argv_uses_the_saved_launch_settings() {
        let (_dir, paths) = temp_paths();
        save(&paths, "claude", &claude_config()).unwrap();
        let argv = start_argv(&paths, &agents::CLAUDE, Some("go")).unwrap();
        assert!(argv.windows(2).any(|w| w[0] == "--mcp-config"));
        assert_eq!(&argv[argv.len() - 4..], ["--model", "opus", "--", "go"]);

        let kind = SessionKind::Agent { name: "claude".into() };
        let resume = resume_argv(&paths, &kind, Some("ref")).unwrap().unwrap();
        assert_eq!(&resume[resume.len() - 2..], ["--resume", "ref"]);
        assert!(resume_argv(&paths, &kind, None).unwrap().is_none());
        assert!(resume_argv(&paths, &SessionKind::Shell, Some("ref")).unwrap().is_none());
    }

    #[test]
    fn atomic_write_leaves_no_temp_file() {
        let (_dir, paths) = temp_paths();
        let target = paths.agent_dir("claude").join("x.json");
        write_atomic(&target, "1").unwrap();
        write_atomic(&target, "2").unwrap();
        assert_eq!(std::fs::read_to_string(&target).unwrap(), "2");
        let names: Vec<_> = std::fs::read_dir(paths.agent_dir("claude")).unwrap().map(|e| e.unwrap().file_name()).collect();
        assert_eq!(names, ["x.json"]);
    }
}
