use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Duration;

use asterism_proto::types::{HookEvent, SessionStatus};
use tokio::sync::broadcast::error::RecvError;
use tokio::sync::watch;
use tokio::time::{sleep_until, Instant};

use crate::session::Pty;

pub const IDLE_AFTER: Duration = Duration::from_secs(2);

pub fn idle_status(screen: &str, waiting_patterns: &[&str]) -> SessionStatus {
    if waiting_patterns.iter().any(|p| screen.contains(p)) {
        SessionStatus::WaitingInput
    } else {
        SessionStatus::Idle
    }
}

pub fn hook_status(event: HookEvent) -> SessionStatus {
    match event {
        HookEvent::PromptSubmit | HookEvent::Tool => SessionStatus::Working,
        HookEvent::Stop => SessionStatus::Idle,
        HookEvent::Notification => SessionStatus::WaitingInput,
    }
}

/// Derives status from PTY activity; once hooks report for a session, only the exit transition is applied here.
pub async fn track(
    pty: Arc<Pty>,
    waiting_patterns: &'static [&'static str],
    status: Arc<watch::Sender<SessionStatus>>,
    hooks_active: Arc<AtomicBool>,
) {
    let set = |next: SessionStatus| {
        status.send_if_modified(|current| {
            let changed = *current != next;
            *current = next;
            changed
        });
    };
    let mut output = pty.subscribe();
    let mut exited = pty.exited();
    let mut idle_at = Some(Instant::now() + IDLE_AFTER);
    loop {
        tokio::select! {
            received = output.recv() => {
                if matches!(received, Err(RecvError::Closed)) {
                    break;
                }
                if !hooks_active.load(Ordering::Relaxed) {
                    set(SessionStatus::Working);
                }
                idle_at = Some(Instant::now() + IDLE_AFTER);
            }
            _ = sleep_until(idle_at.unwrap_or_else(Instant::now)), if idle_at.is_some() => {
                idle_at = None;
                if !hooks_active.load(Ordering::Relaxed) {
                    set(idle_status(&pty.text(), waiting_patterns));
                }
            }
            _ = exited.wait_for(|done| *done) => {
                set(SessionStatus::Exited);
                break;
            }
        }
    }
}
