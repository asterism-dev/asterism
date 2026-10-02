mod common;

use asterism_node::daemon::connect_or_spawn;
use asterism_node::login_env::{extract_marked, fallback_path};
use asterism_proto::paths::Paths;
use asterism_proto::PROTO_VERSION;
use common::{daemon_bin, stop_daemon};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::UnixStream;

struct StopOnDrop(Paths);

impl Drop for StopOnDrop {
    fn drop(&mut self) {
        stop_daemon(&self.0);
    }
}

async fn hello_pid(stream: UnixStream) -> u64 {
    let (reader, mut writer) = stream.into_split();
    let hello = format!(
        "{{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"hello\",\"params\":{{\"proto_version\":{PROTO_VERSION},\"client_kind\":\"cli\"}}}}\n"
    );
    writer.write_all(hello.as_bytes()).await.unwrap();
    let mut line = String::new();
    BufReader::new(reader).read_line(&mut line).await.unwrap();
    let reply: serde_json::Value = serde_json::from_str(&line).unwrap();
    reply["result"]["pid"].as_u64().unwrap_or_else(|| panic!("no pid in {line}"))
}

#[test]
fn marked_path_is_extracted_from_noisy_shell_output() {
    let out = "Welcome!\n__ASTERISM_PATH__/opt/homebrew/bin:/usr/bin__ASTERISM_PATH__\nbye";
    assert_eq!(extract_marked(out).as_deref(), Some("/opt/homebrew/bin:/usr/bin"));
    assert_eq!(extract_marked("no markers here"), None);
    assert_eq!(extract_marked("__ASTERISM_PATH____ASTERISM_PATH__"), None);
}

#[test]
fn fallback_path_appends_only_missing_dirs() {
    assert_eq!(
        fallback_path("/usr/bin:/opt/homebrew/bin", Some("/Users/u")),
        "/usr/bin:/opt/homebrew/bin:/usr/local/bin:/Users/u/.local/bin"
    );
    assert_eq!(fallback_path("", None), "/opt/homebrew/bin:/usr/local/bin");
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
    let _stop = StopOnDrop(paths.clone());
    let first = hello_pid(connect_or_spawn(&paths, &daemon_bin(), None).await.unwrap()).await;
    assert!(paths.socket().exists());
    assert!(paths.log().exists());

    let second = hello_pid(connect_or_spawn(&paths, &daemon_bin(), None).await.unwrap()).await;
    assert_eq!(first, second, "second connect should reuse the running daemon");
}

#[tokio::test]
async fn missing_daemon_binary_is_an_error() {
    let home = tempfile::tempdir().unwrap();
    let paths = Paths { home: home.path().join("h") };
    assert!(connect_or_spawn(&paths, std::path::Path::new("/nonexistent/asterismd"), None).await.is_err());
}
