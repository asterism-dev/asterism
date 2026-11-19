use std::io::{Read, Write};
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use asterism_core::machines::install::Ssh;
use tempfile::TempDir;

/// A stand-in `ssh` that runs the remote command locally with `HOME` set to a temp dir.
pub struct FakeSsh {
    // Short and under /tmp: Task 4 binds a unix socket inside home (104-byte limit on macOS).
    root: TempDir,
    home: PathBuf,
}

impl FakeSsh {
    pub fn new() -> Self {
        let root = tempfile::Builder::new()
            .prefix("as ")
            .tempdir_in("/tmp")
            .unwrap();
        let home = root.path().join("home");
        std::fs::create_dir(&home).unwrap();
        let script = format!(
            r#"#!/bin/sh
root={root}
echo >> "$root/calls"
if [ -f "$root/hang" ]; then
  exec sleep 86400
fi
if [ -f "$root/fail" ]; then
  {{ read -r code; read -r line; }} < "$root/fail"
  echo "$line" >&2
  exit "$code"
fi
while [ $# -gt 0 ]; do
  case "$1" in
    -T) shift ;;
    -o) shift 2 ;;
    *) break ;;
  esac
done
shift
# Like real ssh, the remote side gets none of our environment (e.g. ASTERISM_SOCKET).
exec env -i HOME={home} PATH="$PATH" sh -c "$*"
"#,
            root = sh_quote(root.path()),
            home = sh_quote(&home),
        );
        let bin = root.path().join("ssh");
        std::fs::write(&bin, script).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        let bins = root.path().join("bins").join(host_platform());
        std::fs::create_dir_all(&bins).unwrap();
        symlink(env!("CARGO_BIN_EXE_asterismd"), bins.join("asterismd")).unwrap();
        symlink(env!("CARGO_BIN_EXE_asterism"), bins.join("asterism")).unwrap();
        Self { root, home }
    }

    pub fn bin(&self) -> PathBuf {
        self.root.path().join("ssh")
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    pub fn bins_dir(&self) -> PathBuf {
        self.root.path().join("bins")
    }

    pub fn ssh(&self, destination: &str) -> Ssh {
        Ssh::new(self.bin(), destination.to_string())
    }

    /// Every later invocation prints `stderr_line` and exits with `code`.
    pub fn fail_with(&self, stderr_line: &str, code: i32) {
        std::fs::write(
            self.root.path().join("fail"),
            format!("{code}\n{stderr_line}\n"),
        )
        .unwrap();
    }

    pub fn recover(&self) {
        let _ = std::fs::remove_file(self.root.path().join("fail"));
    }

    /// Every later invocation sleeps forever, like an unreachable host.
    pub fn hang(&self) {
        std::fs::write(self.root.path().join("hang"), "").unwrap();
    }

    pub fn invocations(&self) -> usize {
        std::fs::read_to_string(self.root.path().join("calls"))
            .map_or(0, |calls| calls.lines().count())
    }

    pub fn machine_home(&self) -> PathBuf {
        self.home.join(".asterism-machine")
    }

    /// Stops the `asterism attach` relays of open connections, like a link that went silent.
    pub fn pause_link(&self) {
        self.signal_link("-STOP");
    }

    pub fn resume_link(&self) {
        self.signal_link("-CONT");
    }

    fn signal_link(&self, signal: &str) {
        let pattern = format!("{}/bin/.*/asterism attach", self.machine_home().display());
        std::process::Command::new("pkill")
            .args([signal, "-f", &pattern])
            .status()
            .unwrap();
    }

    /// Sends `shutdown` to the machine daemon, if one is listening.
    pub fn stop_daemon(&self) {
        let socket = self.machine_home().join("asterismd.sock");
        if let Ok(mut stream) = std::os::unix::net::UnixStream::connect(socket) {
            let _ = stream.write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"shutdown\"}\n");
            let _ = stream.read(&mut [0; 512]);
        }
    }
}

impl Drop for FakeSsh {
    fn drop(&mut self) {
        // The machine daemon runs detached and would outlive the test.
        self.stop_daemon();
    }
}

pub fn host_platform() -> String {
    let os = match std::env::consts::OS {
        "macos" => "darwin",
        os => os,
    };
    format!("{os}-{}", std::env::consts::ARCH)
}

fn sh_quote(path: &Path) -> String {
    format!("'{}'", path.display().to_string().replace('\'', r"'\''"))
}
