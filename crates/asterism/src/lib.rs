use std::io;
use std::sync::{Mutex, MutexGuard, PoisonError};

use tokio::net::{UnixListener, UnixStream};

pub mod agent_settings;
pub mod agents;
pub mod config;
pub mod daemon;
pub mod error;
pub mod files;
pub mod git;
pub mod hibernate;
pub mod node_settings;
pub mod paths;
pub mod plugins;
pub mod pr_status;
pub mod proc_stats;
pub mod repo_source;
pub mod rpc;
pub mod session;
pub mod status;
pub mod store;

use daemon::Daemon;
use paths::Paths;

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

pub async fn run(paths: Paths) -> io::Result<()> {
    let socket = paths.socket();
    paths.ensure_dirs()?;
    // Held for the whole run: two daemons racing after a crash must not both bind and recover.
    let lock_file = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(paths.lock())?;
    if lock_file.try_lock().is_err() {
        return Err(io::Error::new(
            io::ErrorKind::AddrInUse,
            "another daemon is running",
        ));
    }
    if socket.exists() {
        if UnixStream::connect(&socket).await.is_ok() {
            return Err(io::Error::new(
                io::ErrorKind::AddrInUse,
                format!("a daemon is already listening on {}", socket.display()),
            ));
        }
        std::fs::remove_file(&socket)?;
    }
    let daemon = Daemon::new(paths).map_err(io::Error::other)?;
    let listener = UnixListener::bind(&socket)?;
    let recovering = daemon.clone();
    tokio::spawn(async move {
        if let Err(e) = recovering.recover().await {
            eprintln!("asterismd: session recovery failed: {e}");
        }
    });
    tokio::spawn(daemon.clone().store_refresh_loop());
    tokio::spawn(daemon.clone().pr_poll_loop());
    tokio::spawn(daemon.clone().hibernate_loop());
    tokio::select! {
        _ = rpc::serve(daemon.clone(), listener) => {}
        _ = daemon.shutdown_requested() => {}
    }
    let _ = std::fs::remove_file(&socket);
    Ok(())
}
