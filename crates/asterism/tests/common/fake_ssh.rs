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
HOME={home} exec sh -c "$*"
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
