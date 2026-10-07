use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use asterism_core::session::{Pty, SpawnSpec};
use asterism_core::status::{hook_status, idle_status, track};
use asterism_proto::types::{HookEvent, SessionStatus};
use tokio::sync::watch;

fn claude_patterns() -> Vec<String> {
    vec!["Do you want to".into(), "❯ 1. Yes".into()]
}

fn start(argv: &[&str], patterns: Vec<String>, hooks: bool) -> watch::Receiver<SessionStatus> {
    let initial = if hooks {
        SessionStatus::Idle
    } else {
        SessionStatus::Working
    };
    start_in(argv, patterns, hooks, initial)
}

fn start_in(
    argv: &[&str],
    patterns: Vec<String>,
    hooks: bool,
    initial: SessionStatus,
) -> watch::Receiver<SessionStatus> {
    let pty = Pty::spawn(SpawnSpec {
        argv: argv.iter().map(|s| s.to_string()).collect(),
        cwd: std::env::temp_dir(),
        env: vec![("PATH".into(), std::env::var("PATH").unwrap())],
        rows: 24,
        cols: 80,
    })
    .unwrap();
    let (tx, rx) = watch::channel(initial);
    tokio::spawn(track(
        pty,
        Arc::from(patterns),
        Arc::new(tx),
        Arc::new(AtomicBool::new(hooks)),
    ));
    rx
}

async fn reaches(rx: &mut watch::Receiver<SessionStatus>, status: SessionStatus) {
    tokio::time::timeout(Duration::from_secs(6), rx.wait_for(|s| *s == status))
        .await
        .unwrap()
        .unwrap();
}

#[test]
fn idle_status_detects_waiting_prompts() {
    assert_eq!(
        idle_status("all done", &claude_patterns()),
        SessionStatus::Idle
    );
    assert_eq!(
        idle_status("Do you want to make this edit?", &claude_patterns()),
        SessionStatus::WaitingInput
    );
    assert_eq!(idle_status("Do you want to", &[]), SessionStatus::Idle);
}

#[test]
fn hook_events_map_to_statuses() {
    assert_eq!(
        hook_status(HookEvent::PromptSubmit),
        Some(SessionStatus::Working)
    );
    assert_eq!(hook_status(HookEvent::Tool), Some(SessionStatus::Working));
    assert_eq!(hook_status(HookEvent::Stop), None);
    assert_eq!(
        hook_status(HookEvent::Notification),
        Some(SessionStatus::WaitingInput)
    );
}

#[tokio::test]
async fn silence_means_idle() {
    let mut rx = start(&["sh", "-c", "echo hi; sleep 30"], vec![], false);
    reaches(&mut rx, SessionStatus::Idle).await;
}

#[tokio::test]
async fn waiting_prompt_on_screen_means_waiting_input() {
    let mut rx = start(
        &["sh", "-c", "echo 'Do you want to proceed?'; sleep 30"],
        claude_patterns(),
        false,
    );
    reaches(&mut rx, SessionStatus::WaitingInput).await;
}

#[tokio::test]
async fn process_exit_means_exited() {
    let mut rx = start(&["sh", "-c", "exit 3"], vec![], false);
    reaches(&mut rx, SessionStatus::Exited).await;
}

#[tokio::test]
async fn hook_driven_sessions_ignore_output_but_not_exit() {
    let mut rx = start(
        &["sh", "-c", "sleep 0.3; echo burst; sleep 1; exit 0"],
        vec![],
        true,
    );
    tokio::time::sleep(Duration::from_millis(800)).await;
    assert_eq!(*rx.borrow(), SessionStatus::Idle);
    reaches(&mut rx, SessionStatus::Exited).await;
}

#[tokio::test]
async fn hook_driven_work_ends_only_after_output_goes_quiet() {
    let mut rx = start_in(
        &[
            "sh",
            "-c",
            "for i in 1 2 3 4 5 6; do echo spin; sleep 0.5; done; sleep 30",
        ],
        vec![],
        true,
        SessionStatus::Working,
    );
    tokio::time::sleep(Duration::from_millis(2500)).await;
    assert_eq!(*rx.borrow(), SessionStatus::Working);
    reaches(&mut rx, SessionStatus::Idle).await;
}

#[tokio::test]
async fn silence_keeps_hook_driven_waiting_input() {
    let rx = start_in(
        &["sh", "-c", "echo hi; sleep 30"],
        vec![],
        true,
        SessionStatus::WaitingInput,
    );
    tokio::time::sleep(Duration::from_millis(2800)).await;
    assert_eq!(*rx.borrow(), SessionStatus::WaitingInput);
}
