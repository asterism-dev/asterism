use std::path::PathBuf;

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

/// Everything Claude is launched with besides the prompt or resume reference.
pub struct Launch {
    pub settings: PathBuf,
    pub mcp_config: Option<PathBuf>,
    pub args: Vec<String>,
}

impl AgentProfile {
    fn base_argv(&self, launch: &Launch) -> Vec<String> {
        let mut argv = vec![self.binary.to_string(), "--settings".into(), launch.settings.display().to_string()];
        if let Some(mcp) = &launch.mcp_config {
            argv.extend(["--mcp-config".to_string(), mcp.display().to_string()]);
        }
        argv.extend(launch.args.iter().cloned());
        argv
    }

    pub fn start_argv(&self, launch: &Launch, prompt: Option<&str>) -> Vec<String> {
        let mut argv = self.base_argv(launch);
        if let Some(prompt) = prompt {
            argv.extend(["--".to_string(), prompt.to_string()]);
        }
        argv
    }

    pub fn resume_argv(&self, launch: &Launch, agent_ref: &str) -> Vec<String> {
        let mut argv = self.base_argv(launch);
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

    #[test]
    fn claude_argv_puts_settings_mcp_and_args_before_prompt_or_resume() {
        let launch = Launch {
            settings: "/h/claude-settings.json".into(),
            mcp_config: Some("/h/agents/claude/mcp.json".into()),
            args: vec!["--model".into(), "opus".into()],
        };
        assert_eq!(
            CLAUDE.start_argv(&launch, Some("fix it")),
            [
                "claude", "--settings", "/h/claude-settings.json", "--mcp-config", "/h/agents/claude/mcp.json",
                "--model", "opus", "--", "fix it",
            ]
        );
        assert_eq!(
            CLAUDE.resume_argv(&launch, "abc"),
            [
                "claude", "--settings", "/h/claude-settings.json", "--mcp-config", "/h/agents/claude/mcp.json",
                "--model", "opus", "--resume", "abc",
            ]
        );
        let plain = Launch { settings: "/s.json".into(), mcp_config: None, args: vec![] };
        assert_eq!(CLAUDE.start_argv(&plain, None), ["claude", "--settings", "/s.json"]);
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
