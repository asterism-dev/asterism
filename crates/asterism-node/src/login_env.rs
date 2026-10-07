use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const MARK: &str = "__ASTERISM_PATH__";
const PRINT_PATH: &str = "printf '__ASTERISM_PATH__%s__ASTERISM_PATH__' \"$PATH\"";
const LOGIN_SHELL_TIMEOUT: Duration = Duration::from_secs(5);

/// PATH as the user's interactive login shell sets it; GUI launches only get a minimal one.
pub fn login_shell_path() -> Option<String> {
    let shell = std::env::var("SHELL").ok()?;
    let child = Command::new(shell)
        .args(["-l", "-i", "-c", PRINT_PATH])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let (tx, rx) = mpsc::channel();
    // ponytail: a shell that hangs past the timeout is left running; kill it if that ever shows up.
    std::thread::spawn(move || {
        let _ = tx.send(child.wait_with_output());
    });
    let output = rx.recv_timeout(LOGIN_SHELL_TIMEOUT).ok()?.ok()?;
    extract_marked(&String::from_utf8_lossy(&output.stdout))
}

pub fn extract_marked(output: &str) -> Option<String> {
    let start = output.find(MARK)? + MARK.len();
    let end = start + output[start..].find(MARK)?;
    Some(output[start..end].to_string()).filter(|path| !path.is_empty())
}

/// Inherited PATH plus the usual user tool dirs, for when the login shell cannot be read.
pub fn fallback_path(inherited: &str, home: Option<&str>) -> String {
    let mut dirs: Vec<String> = inherited
        .split(':')
        .filter(|d| !d.is_empty())
        .map(String::from)
        .collect();
    let extra = [
        "/opt/homebrew/bin".to_string(),
        "/usr/local/bin".to_string(),
    ]
    .into_iter()
    .chain(home.map(|h| format!("{h}/.local/bin")));
    for dir in extra {
        if !dirs.contains(&dir) {
            dirs.push(dir);
        }
    }
    dirs.join(":")
}
