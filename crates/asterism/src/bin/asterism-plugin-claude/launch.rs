use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use asterism_plugin::protocol::{LaunchMode, PrepareParams, PrepareResult};
use asterism_plugin::{ErrorKind, RpcError};
use serde_json::{json, Map, Value};

const STATUS_HOOKS: &[(&str, &str)] =
    &[("UserPromptSubmit", "prompt-submit"), ("PreToolUse", "tool"), ("Stop", "stop"), ("Notification", "notification")];
static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Claude settings passed via `--settings`, so the user's own config is never modified.
pub fn status_settings(exe: &Path) -> Value {
    let mut hooks = Map::new();
    for (event, arg) in STATUS_HOOKS {
        let command = format!("\"{}\" hook {arg}", exe.display());
        hooks.insert(event.to_string(), json!([{ "hooks": [{ "type": "command", "command": command }] }]));
    }
    json!({ "hooks": hooks })
}

/// asterism's status hooks followed by the user's hooks, per event.
pub fn merged_settings(exe: &Path, user_hooks: Option<&Value>) -> Value {
    let mut settings = status_settings(exe);
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

fn io_error(e: std::io::Error) -> RpcError {
    RpcError::new(ErrorKind::Internal, e.to_string())
}

/// Concurrent prepares write identical content, so the last rename winning is fine.
fn write_atomic(path: &Path, contents: &str) -> Result<(), RpcError> {
    let dir = path.parent().ok_or_else(|| RpcError::new(ErrorKind::Internal, "settings path has no parent"))?;
    std::fs::create_dir_all(dir).map_err(io_error)?;
    let temp = dir.join(format!(".claude-settings.{}.{}.tmp", std::process::id(), TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)));
    let result = std::fs::write(&temp, contents).and_then(|()| std::fs::rename(&temp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&temp);
    }
    result.map_err(io_error)
}

pub fn prepare(data_dir: &Path, exe: &Path, p: &PrepareParams) -> Result<PrepareResult, RpcError> {
    let settings = data_dir.join("claude-settings.json");
    write_atomic(&settings, &merged_settings(exe, p.settings.hooks.as_ref()).to_string())?;
    let mut argv = vec!["claude".to_string(), "--settings".into(), settings.display().to_string()];
    if let Some(mcp) = &p.settings.mcp_config {
        argv.extend(["--mcp-config".to_string(), mcp.clone()]);
    }
    argv.extend(p.settings.args.iter().cloned());
    match p.mode {
        LaunchMode::Start => {
            if let Some(prompt) = &p.prompt {
                argv.extend(["--".to_string(), prompt.clone()]);
            }
        }
        LaunchMode::Resume => {
            let agent_ref = p.agent_ref.as_ref().ok_or_else(|| RpcError::new(ErrorKind::InvalidParams, "resume needs an agent reference"))?;
            argv.extend(["--resume".to_string(), agent_ref.clone()]);
        }
    }
    Ok(PrepareResult { argv, env: Vec::new() })
}

#[cfg(test)]
mod tests {
    use super::*;
    use asterism_plugin::protocol::LaunchSettings;
    use serde_json::json;
    use std::path::Path;

    const EXE: &str = "/opt/asterism/asterism-plugin-claude";

    fn params(mode: LaunchMode, prompt: Option<&str>, agent_ref: Option<&str>, settings: LaunchSettings) -> PrepareParams {
        PrepareParams { agent: "claude".into(), mode, prompt: prompt.map(String::from), agent_ref: agent_ref.map(String::from), settings }
    }

    #[test]
    fn status_hooks_call_this_binary() {
        let settings = status_settings(Path::new(EXE));
        for (event, arg) in [("UserPromptSubmit", "prompt-submit"), ("PreToolUse", "tool"), ("Stop", "stop"), ("Notification", "notification")] {
            let command = settings["hooks"][event][0]["hooks"][0]["command"].as_str().unwrap();
            assert_eq!(command, format!("\"{EXE}\" hook {arg}"));
        }
    }

    #[test]
    fn user_hooks_follow_status_hooks() {
        let user = json!({"hooks": {"Stop": [{"hooks": [{"type": "command", "command": "say done"}]}], "PostToolUse": [{"hooks": []}]}});
        let merged = merged_settings(Path::new(EXE), Some(&user));
        assert_eq!(merged["hooks"]["Stop"].as_array().unwrap().len(), 2);
        assert_eq!(merged["hooks"]["Stop"][1]["hooks"][0]["command"], "say done");
        assert!(merged["hooks"]["PostToolUse"].is_array());
        assert_eq!(merged_settings(Path::new(EXE), None), status_settings(Path::new(EXE)));
    }

    #[test]
    fn argv_puts_settings_mcp_and_args_before_prompt_or_resume() {
        let dir = tempfile::tempdir().unwrap();
        let settings = LaunchSettings { args: vec!["--model".into(), "opus".into()], mcp_config: Some("/h/mcp.json".into()), hooks: None };
        let file = dir.path().join("claude-settings.json").display().to_string();
        let start = prepare(dir.path(), Path::new(EXE), &params(LaunchMode::Start, Some("fix it"), None, settings.clone())).unwrap();
        assert_eq!(start.argv, ["claude", "--settings", &file, "--mcp-config", "/h/mcp.json", "--model", "opus", "--", "fix it"]);
        let resume = prepare(dir.path(), Path::new(EXE), &params(LaunchMode::Resume, None, Some("abc"), settings)).unwrap();
        assert_eq!(resume.argv[resume.argv.len() - 2..], ["--resume", "abc"]);
        let plain = prepare(dir.path(), Path::new(EXE), &params(LaunchMode::Start, None, None, LaunchSettings::default())).unwrap();
        assert_eq!(plain.argv, ["claude", "--settings", &file]);
        let written: Value = serde_json::from_str(&std::fs::read_to_string(&file).unwrap()).unwrap();
        assert_eq!(written, status_settings(Path::new(EXE)));
        assert!(prepare(dir.path(), Path::new(EXE), &params(LaunchMode::Resume, None, None, LaunchSettings::default())).is_err());
    }
}
