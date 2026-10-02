use std::io;
use std::sync::{Mutex, MutexGuard, PoisonError};

use tokio::net::{UnixListener, UnixStream};

pub mod agents;
pub mod daemon;
pub mod error;
pub mod git;
pub mod paths;
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
    daemon.recover().map_err(io::Error::other)?;
    tokio::select! {
        _ = rpc::serve(daemon.clone(), listener) => {}
        _ = daemon.shutdown_requested() => {}
    }
    let _ = std::fs::remove_file(&socket);
    Ok(())
}
