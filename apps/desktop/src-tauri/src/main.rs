use std::sync::Arc;

use asterism_node::{login_env, CallError, LocalNode, LocalNodeConfig, NodeSink, NodeStatus};
use asterism_proto::paths::Paths;
use asterism_proto::types::{Event, SessionAttachResult};
use serde_json::Value;
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

struct TauriSink(AppHandle);

impl NodeSink for TauriSink {
    fn status(&self, status: &NodeStatus) {
        let _ = self.0.emit("node-status", status);
    }

    fn event(&self, event: &Event) {
        let _ = self.0.emit("node-event", event);
    }
}

#[tauri::command]
fn node_status(node: State<'_, Arc<LocalNode>>) -> NodeStatus {
    node.status()
}

#[tauri::command]
async fn node_call(node: State<'_, Arc<LocalNode>>, method: String, params: Value) -> Result<Value, CallError> {
    node.call(&method, params).await
}

#[tauri::command]
async fn session_attach(
    node: State<'_, Arc<LocalNode>>,
    session_id: i64,
    on_output: Channel<String>,
) -> Result<SessionAttachResult, CallError> {
    node.attach(session_id, Box::new(move |data| {
        let _ = on_output.send(data);
    }))
    .await
}

#[tauri::command]
async fn session_detach(node: State<'_, Arc<LocalNode>>, session_id: i64) -> Result<(), CallError> {
    node.detach(session_id).await
}

#[tauri::command]
async fn restart_daemon(node: State<'_, Arc<LocalNode>>) -> Result<(), CallError> {
    node.restart_daemon().await
}

fn main() {
    let result = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config = LocalNodeConfig {
                paths: Paths::from_env(),
                // Tauri places externalBin sidecars next to the app executable.
                daemon_bin: std::env::current_exe()?.with_file_name("asterismd"),
                path_env: login_env::login_shell_path(),
                bundled_version: env!("CARGO_PKG_VERSION").into(),
            };
            let node = LocalNode::new(config, Arc::new(TauriSink(app.handle().clone())));
            app.manage(node.clone());
            tauri::async_runtime::spawn(node.run());
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![node_status, node_call, session_attach, session_detach, restart_daemon])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("asterism: {e}");
        std::process::exit(1);
    }
}
