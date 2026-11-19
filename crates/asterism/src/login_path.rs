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
    // Stops at the end marker: a background process started by an rc file may hold stdout open for good.
    let reader = std::thread::spawn(move || {
        let mut out = Vec::new();
        let mut buf = [0; 4096];
        while let Ok(n @ 1..) = stdout.read(&mut buf) {
            out.extend_from_slice(&buf[..n]);
            if out.windows(END.len()).any(|w| w == END.as_bytes()) {
                break;
            }
        }
        let _ = tx.send(String::from_utf8_lossy(&out).into_owned());
    });
    let out = rx.recv_timeout(timeout);
    // SIGKILL makes the wait return at once, whether the shell already exited or still runs.
    let _ = child.kill();
    let _ = child.wait();
    let out = out.ok()?;
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

    fn fake_shell(dir: &Path, body: &str) -> std::path::PathBuf {
        use std::os::unix::fs::PermissionsExt;
        let path = dir.join("shell");
        std::fs::write(&path, format!("#!/bin/sh\n{body}")).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        path
    }

    #[test]
    fn probe_stops_at_the_end_marker_despite_a_background_child() {
        let dir = tempfile::tempdir().unwrap();
        let shell = fake_shell(
            dir.path(),
            "sleep 10 &\nprintf '\\n__ASTERISM_PATH_START__/x:/y__ASTERISM_PATH_END__\\n'\n",
        );
        let started = std::time::Instant::now();
        assert_eq!(
            probe_login_path(&shell, Duration::from_secs(3)),
            Some("/x:/y".into())
        );
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn probe_without_marker_times_out() {
        let dir = tempfile::tempdir().unwrap();
        let shell = fake_shell(dir.path(), "echo noise\nexec sleep 10\n");
        let started = std::time::Instant::now();
        assert_eq!(probe_login_path(&shell, Duration::from_millis(300)), None);
        assert!(started.elapsed() < Duration::from_secs(2));
    }

    #[test]
    fn merge_keeps_login_first_and_dedups() {
        assert_eq!(merge_path("/a:/b", "/b:/c"), "/a:/b:/c");
    }
}
