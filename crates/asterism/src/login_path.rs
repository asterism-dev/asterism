use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

const START: &str = "__ASTERISM_PATH_START__";
const END: &str = "__ASTERISM_PATH_END__";

/// The `PATH` between the markers; rc-file noise around them is ignored.
pub fn parse_login_path(stdout: &str) -> Option<String> {
    let rest = &stdout[stdout.find(START)? + START.len()..];
    Some(rest[..rest.find(END)?].to_string())
}

/// Login entries first, then inherited entries not already present.
pub fn merge_path(login: &str, inherited: &str) -> String {
    let mut entries: Vec<&str> = Vec::new();
    for entry in login.split(':').chain(inherited.split(':')) {
        if !entry.is_empty() && !entries.contains(&entry) {
            entries.push(entry);
        }
    }
    entries.join(":")
}

/// `None` on any failure, timeout included, so the caller keeps its inherited `PATH`.
pub fn probe_login_path(shell: &Path, timeout: Duration) -> Option<String> {
    let mut child = Command::new(shell)
        .args(["-l", "-i", "-c"])
        .arg(format!(r#"printf "\n{START}%s{END}\n" "$PATH""#))
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let (tx, rx) = mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut out = String::new();
        let _ = stdout.read_to_string(&mut out);
        let _ = tx.send(out);
    });
    let Ok(out) = rx.recv_timeout(timeout) else {
        let _ = child.kill();
        let _ = child.wait();
        return None;
    };
    let _ = child.wait();
    // The caller sets PATH next, which is only sound with no other thread alive.
    let _ = reader.join();
    parse_login_path(&out).filter(|path| !path.is_empty())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_ignores_rc_noise() {
        assert_eq!(
            parse_login_path(
                "Welcome!\nmotd\n__ASTERISM_PATH_START__/a:/b__ASTERISM_PATH_END__\nbye\n"
            ),
            Some("/a:/b".into())
        );
        assert_eq!(parse_login_path("Welcome!\n/a:/b\n"), None);
    }

    #[test]
    fn merge_keeps_login_first_and_dedups() {
        assert_eq!(merge_path("/a:/b", "/b:/c"), "/a:/b:/c");
    }
}
