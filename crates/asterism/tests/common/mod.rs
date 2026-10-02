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
