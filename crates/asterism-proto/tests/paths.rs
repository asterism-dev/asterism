use std::os::unix::fs::PermissionsExt;

use asterism_proto::paths::Paths;

#[test]
fn layout_lives_under_home() {
    let paths = Paths { home: "/h".into() };
    assert_eq!(paths.socket(), std::path::Path::new("/h/asterismd.sock"));
    assert_eq!(paths.config(), std::path::Path::new("/h/config.toml"));
    assert_eq!(paths.log(), std::path::Path::new("/h/asterismd.log"));
}

#[test]
fn ensure_dirs_makes_home_private() {
    let dir = tempfile::tempdir().unwrap();
    let home = dir.path().join("home");
    std::fs::create_dir(&home).unwrap();
    std::fs::set_permissions(&home, std::fs::Permissions::from_mode(0o755)).unwrap();
    Paths { home: home.clone() }.ensure_dirs().unwrap();
    assert_eq!(std::fs::metadata(&home).unwrap().permissions().mode() & 0o777, 0o700);
    assert!(home.join("worktrees").is_dir());
}

#[test]
fn agent_files_live_under_agents_dir() {
    let paths = Paths { home: "/h".into() };
    assert_eq!(paths.agent_mcp("claude"), std::path::Path::new("/h/agents/claude/mcp.json"));
    assert_eq!(paths.agent_hooks("claude"), std::path::Path::new("/h/agents/claude/hooks.json"));
}
