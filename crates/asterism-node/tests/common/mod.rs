#![allow(dead_code)]

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::process::Command;

use asterism_proto::paths::Paths;

/// The daemon built by `cargo build`; these tests do not build it themselves.
pub fn daemon_bin() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| Path::new(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let bin = target.join("debug/asterismd");
    assert!(
        bin.exists(),
        "{} missing — run `cargo build` before these tests",
        bin.display()
    );
    bin
}

pub fn init_repo(dir: &Path) {
    for args in [
        &["init", "-q", "-b", "main"][..],
        &[
            "-c",
            "user.name=t",
            "-c",
            "user.email=t@example.com",
            "commit",
            "-q",
            "--allow-empty",
            "-m",
            "init",
        ][..],
    ] {
        let status = Command::new("git")
            .arg("-C")
            .arg(dir)
            .args(args)
            .env("GIT_CONFIG_GLOBAL", "/dev/null")
            .env("GIT_CONFIG_NOSYSTEM", "1")
            .status()
            .unwrap();
        assert!(status.success());
    }
}

/// Sends a raw `shutdown` so tests never leave daemons behind.
pub fn stop_daemon(paths: &Paths) {
    let Ok(mut stream) = UnixStream::connect(paths.socket()) else {
        return;
    };
    let _ = stream
        .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"shutdown\",\"params\":null}\n");
    let _ = BufReader::new(stream).read_line(&mut String::new());
}

use std::sync::{Arc, Mutex};
use std::time::Duration;

use asterism_node::{NodeSink, NodeStatus};
use asterism_proto::types::Event;

#[derive(Default)]
pub struct Recorder {
    pub statuses: Mutex<Vec<NodeStatus>>,
    pub events: Mutex<Vec<Event>>,
}

impl NodeSink for Recorder {
    fn status(&self, status: &NodeStatus) {
        self.statuses.lock().unwrap().push(status.clone());
    }
    fn event(&self, event: &Event) {
        self.events.lock().unwrap().push(event.clone());
    }
}

impl Recorder {
    /// Waits up to 15 s for a status matching `f` and returns it.
    pub async fn wait_status(&self, f: impl Fn(&NodeStatus) -> bool) -> NodeStatus {
        for _ in 0..300 {
            if let Some(found) = self
                .statuses
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|s| f(s))
                .cloned()
            {
                return found;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!(
            "status not reached; saw {:?}",
            self.statuses.lock().unwrap()
        );
    }

    pub fn clear(&self) {
        self.statuses.lock().unwrap().clear();
    }
}

pub fn shared<T>(value: T) -> Arc<Mutex<T>> {
    Arc::new(Mutex::new(value))
}
