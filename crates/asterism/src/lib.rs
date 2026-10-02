use std::sync::{Mutex, MutexGuard, PoisonError};

pub mod agents;
pub mod error;
pub mod git;
pub mod paths;
pub mod session;
pub mod status;
pub mod store;

pub(crate) fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}
