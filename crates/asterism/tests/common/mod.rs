#![allow(dead_code)]

use std::path::Path;
use std::process::Command;
use std::time::Duration;

pub fn run_git(dir: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(dir)
        .env("GIT_CONFIG_GLOBAL", "/dev/null")
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .args(["-c", "user.name=test", "-c", "user.email=test@example.com"])
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
    String::from_utf8_lossy(&out.stdout).into_owned()
}

pub fn init_repo(dir: &Path) {
    run_git(dir, &["init", "-q", "-b", "main"]);
    std::fs::write(dir.join("README.md"), "hello\n").unwrap();
    run_git(dir, &["add", "."]);
    run_git(dir, &["commit", "-q", "-m", "init"]);
}

/// Polls `f` for up to 5 seconds.
pub async fn eventually(mut f: impl FnMut() -> bool) -> bool {
    for _ in 0..100 {
        if f() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(50)).await;
    }
    false
}

use std::process::Output;

use tempfile::TempDir;

/// An isolated asterism home plus a git repo, driven through the real CLI binary.
pub struct Node {
    home: TempDir,
    repo: TempDir,
}

impl Node {
    pub fn new() -> Self {
        let home = tempfile::tempdir().unwrap();
        let repo = tempfile::tempdir().unwrap();
        init_repo(repo.path());
        Self { home, repo }
    }

    pub fn repo(&self) -> &Path {
        self.repo.path()
    }

    pub fn home(&self) -> &Path {
        self.home.path()
    }

    pub fn command(&self, args: &[&str]) -> Command {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_asterism"));
        cmd.args(args)
            .env("ASTERISM_HOME", self.home.path())
            .env_remove("ASTERISM_SOCKET")
            .env_remove("ASTERISM_TASK")
            .env_remove("ASTERISM_SESSION")
            .current_dir(self.repo.path());
        cmd
    }

    pub fn cmd(&self, args: &[&str]) -> Output {
        self.command(args).output().unwrap()
    }

    pub fn json(&self, args: &[&str]) -> serde_json::Value {
        let out = self.cmd(&[&["--json"][..], args].concat());
        assert!(out.status.success(), "asterism {args:?} failed: {}", String::from_utf8_lossy(&out.stderr));
        serde_json::from_slice(&out.stdout).unwrap()
    }
}

impl Drop for Node {
    fn drop(&mut self) {
        let _ = self.cmd(&["daemon", "stop"]);
    }
}

use std::path::PathBuf;

use asterism_core::daemon::DaemonOptions;
use asterism_core::paths::Paths;

/// Where cargo puts this package's binaries, including the built-in plugin backends.
pub fn bin_dir() -> PathBuf {
    Path::new(env!("CARGO_BIN_EXE_asterism")).parent().unwrap().to_path_buf()
}

pub fn fixture_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/plugins/echo")
}

pub fn daemon_options() -> DaemonOptions {
    DaemonOptions { builtin_plugins_dir: Some(bin_dir()), ..Default::default() }
}

pub fn link_fixture(paths: &Paths) {
    std::fs::create_dir_all(paths.plugins_dir()).unwrap();
    std::fs::write(paths.plugin_links(), format!("[links]\necho = {:?}\n", fixture_dir().display().to_string())).unwrap();
}

pub fn set_fixture_token(paths: &Paths) {
    let manifest = asterism_core::plugins::manifest::parse(&std::fs::read_to_string(fixture_dir().join("plugin.toml")).unwrap()).unwrap();
    let values = [("token".to_string(), serde_json::json!("t"))].into();
    asterism_core::plugins::settings::save(paths, "echo", &manifest.settings, &values).unwrap();
}
