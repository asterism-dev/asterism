use std::collections::BTreeSet;

use asterism_plugin::protocol::PROTOCOL;
use asterism_proto::types::{AgentSettingKind, Capability, CapabilityKind, SettingSpec, SettingType};
use serde::Deserialize;

/// CLI subcommands a plugin command may not shadow.
pub const BUILTIN_COMMANDS: &[&str] = &["project", "task", "session", "send", "read", "wait", "attach", "hook", "daemon", "plugin", "store", "help"];
/// Built-in session kinds that share the agent namespace for settings.
const SESSION_KINDS: &[&str] = &["shell", "command"];

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub protocol: u32,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub permissions: Vec<String>,
    pub backend: Option<BackendDecl>,
    #[serde(default)]
    pub provides: Provides,
    #[serde(default)]
    pub settings: Vec<SettingSpec>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BackendDecl {
    pub command: Vec<String>,
}

#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Provides {
    #[serde(default)]
    pub forge: Vec<ForgeDecl>,
    #[serde(default)]
    pub agent: Vec<AgentDecl>,
    #[serde(default)]
    pub command: Vec<CommandDecl>,
    #[serde(default)]
    pub task_source: Vec<TaskSourceDecl>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ForgeDecl {
    pub id: String,
    pub display_name: String,
    #[serde(default)]
    pub hosts: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum LaunchKind {
    Static,
    Backend,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentDecl {
    pub id: String,
    #[serde(default)]
    pub display_name: Option<String>,
    pub binary: String,
    pub launch: LaunchKind,
    #[serde(default)]
    pub waiting_patterns: Vec<String>,
    #[serde(default)]
    pub settings: Vec<AgentSettingKind>,
    /// Flags the user may not pass as args; entries ending in `=` match as prefixes.
    #[serde(default)]
    pub reserved_args: Vec<String>,
    #[serde(default)]
    pub start: Vec<String>,
    #[serde(default)]
    pub prompt: Vec<String>,
    #[serde(default)]
    pub resume: Vec<String>,
}

impl AgentDecl {
    pub fn display_name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.id)
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CommandDecl {
    pub name: String,
    #[serde(default)]
    pub description: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskSourceDecl {
    pub id: String,
    pub display_name: String,
}

impl Manifest {
    /// Capabilities the backend must confirm in its `initialize` reply.
    pub fn backend_capabilities(&self) -> BTreeSet<String> {
        let provides = &self.provides;
        [
            ("forge", !provides.forge.is_empty()),
            ("agent", provides.agent.iter().any(|a| a.launch == LaunchKind::Backend)),
            ("command", !provides.command.is_empty()),
            ("task_source", !provides.task_source.is_empty()),
        ]
        .into_iter()
        .filter(|(_, present)| *present)
        .map(|(kind, _)| kind.to_string())
        .collect()
    }

    pub fn capabilities(&self) -> Vec<Capability> {
        let cap = |kind, id: &str, description: &str| Capability { kind, id: id.to_string(), description: description.to_string() };
        let p = &self.provides;
        p.forge
            .iter()
            .map(|f| cap(CapabilityKind::Forge, &f.id, &f.display_name))
            .chain(p.agent.iter().map(|a| cap(CapabilityKind::Agent, &a.id, a.display_name())))
            .chain(p.command.iter().map(|c| cap(CapabilityKind::Command, &c.name, &c.description)))
            .chain(p.task_source.iter().map(|t| cap(CapabilityKind::TaskSource, &t.id, &t.display_name)))
            .collect()
    }
}

pub fn parse(text: &str) -> Result<Manifest, String> {
    let manifest: Manifest = toml::from_str(text).map_err(|e| e.to_string().trim().to_string())?;
    validate(&manifest)?;
    Ok(manifest)
}

pub(crate) fn is_slug(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('-') && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-')
}

fn is_key(s: &str) -> bool {
    !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

pub(crate) fn is_version(s: &str) -> bool {
    !s.is_empty() && !s.starts_with('.') && s.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '+' | '-'))
}

fn unique<'a>(what: &str, ids: impl Iterator<Item = &'a String>) -> Result<(), String> {
    let mut seen = BTreeSet::new();
    for id in ids {
        if !seen.insert(id) {
            return Err(format!("duplicate {what} {id:?}"));
        }
    }
    Ok(())
}

fn validate(m: &Manifest) -> Result<(), String> {
    if !is_slug(&m.name) {
        return Err(format!("invalid plugin name {:?}: use a-z, 0-9 and '-'", m.name));
    }
    if !is_version(&m.version) {
        return Err(format!("invalid version {:?}: use letters, digits, '.', '_', '+' or '-'", m.version));
    }
    if m.protocol != PROTOCOL {
        return Err(format!("unsupported protocol {} (this asterism speaks {PROTOCOL})", m.protocol));
    }
    if m.backend.as_ref().is_some_and(|b| b.command.is_empty()) {
        return Err("backend.command must not be empty".into());
    }
    if !m.backend_capabilities().is_empty() && m.backend.is_none() {
        return Err("forges, commands, task sources and backend agents need a [backend] section".into());
    }
    let p = &m.provides;
    unique("forge", p.forge.iter().map(|f| &f.id))?;
    unique("agent", p.agent.iter().map(|a| &a.id))?;
    unique("command", p.command.iter().map(|c| &c.name))?;
    unique("task source", p.task_source.iter().map(|t| &t.id))?;
    unique("setting", m.settings.iter().map(|s| &s.key))?;
    for id in p.forge.iter().map(|f| &f.id).chain(p.task_source.iter().map(|t| &t.id)) {
        if !is_slug(id) {
            return Err(format!("invalid id {id:?}: use a-z, 0-9 and '-'"));
        }
    }
    for agent in &p.agent {
        if !is_slug(&agent.id) || SESSION_KINDS.contains(&agent.id.as_str()) {
            return Err(format!("invalid agent id {:?}", agent.id));
        }
        if agent.launch == LaunchKind::Static && agent.start.is_empty() {
            return Err(format!("agent {}: static agents need a start template", agent.id));
        }
    }
    for command in &p.command {
        if !is_slug(&command.name) || BUILTIN_COMMANDS.contains(&command.name.as_str()) {
            return Err(format!("command {:?} is not a valid name or clashes with a built-in subcommand", command.name));
        }
    }
    for setting in &m.settings {
        if !is_key(&setting.key) {
            return Err(format!("invalid setting key {:?}: use a-z, 0-9 and '_'", setting.key));
        }
        if setting.kind == SettingType::Enum && setting.options.is_empty() {
            return Err(format!("setting {}: enum settings need options", setting.key));
        }
        if let Some(default) = &setting.default {
            if setting.kind == SettingType::Secret {
                return Err(format!("setting {}: a secret cannot have a default", setting.key));
            }
            super::settings::check(setting, default).map_err(|e| format!("setting {}: default is invalid: {}", setting.key, e.message))?;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    const GITHUB: &str = r#"
name = "github"
version = "0.2.0"
protocol = 1
permissions = ["exec:gh"]

[backend]
command = ["./asterism-plugin-github"]

[[provides.forge]]
id = "github"
display_name = "GitHub"
hosts = ["github.com"]

[[provides.task_source]]
id = "github-issues"
display_name = "GitHub Issues"

[[settings]]
key = "api_key"
title = "API key"
type = "secret"
required = true
"#;

    fn with(extra: &str) -> String {
        format!("{GITHUB}\n{extra}")
    }

    #[test]
    fn parses_a_forge_plugin() {
        let m = parse(GITHUB).unwrap();
        assert_eq!(m.name, "github");
        assert_eq!(m.provides.forge[0].hosts, ["github.com"]);
        assert_eq!(m.provides.task_source[0].id, "github-issues");
        assert_eq!(m.settings[0].kind, SettingType::Secret);
        assert_eq!(m.backend_capabilities(), ["forge", "task_source"].map(String::from).into());
    }

    #[test]
    fn rejects_ui_and_unknown_sections() {
        assert!(parse(&with("[ui]\nentry = \"index.html\"")).unwrap_err().contains("ui"));
        assert!(parse(&with("[extra]\nx = 1")).is_err());
    }

    #[test]
    fn rejects_bad_names_protocols_and_missing_backends() {
        assert!(parse(&GITHUB.replace("name = \"github\"", "name = \"Git Hub\"")).unwrap_err().contains("invalid plugin name"));
        assert!(parse(&GITHUB.replace("protocol = 1", "protocol = 2")).unwrap_err().contains("unsupported protocol"));
        let no_backend = GITHUB.replace("[backend]\ncommand = [\"./asterism-plugin-github\"]", "");
        assert!(parse(&no_backend).unwrap_err().contains("[backend]"));
        assert!(parse(&GITHUB.replace("[\"./asterism-plugin-github\"]", "[]")).unwrap_err().contains("must not be empty"));
    }

    #[test]
    fn validates_agents_commands_and_settings() {
        let static_agent = "[[provides.agent]]\nid = \"aider\"\nbinary = \"aider\"\nlaunch = \"static\"\n";
        assert!(parse(&with(static_agent)).unwrap_err().contains("start template"));
        let shell = "[[provides.agent]]\nid = \"shell\"\nbinary = \"sh\"\nlaunch = \"backend\"\n";
        assert!(parse(&with(shell)).is_err());
        assert!(parse(&with("[[provides.command]]\nname = \"task\"\n")).unwrap_err().contains("built-in"));
        let duplicate = "[[settings]]\nkey = \"api_key\"\ntitle = \"Again\"\ntype = \"string\"\n";
        assert!(parse(&with(duplicate)).unwrap_err().contains("duplicate setting"));
        let enum_without_options = "[[settings]]\nkey = \"region\"\ntitle = \"Region\"\ntype = \"enum\"\n";
        assert!(parse(&with(enum_without_options)).unwrap_err().contains("options"));
    }

    #[test]
    fn rejects_mistyped_defaults() {
        let setting = |kind: &str, default: &str, options: &str| {
            with(&format!("[[settings]]\nkey = \"k\"\ntitle = \"K\"\ntype = \"{kind}\"\ndefault = {default}\n{options}"))
        };
        assert!(parse(&setting("number", "\"x\"", "")).unwrap_err().contains("default is invalid"));
        assert!(parse(&setting("bool", "1", "")).unwrap_err().contains("default is invalid"));
        assert!(parse(&setting("enum", "\"b\"", "options = [\"a\"]")).unwrap_err().contains("default is invalid"));
        assert!(parse(&setting("secret", "\"s\"", "")).unwrap_err().contains("secret cannot have a default"));
        assert!(parse(&setting("enum", "\"a\"", "options = [\"a\"]")).is_ok());
        assert!(parse(&setting("number", "3", "")).is_ok());
    }

    #[test]
    fn backend_capabilities_cover_forges_backend_agents_and_commands() {
        let extra = "[[provides.agent]]\nid = \"claude\"\nbinary = \"claude\"\nlaunch = \"backend\"\n\
                     [[provides.agent]]\nid = \"aider\"\nbinary = \"aider\"\nlaunch = \"static\"\nstart = [\"{binary}\"]\n\
                     [[provides.command]]\nname = \"gh-sync\"\n";
        let m = parse(&with(extra)).unwrap();
        assert_eq!(m.backend_capabilities(), ["agent", "command", "forge", "task_source"].map(String::from).into());
        assert_eq!(m.provides.agent[0].display_name(), "claude");
    }

    #[test]
    fn versions_must_be_safe_directory_names() {
        for bad in ["../1", ".hidden", "1/2", "", "1 2"] {
            assert!(parse(&GITHUB.replace("version = \"0.2.0\"", &format!("version = {bad:?}"))).unwrap_err().contains("version"), "{bad}");
        }
        assert!(parse(&GITHUB.replace("0.2.0", "1.0.0-rc.1+build_7")).is_ok());
    }
}
