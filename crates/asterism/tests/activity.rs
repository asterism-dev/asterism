mod common;

use asterism_core::daemon::Daemon;
use asterism_core::paths::Paths;
use asterism_proto::types::*;

fn daemon() -> (tempfile::TempDir, std::sync::Arc<Daemon>) {
    let home = tempfile::tempdir().unwrap();
    let daemon = Daemon::with_options(
        Paths {
            home: home.path().to_path_buf(),
        },
        common::daemon_options(),
    )
    .unwrap();
    (home, daemon)
}

fn start(daemon: &Daemon, steps: &[&str]) -> i64 {
    daemon.start_activity(ActivityKind::Clone, "t".into(), None, steps)
}

#[test]
fn activity_steps_advance_and_finish() {
    let (_home, d) = daemon();
    let id = start(&d, &["a", "b"]);
    d.advance_activity(id, None);
    d.advance_activity(id, Some("ok".into()));
    let a = &d.activities()[0];
    assert_eq!(a.status, ActivityStatus::Done);
    assert!(a.steps.iter().all(|s| s.status == ActivityStatus::Done));
    assert_eq!(a.steps[1].detail.as_deref(), Some("ok"));
    assert!(a.finished_at.is_some());
}

#[test]
fn fatal_failure_marks_activity_failed() {
    let (_home, d) = daemon();
    let id = start(&d, &["a", "b"]);
    d.fail_activity_step(id, "boom".into(), true);
    let a = &d.activities()[0];
    assert_eq!(a.status, ActivityStatus::Failed);
    assert_eq!(a.steps[0].status, ActivityStatus::Failed);
    assert_eq!(a.steps[0].detail.as_deref(), Some("boom"));
    assert_eq!(a.steps[1].status, ActivityStatus::Pending);
    assert!(a.finished_at.is_some());
}

#[test]
fn log_keeps_newest_fifty() {
    let (_home, d) = daemon();
    let mut last = 0;
    for _ in 0..51 {
        last = start(&d, &["a"]);
    }
    assert_eq!(d.activities().len(), 50);
    assert_eq!(d.activities()[0].id, last);
}

#[test]
fn changes_are_broadcast() {
    let (_home, d) = daemon();
    let mut rx = d.subscribe();
    let id = start(&d, &["a"]);
    assert!(matches!(rx.try_recv(), Ok(Event::ActivityChanged(a)) if a.id == id));
}
