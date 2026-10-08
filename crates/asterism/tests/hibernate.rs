mod common;

use std::sync::Arc;
use std::time::Duration;

use asterism_core::daemon::{Daemon, DaemonOptions};
use asterism_core::paths::Paths;
use asterism_proto::types::*;
use common::{eventually, init_repo};

struct Fixture {
    home: tempfile::TempDir,
    _repo: tempfile::TempDir,
    daemon: Arc<Daemon>,
    session: i64,
}

fn options() -> DaemonOptions {
    DaemonOptions {
        hibernate_minute: Duration::from_millis(10),
        ..common::daemon_options()
    }
}

fn open(home: &std::path::Path) -> Arc<Daemon> {
    let paths = Paths {
        home: home.join("h"),
    };
    Daemon::with_options(paths, options()).unwrap()
}

/// Starts an echo-agent session that reported agent ref "r1" and has gone idle.
async fn idle_session() -> Fixture {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let paths = Paths {
        home: home.path().join("h"),
    };
    paths.ensure_dirs().unwrap();
    common::link_fixture(&paths);
    common::set_fixture_token(&paths);
    let daemon = open(home.path());
    let config = AgentConfig {
        hibernate_after_min: Some(1),
        ..Default::default()
    };
    daemon.set_agent_config("echo-agent", &config).unwrap();
    let project = daemon
        .add_project(&repo.path().display().to_string())
        .unwrap();
    let params = TaskCreateParams {
        project_id: project.id,
        title: "t".into(),
        agent: Some("echo-agent".into()),
        ..Default::default()
    };
    let session = daemon
        .create_task(params)
        .await
        .unwrap()
        .session
        .unwrap()
        .id;
    daemon
        .hook(SessionHookParams {
            session_id: session,
            event: HookEvent::Stop,
            agent_ref: Some("r1".into()),
        })
        .unwrap();
    wait_status(&daemon, session, SessionStatus::Idle).await;
    Fixture {
        home,
        _repo: repo,
        daemon,
        session,
    }
}

async fn wait_status(daemon: &Daemon, session: i64, status: SessionStatus) {
    assert!(
        eventually(|| daemon.session(session).unwrap().status == status).await,
        "session never became {status:?}, is {:?}",
        daemon.session(session).unwrap().status
    );
}

fn screen(daemon: &Daemon, session: i64) -> String {
    daemon
        .read(SessionReadParams {
            session_id: session,
            lines: 20,
        })
        .unwrap()
        .text
}

fn running(daemon: &Daemon) -> usize {
    daemon.stats(NodeStatsParams::default()).sessions.len()
}

#[tokio::test]
async fn idle_unattached_session_hibernates_and_stays_asleep_across_restart() {
    let f = idle_session().await;
    tokio::time::sleep(Duration::from_millis(20)).await;
    f.daemon.hibernate_idle().unwrap();
    wait_status(&f.daemon, f.session, SessionStatus::Hibernated).await;
    assert_eq!(running(&f.daemon), 0);
    assert!(screen(&f.daemon, f.session).contains("started"));

    let restarted = open(f.home.path());
    restarted.recover().await.unwrap();
    assert_eq!(
        restarted.session(f.session).unwrap().status,
        SessionStatus::Hibernated
    );
    assert_eq!(running(&restarted), 0);
}

#[tokio::test]
async fn attached_session_is_not_hibernated() {
    let f = idle_session().await;
    let _attached = f.daemon.attach(f.session).unwrap();
    tokio::time::sleep(Duration::from_millis(20)).await;
    f.daemon.hibernate_idle().unwrap();
    tokio::time::sleep(Duration::from_millis(200)).await;
    assert_eq!(
        f.daemon.session(f.session).unwrap().status,
        SessionStatus::Idle
    );
}
