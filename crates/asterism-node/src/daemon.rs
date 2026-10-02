use std::fs::OpenOptions;
use std::io;
use std::os::unix::process::CommandExt;
use std::path::Path;
use std::process::{Command, Stdio};
use std::time::Duration;

use asterism_proto::paths::Paths;
use tokio::net::UnixStream;

const SPAWN_ATTEMPTS: u32 = 60;
const SPAWN_POLL: Duration = Duration::from_millis(50);

/// Connects to the node daemon, starting `daemon_bin` first when nothing listens on the socket.
pub async fn connect_or_spawn(paths: &Paths, daemon_bin: &Path, path_env: Option<&str>) -> io::Result<UnixStream> {
    let socket = paths.socket();
    if let Ok(stream) = UnixStream::connect(&socket).await {
        return Ok(stream);
    }
    spawn(paths, daemon_bin, path_env)?;
    for _ in 0..SPAWN_ATTEMPTS {
        tokio::time::sleep(SPAWN_POLL).await;
        if let Ok(stream) = UnixStream::connect(&socket).await {
            return Ok(stream);
        }
    }
    UnixStream::connect(&socket).await
}

pub fn spawn(paths: &Paths, daemon_bin: &Path, path_env: Option<&str>) -> io::Result<()> {
    paths.ensure_dirs()?;
    let log = OpenOptions::new().create(true).append(true).open(paths.log())?;
    let mut cmd = Command::new(daemon_bin);
    cmd.current_dir("/").env("ASTERISM_HOME", &paths.home);
    if let Some(path) = path_env {
        cmd.env("PATH", path);
    }
    // Own process group so quitting the app leaves the daemon and its sessions running.
    let mut child = cmd.stdin(Stdio::null()).stdout(Stdio::null()).stderr(log).process_group(0).spawn()?;
    // Reap the daemon if it exits while the app is still running.
    std::thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(())
}
