mod common;

use asterism_core::daemon::Daemon;
use asterism_core::paths::Paths;
use asterism_plugin::protocol::LaunchMode;
use asterism_proto::types::*;
use common::init_repo;

fn daemon(home: &std::path::Path) -> std::sync::Arc<Daemon> {
    let paths = Paths { home: home.join("h") };
    paths.ensure_dirs().unwrap();
    common::link_fixture(&paths);
    common::set_fixture_token(&paths);
    Daemon::with_options(paths, common::daemon_options()).unwrap()
}

#[tokio::test]
async fn claude_argv_comes_from_the_claude_plugin() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path());
    let (argv, _env) = daemon.agent_argv("claude", LaunchMode::Start, Some("fix it"), None).await.unwrap().unwrap();
    let settings = home.path().join("h/plugins/data/claude/claude-settings.json");
    assert_eq!(argv[..3], ["claude".to_string(), "--settings".into(), settings.display().to_string()]);
    assert_eq!(argv[argv.len() - 2..], ["--".to_string(), "fix it".into()]);
    let written = std::fs::read_to_string(&settings).unwrap();
    assert!(written.contains("asterism-plugin-claude") && written.contains("hook stop"), "{written}");
    assert_eq!(daemon.agent_argv("claude", LaunchMode::Resume, None, None).await.unwrap(), None);
}

#[tokio::test]
async fn agents_are_listed_from_plugins() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path());
    let agents = daemon.agent_infos();
    let claude = agents.iter().find(|a| a.name == "claude").unwrap();
    assert_eq!((claude.display_name.as_str(), claude.plugin.as_str()), ("Claude Code", "claude"));
    assert_eq!(claude.settings, [AgentSettingKind::Args, AgentSettingKind::Mcp, AgentSettingKind::Hooks]);
    let echo = agents.iter().find(|a| a.name == "echo-agent").unwrap();
    assert!(echo.available, "sh is on PATH");
    assert_eq!(echo.settings, [AgentSettingKind::Args]);
}

#[tokio::test]
async fn static_agents_start_from_their_template_with_configured_args() {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let daemon = daemon(home.path());
    let config = AgentConfig { args: vec!["hello".into()], ..Default::default() };
    daemon.set_agent_config("echo-agent", &config).unwrap();
    let project = daemon.add_project(&repo.path().display().to_string()).unwrap();
    let params = TaskCreateParams { project_id: project.id, title: "t".into(), prompt: None, agent: Some("echo-agent".into()) };
    let session = daemon.create_task(params).await.unwrap().session.unwrap();
    let mut text = String::new();
    for _ in 0..100 {
        text = daemon.read(SessionReadParams { session_id: session.id, lines: 5 }).unwrap().text;
        if text.contains("started hello") {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
    }
    assert!(text.contains("started hello"), "{text}");
    daemon.kill_session(session.id).unwrap();

    let err = daemon.set_agent_config("echo-agent", &AgentConfig { mcp: Some(serde_json::json!({})), ..Default::default() }).unwrap_err();
    assert_eq!(err.kind, asterism_proto::rpc::ErrorKind::InvalidParams);
}

#[tokio::test]
async fn claude_resume_argv_ends_with_the_agent_ref() {
    let home = tempfile::tempdir().unwrap();
    let daemon = daemon(home.path());
    let (argv, _env) = daemon.agent_argv("claude", LaunchMode::Resume, None, Some("abc")).await.unwrap().unwrap();
    assert_eq!(argv[argv.len() - 2..], ["--resume".to_string(), "abc".into()]);
}

#[tokio::test]
async fn sessions_of_a_removed_agent_plugin_are_marked_exited_on_recover() {
    let home = tempfile::tempdir().unwrap();
    let repo = tempfile::tempdir().unwrap();
    init_repo(repo.path());
    let first = daemon(home.path());
    let project = first.add_project(&repo.path().display().to_string()).unwrap();
    let params = TaskCreateParams { project_id: project.id, title: "t".into(), prompt: None, agent: Some("echo-agent".into()) };
    let session = first.create_task(params).await.unwrap().session.unwrap();

    let paths = Paths { home: home.path().join("h") };
    std::fs::remove_file(paths.plugin_links()).unwrap();
    let second = Daemon::with_options(paths, common::daemon_options()).unwrap();
    second.recover().await.unwrap();
    assert_eq!(second.session(session.id).unwrap().status, SessionStatus::Exited);
    assert!(second.read(SessionReadParams { session_id: session.id, lines: 5 }).is_ok());
    first.kill_session(session.id).unwrap();
}
