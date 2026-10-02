use std::path::Path;

use asterism_proto::types::SessionKind;
use serde_json::{json, Value};

pub struct AgentProfile {
    pub name: &'static str,
    pub binary: &'static str,
    /// Screen text meaning the agent is blocked on the user; used when hooks are unavailable.
    pub waiting_patterns: &'static [&'static str],
}

pub const CLAUDE: AgentProfile = AgentProfile {
    name: "claude",
    binary: "claude",
    waiting_patterns: &["Do you want to", "❯ 1. Yes"],
};

pub const PROFILES: &[AgentProfile] = &[CLAUDE];

pub fn profile(name: &str) -> Option<&'static AgentProfile> {
    PROFILES.iter().find(|p| p.name == name)
}

impl AgentProfile {
    pub fn start_argv(&self, settings: &Path, prompt: Option<&str>) -> Vec<String> {
        let mut argv = vec![self.binary.to_string(), "--settings".into(), settings.display().to_string()];
        if let Some(prompt) = prompt {
            argv.extend(["--".to_string(), prompt.to_string()]);
        }
        argv
    }

    pub fn resume_argv(&self, settings: &Path, agent_ref: &str) -> Vec<String> {
        let mut argv = self.start_argv(settings, None);
        argv.extend(["--resume".to_string(), agent_ref.to_string()]);
        argv
    }

    pub fn is_available(&self) -> bool {
        on_path(self.binary)
    }
}

pub fn waiting_patterns(kind: &SessionKind) -> &'static [&'static str] {
    match kind {
        SessionKind::Agent { name } => profile(name).map_or(&[], |p| p.waiting_patterns),
        _ => &[],
    }
}

pub fn resume_argv_for(kind: &SessionKind, agent_ref: Option<&str>, settings: &Path) -> Option<Vec<String>> {
    match kind {
        SessionKind::Agent { name } => Some(profile(name)?.resume_argv(settings, agent_ref?)),
        _ => None,
    }
}

pub fn on_path(binary: &str) -> bool {
    std::env::var_os("PATH")
        .is_some_and(|paths| std::env::split_paths(&paths).any(|dir| dir.join(binary).is_file()))
}

/// Claude settings passed via `--settings`, so the user's own config is never modified.
pub fn claude_settings() -> Value {
    let hook = |arg: &str| {
        json!([{ "hooks": [{ "type": "command", "command": format!("\"$ASTERISM_CLI\" hook {arg}") }] }])
    };
    json!({
        "hooks": {
            "UserPromptSubmit": hook("prompt-submit"),
            "PreToolUse": hook("tool"),
            "Stop": hook("stop"),
            "Notification": hook("notification"),
        }
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn claude_start_and_resume_commands() {
        let settings = Path::new("/h/claude-settings.json");
        assert_eq!(
            CLAUDE.start_argv(settings, Some("fix it")),
            ["claude", "--settings", "/h/claude-settings.json", "--", "fix it"]
        );
        assert_eq!(CLAUDE.start_argv(settings, None), ["claude", "--settings", "/h/claude-settings.json"]);
        assert_eq!(
            CLAUDE.resume_argv(settings, "abc"),
            ["claude", "--settings", "/h/claude-settings.json", "--resume", "abc"]
        );
    }

    #[test]
    fn only_agent_sessions_with_a_ref_are_resumable() {
        let settings = Path::new("/s.json");
        let claude = SessionKind::Agent { name: "claude".into() };
        assert!(resume_argv_for(&claude, Some("abc"), settings).is_some());
        assert!(resume_argv_for(&claude, None, settings).is_none());
        assert!(resume_argv_for(&SessionKind::Shell, Some("abc"), settings).is_none());
        let unknown = SessionKind::Agent { name: "nope".into() };
        assert!(resume_argv_for(&unknown, Some("abc"), settings).is_none());
    }

    #[test]
    fn waiting_patterns_come_from_the_profile() {
        assert_eq!(waiting_patterns(&SessionKind::Agent { name: "claude".into() }), CLAUDE.waiting_patterns);
        assert!(waiting_patterns(&SessionKind::Shell).is_empty());
    }

    #[test]
    fn settings_route_every_hook_to_the_cli() {
        let settings = claude_settings();
        for (event, arg) in [
            ("UserPromptSubmit", "prompt-submit"),
            ("PreToolUse", "tool"),
            ("Stop", "stop"),
            ("Notification", "notification"),
        ] {
            let command = settings["hooks"][event][0]["hooks"][0]["command"].as_str().unwrap();
            assert_eq!(command, format!("\"$ASTERISM_CLI\" hook {arg}"));
        }
    }

    #[test]
    fn on_path_finds_sh() {
        assert!(on_path("sh"));
        assert!(!on_path("definitely-not-a-binary-asterism"));
    }
}
