use std::io::{IsTerminal, Read};
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::Value;

const STDIN_BUDGET: Duration = Duration::from_millis(1500);

pub fn normalize(event: &str, payload: &Value) -> Option<(&'static str, Option<String>)> {
    let event = match event {
        "prompt-submit" => "prompt-submit",
        "tool" => "tool",
        "stop" => "stop",
        "notification" => "notification",
        "subagent-start" => "subagent-start",
        "subagent-stop" | "subagent-end" | "subagent-failed" => "subagent-stop",
        _ => return None,
    };
    let message = payload["message"]
        .as_str()
        .unwrap_or_default()
        .to_lowercase();
    // ponytail: matches Claude's idle-prompt Notification text; verify against real Claude and update if it changes.
    let event = if event == "notification" && message.contains("waiting for your input") {
        "stop"
    } else {
        event
    };
    let event = if event == "stop"
        && asks_question(
            payload["last_assistant_message"]
                .as_str()
                .unwrap_or_default(),
        ) {
        "notification"
    } else {
        event
    };
    Some((event, payload["session_id"].as_str().map(String::from)))
}

// ponytail: any `?` followed by space or end counts as a question (URL queries excluded); "let me know if…" is missed.
fn asks_question(reply: &str) -> bool {
    reply.split('?').skip(1).any(|after| {
        after
            .trim_start_matches('*')
            .chars()
            .next()
            .is_none_or(char::is_whitespace)
    })
}

/// The CLI event and args for a subagent hook; `None` means send nothing.
pub fn subagent_args(event: &str, payload: &Value) -> Option<(&'static str, Vec<String>)> {
    // ponytail: Claude's payload shapes (tool_response.status/agentId, SubagentStop agent_id) change between Claude versions; verify and update.
    // `--flag=value` keeps values that start with `-` from parsing as flags.
    let tool_id = || {
        payload["tool_use_id"]
            .as_str()
            .map(|id| format!("--id={id}"))
    };
    match event {
        "subagent-start" => {
            let mut args = vec![tool_id()?];
            for (flag, key) in [
                ("--kind", "subagent_type"),
                ("--description", "description"),
            ] {
                if let Some(value) = payload["tool_input"][key].as_str() {
                    args.push(format!("{flag}={value}"));
                }
            }
            Some(("subagent-start", args))
        }
        // An async launch returns at once; its SubagentStop only knows the agent id.
        "subagent-stop" if payload["tool_response"]["status"] == "async_launched" => {
            let agent_id = payload["tool_response"]["agentId"].as_str()?;
            Some((
                "subagent-start",
                vec![tool_id()?, format!("--alias={agent_id}")],
            ))
        }
        "subagent-stop" => {
            let mut args = vec![tool_id()?];
            if payload["tool_response"]["is_error"].as_bool() == Some(true) {
                args.push("--failed".to_string());
            }
            Some(("subagent-stop", args))
        }
        "subagent-end" => {
            let id = payload["agent_id"].as_str().map(String::from).or_else(|| {
                let path = std::path::Path::new(payload["agent_transcript_path"].as_str()?);
                let stem = path.file_stem()?.to_str()?;
                stem.strip_prefix("agent-").map(String::from)
            })?;
            Some(("subagent-stop", vec![format!("--id={id}")]))
        }
        "subagent-failed" => Some(("subagent-stop", vec![tool_id()?, "--failed".to_string()])),
        _ => None,
    }
}

/// A hung stdin must not stall Claude's hook runner.
fn read_payload() -> Value {
    if std::io::stdin().is_terminal() {
        return Value::Null;
    }
    let (tx, rx) = mpsc::channel();
    std::thread::spawn(move || {
        let mut input = String::new();
        let _ = std::io::stdin().read_to_string(&mut input);
        let _ = tx.send(input);
    });
    rx.recv_timeout(STDIN_BUDGET)
        .ok()
        .and_then(|input| serde_json::from_str(&input).ok())
        .unwrap_or_default()
}

/// Runs inside Claude's hooks: must never fail or block.
pub fn run(event: &str) -> ! {
    let payload = read_payload();
    let normalized = normalize(event, &payload).and_then(|(cli_event, agent_ref)| {
        if !event.starts_with("subagent-") {
            return Some((cli_event, Vec::new(), agent_ref));
        }
        let (cli_event, extra) = subagent_args(event, &payload)?;
        Some((cli_event, extra, agent_ref))
    });
    if let (Some((event, extra, agent_ref)), Some(cli)) =
        (normalized, std::env::var_os("ASTERISM_CLI"))
    {
        let mut command = Command::new(cli);
        command
            .args(["hook", event])
            .args(extra)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null());
        if let Some(agent_ref) = agent_ref {
            command.args(["--agent-ref", &agent_ref]);
        }
        let _ = command.status();
    }
    std::process::exit(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn idle_prompt_notifications_mean_stop() {
        let waiting = json!({"session_id": "x", "message": "Claude is waiting for your input"});
        assert_eq!(
            normalize("notification", &waiting),
            Some(("stop", Some("x".to_string())))
        );
        let asking =
            json!({"session_id": "x", "message": "Claude needs your permission to use Bash"});
        assert_eq!(
            normalize("notification", &asking),
            Some(("notification", Some("x".to_string())))
        );
    }

    #[test]
    fn stops_with_a_question_mean_waiting() {
        let asks = json!({"session_id": "x", "last_assistant_message": "Done.\n\n**Question:** should I commit?**\n"});
        assert_eq!(
            normalize("stop", &asks),
            Some(("notification", Some("x".to_string())))
        );
        let mid = json!({"last_assistant_message": "Danke!\n\nWoran arbeiten wir heute? Das Worktree ist sauber."});
        assert_eq!(normalize("stop", &mid), Some(("notification", None)));
        let url = json!({"last_assistant_message": "See https://x.io/pro?from=start for details."});
        assert_eq!(normalize("stop", &url), Some(("stop", None)));
        let earlier = json!({"last_assistant_message": "Should I?\n\nI did, done."});
        assert_eq!(normalize("stop", &earlier), Some(("notification", None)));
        assert_eq!(normalize("stop", &Value::Null), Some(("stop", None)));
    }

    #[test]
    fn subagent_tool_calls_become_subagent_args() {
        let start = json!({"tool_use_id": "toolu_1", "tool_input": {"subagent_type": "Explore", "description": "find it"}});
        assert_eq!(
            normalize("subagent-start", &start).map(|n| n.0),
            Some("subagent-start")
        );
        assert_eq!(
            subagent_args("subagent-start", &start),
            Some((
                "subagent-start",
                vec![
                    "--id=toolu_1".into(),
                    "--kind=Explore".into(),
                    "--description=find it".into()
                ]
            ))
        );
        let done = json!({"tool_use_id": "toolu_1", "tool_response": {"status": "completed"}});
        assert_eq!(
            subagent_args("subagent-stop", &done),
            Some(("subagent-stop", vec!["--id=toolu_1".into()]))
        );
        let failed = json!({"tool_use_id": "toolu_1", "tool_response": {"is_error": true}});
        assert_eq!(
            subagent_args("subagent-stop", &failed),
            Some((
                "subagent-stop",
                vec!["--id=toolu_1".into(), "--failed".into()]
            ))
        );
        assert_eq!(subagent_args("subagent-start", &json!({})), None);
        assert_eq!(subagent_args("tool", &start), None);
    }

    #[test]
    fn async_launches_alias_the_tool_call_to_the_agent_id() {
        let launched = json!({"tool_use_id": "toolu_2", "tool_response": {"isAsync": true, "status": "async_launched", "agentId": "a88"}});
        assert_eq!(
            subagent_args("subagent-stop", &launched),
            Some((
                "subagent-start",
                vec!["--id=toolu_2".into(), "--alias=a88".into()]
            ))
        );
        let no_agent =
            json!({"tool_use_id": "toolu_2", "tool_response": {"status": "async_launched"}});
        assert_eq!(subagent_args("subagent-stop", &no_agent), None);
    }

    #[test]
    fn subagent_stop_and_tool_failure_end_the_subagent() {
        assert_eq!(
            normalize("subagent-end", &json!({"session_id": "s"})),
            Some(("subagent-stop", Some("s".to_string())))
        );
        assert_eq!(
            normalize("subagent-failed", &Value::Null),
            Some(("subagent-stop", None))
        );
        let ended =
            json!({"agent_id": "a88", "agent_transcript_path": "/t/subagents/agent-zzz.jsonl"});
        assert_eq!(
            subagent_args("subagent-end", &ended),
            Some(("subagent-stop", vec!["--id=a88".into()]))
        );
        let path_only = json!({"agent_transcript_path": "/t/subagents/agent-a88.jsonl"});
        assert_eq!(
            subagent_args("subagent-end", &path_only),
            Some(("subagent-stop", vec!["--id=a88".into()]))
        );
        let foreign = json!({"agent_transcript_path": "/t/other.jsonl"});
        assert_eq!(subagent_args("subagent-end", &foreign), None);
        assert_eq!(subagent_args("subagent-end", &json!({})), None);
        let failure = json!({"tool_use_id": "toolu_3", "error": "boom", "is_interrupt": true});
        assert_eq!(
            subagent_args("subagent-failed", &failure),
            Some((
                "subagent-stop",
                vec!["--id=toolu_3".into(), "--failed".into()]
            ))
        );
        assert_eq!(subagent_args("subagent-failed", &json!({})), None);
    }

    #[test]
    fn events_pass_through_and_unknown_ones_are_dropped() {
        assert_eq!(
            normalize("prompt-submit", &Value::Null),
            Some(("prompt-submit", None))
        );
        assert_eq!(
            normalize("tool", &json!({"session_id": "s"})),
            Some(("tool", Some("s".to_string())))
        );
        assert_eq!(normalize("bogus", &Value::Null), None);
    }
}
