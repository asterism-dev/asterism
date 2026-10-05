use std::io;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Mutex;

use asterism_proto::paths::Paths;
use asterism_proto::rpc::ErrorKind;
use asterism_plugin::protocol::LaunchSettings;
use asterism_proto::types::{AgentConfig, AgentConfigRaw, AgentSettingKind, EnvSettings};
use serde_json::Value;

use crate::config::{Config, EnvPolicy};
use crate::error::{Error, Result};
use crate::plugins::registry::Registry;

/// Serialises settings writers: config.toml is read-modify-write and the temp names must not collide.
pub(crate) static SAVE_LOCK: Mutex<()> = Mutex::new(());
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Session kinds that only take environment settings.
pub const BASE_AGENTS: &[&str] = &["shell", "command"];

fn invalid(message: impl Into<String>) -> Error {
    Error::new(ErrorKind::InvalidParams, message)
}

/// Only known agent keys may name settings files, which also rules out path traversal.
pub fn check_agent(registry: &Registry, agent: &str) -> Result<()> {
    if BASE_AGENTS.contains(&agent) || registry.agent(agent).is_some() {
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

pub fn load(paths: &Paths, registry: &Registry, agent: &str) -> Result<AgentConfig> {
    check_agent(registry, agent)?;
    let entry = Config::load(&paths.config())?.agents.remove(agent).unwrap_or_default();
    Ok(AgentConfig {
        args: entry.args,
        env: EnvSettings { remove: entry.env.remove, set: entry.env.set },
        mcp: read_json(&paths.agent_mcp(agent))?,
        hooks: read_json(&paths.agent_hooks(agent))?,
    })
}

pub fn load_raw(paths: &Paths, registry: &Registry, agent: &str) -> Result<AgentConfigRaw> {
    check_agent(registry, agent)?;
    let entry = Config::load(&paths.config())?.agents.remove(agent).unwrap_or_default();
    Ok(AgentConfigRaw {
        args: entry.args,
        env: EnvSettings { remove: entry.env.remove, set: entry.env.set },
        mcp_text: read_text(&paths.agent_mcp(agent))?,
        hooks_text: read_text(&paths.agent_hooks(agent))?,
    })
}

pub fn validate(registry: &Registry, agent: &str, config: &AgentConfig) -> Result<()> {
    check_agent(registry, agent)?;
    if BASE_AGENTS.contains(&agent) && (!config.args.is_empty() || config.mcp.is_some() || config.hooks.is_some()) {
        return Err(invalid(format!("{agent} sessions only support environment settings")));
    }
    if config.args.iter().any(|arg| arg.contains('\0')) {
        return Err(invalid("parameters must not contain NUL characters"));
    }
    if config.args.iter().any(|arg| arg.trim().is_empty()) {
        return Err(invalid("parameters must not be blank"));
    }
    let decl = registry.agent(agent).map(|(_, decl)| decl);
    if let Some(decl) = decl {
        for (used, kind, what) in [
            (!config.args.is_empty(), AgentSettingKind::Args, "parameters"),
            (config.mcp.is_some(), AgentSettingKind::Mcp, "MCP servers"),
            (config.hooks.is_some(), AgentSettingKind::Hooks, "hooks"),
        ] {
            if used && !decl.settings.contains(&kind) {
                return Err(invalid(format!("{agent} does not support {what}")));
            }
        }
        let reserved = |arg: &str| {
            decl.reserved_args.iter().any(|r| if r.ends_with('=') { arg.starts_with(r.as_str()) } else { arg == r })
        };
        if let Some(arg) = config.args.iter().find(|a| reserved(a)) {
            return Err(invalid(format!("parameter {arg:?} is managed by asterism and cannot be set")));
        }
    }
    if let Some(name) = config.env.set.keys().find(|name| name.is_empty() || name.contains(['=', '\0'])) {
        return Err(invalid(format!("invalid environment variable name {name:?}")));
    }
    if config.env.remove.iter().any(String::is_empty) {
        return Err(invalid("remove patterns must not be empty"));
    }
    if config.env.remove.iter().any(|pattern| pattern.contains('\0')) {
        return Err(invalid("remove patterns must not contain NUL characters"));
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

pub fn save(paths: &Paths, registry: &Registry, agent: &str, config: &AgentConfig) -> Result<()> {
    validate(registry, agent, config)?;
    let _guard = crate::lock(&SAVE_LOCK);
    let mut file = Config::load(&paths.config())?;
    let entry = file.agents.entry(agent.to_string()).or_default();
    entry.args = config.args.clone();
    entry.env = EnvPolicy { remove: config.env.remove.clone(), set: config.env.set.clone() };
    // ponytail: rewriting config.toml drops hand-written comments; switch to toml_edit if people hand-edit it.
    let text = toml::to_string(&file).map_err(|e| Error::new(ErrorKind::Internal, e.to_string()))?;
    write_atomic(&paths.config(), &text)?;
    write_or_remove(&paths.agent_mcp(agent), config.mcp.as_ref())?;
    write_or_remove(&paths.agent_hooks(agent), config.hooks.as_ref())?;
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
    let unique = TEMP_COUNTER.fetch_add(1, Ordering::Relaxed);
    let temp = dir.join(format!(".{name}.{}.{unique}.tmp", std::process::id()));
    let result = std::fs::write(&temp, contents).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    Ok(result?)
}

/// What `agent.prepare` needs; a lenient read (resume after restart) skips broken files instead of failing.
pub fn launch_settings(paths: &Paths, agent: &str, lenient: bool) -> Result<LaunchSettings> {
    let args = Config::load(&paths.config())?.agents.remove(agent).map(|a| a.args).unwrap_or_default();
    let tolerate = |path: &Path, read: Result<Option<Value>>| match read {
        Err(e) if lenient => {
            eprintln!("asterismd: resuming without {}: {}", path.display(), e.message);
            Ok(None)
        }
        other => other,
    };
    let mcp_path = paths.agent_mcp(agent);
    let has_mcp = tolerate(&mcp_path, read_json(&mcp_path))?.is_some();
    let hooks_path = paths.agent_hooks(agent);
    let hooks = tolerate(&hooks_path, read_json(&hooks_path))?;
    Ok(LaunchSettings { args, mcp_config: has_mcp.then(|| mcp_path.display().to_string()), hooks })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    use crate::plugins::registry::Sources;

    fn registry() -> Registry {
        Registry::discover(&Sources::default())
    }

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
        save(&paths, &registry(), "claude", &claude_config()).unwrap();
        assert_eq!(load(&paths, &registry(), "claude").unwrap(), claude_config());
        assert!(paths.agent_mcp("claude").exists());

        let mut cleared = claude_config();
        cleared.mcp = None;
        save(&paths, &registry(), "claude", &cleared).unwrap();
        assert!(!paths.agent_mcp("claude").exists());
        assert_eq!(load(&paths, &registry(), "claude").unwrap().mcp, None);
    }

    #[test]
    fn missing_files_load_as_defaults() {
        let (_dir, paths) = temp_paths();
        assert_eq!(load(&paths, &registry(), "claude").unwrap(), AgentConfig::default());
        assert_eq!(load(&paths, &registry(), "shell").unwrap(), AgentConfig::default());
    }

    #[test]
    fn unknown_agents_are_rejected() {
        let (_dir, paths) = temp_paths();
        for agent in ["../../x", "nope", ""] {
            assert_eq!(load(&paths, &registry(), agent).unwrap_err().kind, ErrorKind::InvalidParams);
            assert_eq!(save(&paths, &registry(), agent, &AgentConfig::default()).unwrap_err().kind, ErrorKind::InvalidParams);
        }
    }

    #[test]
    fn invalid_settings_change_nothing() {
        let (_dir, paths) = temp_paths();
        save(&paths, &registry(), "claude", &claude_config()).unwrap();
        let config_before = std::fs::read_to_string(paths.config()).unwrap();
        let mcp_before = std::fs::read_to_string(paths.agent_mcp("claude")).unwrap();
        let hooks_before = std::fs::read_to_string(paths.agent_hooks("claude")).unwrap();

        let invalid = [
            AgentConfig { mcp: Some(json!({"servers": {}})), ..claude_config() },
            AgentConfig { hooks: Some(json!({"hooks": {"Stop": {"not": "a list"}}})), ..claude_config() },
            AgentConfig { hooks: Some(json!({"hooks": {"Stop": [{"command": "x"}]}})), ..claude_config() },
            AgentConfig { args: vec!["bad\0arg".into()], ..claude_config() },
            AgentConfig { args: vec!["  ".into()], ..claude_config() },
            AgentConfig {
                env: EnvSettings { set: [("A=B".to_string(), "x".to_string())].into(), ..Default::default() },
                ..claude_config()
            },
            AgentConfig { env: EnvSettings { remove: vec![String::new()], ..Default::default() }, ..claude_config() },
            AgentConfig { env: EnvSettings { remove: vec!["A\0".into()], ..Default::default() }, ..claude_config() },
        ];
        let reserved = ["--", "--settings", "--mcp-config", "--resume", "-r", "--continue", "-c", "--print", "-p", "--settings=x", "--mcp-config=x", "--resume=x"];
        let invalid: Vec<_> = invalid
            .into_iter()
            .chain(reserved.map(|arg| AgentConfig { args: vec!["--model".into(), arg.into()], ..claude_config() }))
            .collect();
        for config in invalid {
            assert_eq!(save(&paths, &registry(), "claude", &config).unwrap_err().kind, ErrorKind::InvalidParams, "{config:?}");
        }
        assert_eq!(std::fs::read_to_string(paths.config()).unwrap(), config_before);
        assert_eq!(std::fs::read_to_string(paths.agent_mcp("claude")).unwrap(), mcp_before);
        assert_eq!(std::fs::read_to_string(paths.agent_hooks("claude")).unwrap(), hooks_before);
    }

    #[test]
    fn concurrent_saves_keep_every_agent_and_leave_no_temp_files() {
        let (_dir, paths) = temp_paths();
        let paths = std::sync::Arc::new(paths);
        let threads: Vec<_> = (0..8)
            .map(|t| {
                let paths = paths.clone();
                std::thread::spawn(move || {
                    let (agent, last) = if t % 2 == 0 { ("claude", format!("c{t}")) } else { ("shell", format!("s{t}")) };
                    for i in 0..50 {
                        let value = if i == 49 { last.clone() } else { format!("{agent}{i}") };
                        let config = AgentConfig {
                            env: EnvSettings { set: [(agent.to_string(), value)].into(), ..Default::default() },
                            ..Default::default()
                        };
                        save(&paths, &registry(), agent, &config).unwrap();
                    }
                })
            })
            .collect();
        for t in threads {
            t.join().unwrap();
        }
        assert!(load(&paths, &registry(), "claude").unwrap().env.set["claude"].starts_with('c'));
        assert!(load(&paths, &registry(), "shell").unwrap().env.set["shell"].starts_with('s'));
        let temps = std::fs::read_dir(&paths.home).unwrap().filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().ends_with(".tmp")).count();
        assert_eq!(temps, 0);
    }

    #[test]
    fn base_agents_only_take_environment() {
        let (_dir, paths) = temp_paths();
        let env_only = AgentConfig {
            env: EnvSettings { set: [("FOO".to_string(), "1".to_string())].into(), ..Default::default() },
            ..Default::default()
        };
        save(&paths, &registry(), "shell", &env_only).unwrap();
        assert_eq!(load(&paths, &registry(), "shell").unwrap(), env_only);
        for config in [
            AgentConfig { args: vec!["-l".into()], ..Default::default() },
            AgentConfig { mcp: Some(json!({"mcpServers": {}})), ..Default::default() },
            AgentConfig { hooks: Some(json!({"hooks": {}})), ..Default::default() },
        ] {
            assert_eq!(save(&paths, &registry(), "command", &config).unwrap_err().kind, ErrorKind::InvalidParams);
        }
    }

    #[test]
    fn saving_keeps_other_agents_settings() {
        let (_dir, paths) = temp_paths();
        save(&paths, &registry(), "claude", &claude_config()).unwrap();
        save(
            &paths,
            &registry(),
            "shell",
            &AgentConfig {
                env: EnvSettings { set: [("X".to_string(), "1".to_string())].into(), ..Default::default() },
                ..Default::default()
            },
        )
        .unwrap();
        assert_eq!(load(&paths, &registry(), "claude").unwrap().args, ["--model", "opus"]);
    }

    #[test]
    fn broken_hooks_file_fails_load_but_not_raw_load() {
        let (_dir, paths) = temp_paths();
        std::fs::create_dir_all(paths.agent_dir("claude")).unwrap();
        std::fs::write(paths.agent_hooks("claude"), "{ not json").unwrap();
        let err = load(&paths, &registry(), "claude").unwrap_err();
        assert_eq!(err.kind, ErrorKind::InvalidParams);
        assert!(err.message.contains("hooks.json"), "{}", err.message);
        assert_eq!(load_raw(&paths, &registry(), "claude").unwrap().hooks_text.as_deref(), Some("{ not json"));
    }

    #[test]
    fn raw_load_keeps_args_and_env_next_to_broken_files() {
        let (_dir, paths) = temp_paths();
        save(&paths, &registry(), "claude", &claude_config()).unwrap();
        std::fs::write(paths.agent_hooks("claude"), "{ not json").unwrap();
        let raw = load_raw(&paths, &registry(), "claude").unwrap();
        assert_eq!(raw.args, claude_config().args);
        assert_eq!(raw.env, claude_config().env);
        assert_eq!(raw.hooks_text.as_deref(), Some("{ not json"));
    }

    #[test]
    fn launch_settings_are_strict_on_start_and_lenient_on_resume() {
        let (_dir, paths) = temp_paths();
        std::fs::create_dir_all(paths.agent_dir("claude")).unwrap();
        std::fs::write(paths.agent_hooks("claude"), "{ not json").unwrap();
        std::fs::write(paths.agent_mcp("claude"), "{\"mcpServers\": {}}").unwrap();
        assert_eq!(launch_settings(&paths, "claude", false).unwrap_err().kind, ErrorKind::InvalidParams);
        let lenient = launch_settings(&paths, "claude", true).unwrap();
        assert!(lenient.hooks.is_none());
        assert_eq!(lenient.mcp_config.as_deref(), Some(paths.agent_mcp("claude").to_str().unwrap()));
    }

    #[test]
    fn reserved_args_come_from_the_manifest() {
        for arg in ["--settings", "--resume=abc", "-p"] {
            let config = AgentConfig { args: vec![arg.into()], ..Default::default() };
            assert!(validate(&registry(), "claude", &config).is_err(), "{arg}");
        }
        let fine = AgentConfig { args: vec!["--model".into(), "opus".into()], ..Default::default() };
        assert!(validate(&registry(), "claude", &fine).is_ok());
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
