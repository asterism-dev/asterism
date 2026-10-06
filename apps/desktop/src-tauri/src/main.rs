use std::sync::Arc;

use asterism_node::{CallError, LocalNode, LocalNodeConfig, NodeSink, NodeStatus, PathEnv};
use asterism_proto::paths::Paths;
use asterism_proto::types::{Event, SessionAttachResult};
use serde_json::Value;
use tauri::ipc::Channel;
#[cfg(target_os = "macos")]
use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
#[cfg(target_os = "macos")]
use tauri::Wry;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

const QUIT_MENU_ID: &str = "quit";

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

#[tauri::command]
async fn quit(app: AppHandle, node: State<'_, Arc<LocalNode>>, stop_daemon: bool) -> Result<(), ()> {
    if stop_daemon {
        // An unreachable daemon is as good as stopped.
        let _ = node.call(asterism_proto::types::method::SHUTDOWN, Value::Null).await;
    }
    app.exit(0);
    Ok(())
}

#[tauri::command]
fn app_pid() -> u32 {
    std::process::id()
}

/// The default macOS Quit item terminates the app outright, so ⌘Q gets an item that asks first.
#[cfg(target_os = "macos")]
fn app_menu(app: &AppHandle) -> tauri::Result<Menu<Wry>> {
    let menu = Menu::default(app)?;
    let app_submenu = Submenu::with_items(
        app,
        "asterism",
        true,
        &[
            &PredefinedMenuItem::about(app, None, Some(AboutMetadata::default()))?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::services(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &PredefinedMenuItem::hide(app, None)?,
            &PredefinedMenuItem::hide_others(app, None)?,
            &PredefinedMenuItem::show_all(app, None)?,
            &PredefinedMenuItem::separator(app)?,
            &MenuItem::with_id(app, QUIT_MENU_ID, "Quit asterism", true, Some("CmdOrCtrl+Q"))?,
        ],
    )?;
    menu.remove_at(0)?;
    menu.prepend(&app_submenu)?;
    Ok(menu)
}

fn main() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu);
    let result = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let config = LocalNodeConfig {
                paths: Paths::from_env(),
                // Tauri places externalBin sidecars next to the app executable.
                daemon_bin: std::env::current_exe()?.with_file_name("asterismd"),
                path_env: PathEnv::LoginShell,
                bundled_version: env!("CARGO_PKG_VERSION").into(),
                bundled_build: asterism_proto::BUILD_ID.into(),
            };
            let node = LocalNode::new(config, Arc::new(TauriSink(app.handle().clone())));
            app.manage(node.clone());
            tauri::async_runtime::spawn(node.run());
            Ok(())
        })
        // Closing and ⌘Q ask the frontend whether to stop the daemon; it then calls `quit`.
        .on_window_event(|window, event| {
            if let WindowEvent::CloseRequested { api, .. } = event {
                api.prevent_close();
                let _ = window.emit("quit-requested", ());
            }
        })
        .on_menu_event(|app, event| {
            if event.id() == QUIT_MENU_ID {
                let _ = app.emit("quit-requested", ());
            }
        })
        .invoke_handler(tauri::generate_handler![node_status, node_call, session_attach, session_detach, restart_daemon, quit, app_pid])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("asterism: {e}");
        std::process::exit(1);
    }
}
