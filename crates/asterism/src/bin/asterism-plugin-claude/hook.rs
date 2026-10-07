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
        _ => return None,
    };
    let message = payload["message"].as_str().unwrap_or_default().to_lowercase();
    // ponytail: matches Claude's idle-prompt Notification text; verify against real Claude and update if it changes.
    let event = if event == "notification" && message.contains("waiting for your input") { "stop" } else { event };
    let event = if event == "stop" && asks_question(payload["last_assistant_message"].as_str().unwrap_or_default()) { "notification" } else { event };
    Some((event, payload["session_id"].as_str().map(String::from)))
}

// ponytail: any `?` followed by space or end counts as a question (URL queries excluded); "let me know if…" is missed.
fn asks_question(reply: &str) -> bool {
    reply.split('?').skip(1).any(|after| after.trim_start_matches('*').chars().next().is_none_or(char::is_whitespace))
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
    rx.recv_timeout(STDIN_BUDGET).ok().and_then(|input| serde_json::from_str(&input).ok()).unwrap_or_default()
}

/// Runs inside Claude's hooks: must never fail or block.
pub fn run(event: &str) -> ! {
    let payload = read_payload();
    if let (Some((event, agent_ref)), Some(cli)) = (normalize(event, &payload), std::env::var_os("ASTERISM_CLI")) {
        let mut command = Command::new(cli);
        command.args(["hook", event]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null());
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
        assert_eq!(normalize("notification", &waiting), Some(("stop", Some("x".to_string()))));
        let asking = json!({"session_id": "x", "message": "Claude needs your permission to use Bash"});
        assert_eq!(normalize("notification", &asking), Some(("notification", Some("x".to_string()))));
    }

    #[test]
    fn stops_with_a_question_mean_waiting() {
        let asks = json!({"session_id": "x", "last_assistant_message": "Done.\n\n**Question:** should I commit?**\n"});
        assert_eq!(normalize("stop", &asks), Some(("notification", Some("x".to_string()))));
        let mid = json!({"last_assistant_message": "Danke!\n\nWoran arbeiten wir heute? Das Worktree ist sauber."});
        assert_eq!(normalize("stop", &mid), Some(("notification", None)));
        let url = json!({"last_assistant_message": "See https://x.io/pro?from=start for details."});
        assert_eq!(normalize("stop", &url), Some(("stop", None)));
        let earlier = json!({"last_assistant_message": "Should I?\n\nI did, done."});
        assert_eq!(normalize("stop", &earlier), Some(("notification", None)));
        assert_eq!(normalize("stop", &Value::Null), Some(("stop", None)));
    }

    #[test]
    fn events_pass_through_and_unknown_ones_are_dropped() {
        assert_eq!(normalize("prompt-submit", &Value::Null), Some(("prompt-submit", None)));
        assert_eq!(normalize("tool", &json!({"session_id": "s"})), Some(("tool", Some("s".to_string()))));
        assert_eq!(normalize("bogus", &Value::Null), None);
    }
}
