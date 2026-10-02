mod common;

use std::time::Duration;

use asterism_node::daemon::connect_or_spawn;
use asterism_node::login_env::extract_marked;
use asterism_proto::paths::Paths;
use common::{daemon_bin, stop_daemon};

#[test]
fn marked_path_is_extracted_from_noisy_shell_output() {
    let out = "Welcome!\n__ASTERISM_PATH__/opt/homebrew/bin:/usr/bin__ASTERISM_PATH__\nbye";
    assert_eq!(extract_marked(out).as_deref(), Some("/opt/homebrew/bin:/usr/bin"));
    assert_eq!(extract_marked("no markers here"), None);
    assert_eq!(extract_marked("__ASTERISM_PATH____ASTERISM_PATH__"), None);
}

#[test]
fn login_shell_path_includes_system_dirs() {
    if std::env::var_os("SHELL").is_none() {
        return;
    }
    let path = asterism_node::login_env::login_shell_path().expect("login shell PATH");
    assert!(path.split(':').any(|dir| dir == "/usr/bin"), "{path}");
}

#[tokio::test]
async fn spawns_the_daemon_once_and_reuses_it() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().join("h") };
    let first = connect_or_spawn(&paths, &daemon_bin(), None).await.unwrap();
    drop(first);
    assert!(paths.socket().exists());
    assert!(paths.log().exists());

    let started = std::time::Instant::now();
    let _second = connect_or_spawn(&paths, &daemon_bin(), None).await.unwrap();
    assert!(started.elapsed() < Duration::from_millis(200), "second connect should not spawn");
    stop_daemon(&paths);
}

#[tokio::test]
async fn missing_daemon_binary_is_an_error() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().join("h") };
    assert!(connect_or_spawn(&paths, std::path::Path::new("/nonexistent/asterismd"), None).await.is_err());
}
