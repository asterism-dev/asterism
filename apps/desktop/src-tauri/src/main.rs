use std::sync::Arc;

use asterism_node::{CallError, LocalNode, LocalNodeConfig, NodeSink, NodeStatus, PathEnv};
use asterism_proto::paths::Paths;
use asterism_proto::types::{method, Event, PluginUiFile, SessionAttachResult};
use base64::{engine::general_purpose::STANDARD as BASE64, Engine as _};
use serde_json::Value;
use tauri::ipc::Channel;
#[cfg(target_os = "macos")]
use tauri::menu::{AboutMetadata, Menu, MenuItem, PredefinedMenuItem, Submenu};
#[cfg(target_os = "macos")]
use tauri::Wry;
use tauri::{AppHandle, Emitter, Manager, State, WindowEvent};

const QUIT_MENU_ID: &str = "quit";

const PANEL_CSP: &str = "default-src 'self' asterism-plugin: http://asterism-plugin.localhost; \
    style-src 'self' 'unsafe-inline' asterism-plugin: http://asterism-plugin.localhost; \
    img-src 'self' data: asterism-plugin: http://asterism-plugin.localhost; connect-src 'none'";

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
async fn node_call(
    node: State<'_, Arc<LocalNode>>,
    method: String,
    params: Value,
) -> Result<Value, CallError> {
    node.call(&method, params).await
}

#[tauri::command]
async fn session_attach(
    node: State<'_, Arc<LocalNode>>,
    session_id: i64,
    on_output: Channel<String>,
) -> Result<SessionAttachResult, CallError> {
    node.attach(
        session_id,
        Box::new(move |data| {
            let _ = on_output.send(data);
        }),
    )
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
async fn quit(
    app: AppHandle,
    node: State<'_, Arc<LocalNode>>,
    stop_daemon: bool,
) -> Result<(), ()> {
    if stop_daemon {
        // An unreachable daemon is as good as stopped.
        let _ = node
            .call(asterism_proto::types::method::SHUTDOWN, Value::Null)
            .await;
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
            &MenuItem::with_id(
                app,
                QUIT_MENU_ID,
                "Quit asterism",
                true,
                Some("CmdOrCtrl+Q"),
            )?,
        ],
    )?;
    menu.remove_at(0)?;
    menu.prepend(&app_submenu)?;
    Ok(menu)
}

// ponytail: percent-encoded paths are not decoded; plugin names are slugs
fn split_ui_path(path: &str) -> Option<(&str, &str)> {
    let (plugin, file) = path.trim_start_matches('/').split_once('/')?;
    (!plugin.is_empty() && !file.is_empty()).then_some((plugin, file))
}

async fn serve_ui_file(node: Arc<LocalNode>, path: String) -> tauri::http::Response<Vec<u8>> {
    let not_found = || {
        tauri::http::Response::builder()
            .status(404)
            .body(Vec::new())
            .unwrap_or_default()
    };
    let Some((plugin, file)) = split_ui_path(&path) else {
        return not_found();
    };
    let Ok(value) = node
        .call(
            method::PLUGIN_UI_FILE,
            serde_json::json!({ "plugin": plugin, "path": file }),
        )
        .await
    else {
        return not_found();
    };
    let Ok(ui) = serde_json::from_value::<PluginUiFile>(value) else {
        return not_found();
    };
    let Ok(body) = BASE64.decode(ui.data) else {
        return not_found();
    };
    tauri::http::Response::builder()
        .header("Content-Type", ui.mime)
        .header("Content-Security-Policy", PANEL_CSP)
        .header("Access-Control-Allow-Origin", "*")
        .body(body)
        .unwrap_or_default()
}

fn main() {
    let builder = tauri::Builder::default();
    #[cfg(target_os = "macos")]
    let builder = builder.menu(app_menu);
    let result = builder
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_notification::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
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
        .register_asynchronous_uri_scheme_protocol("asterism-plugin", |ctx, request, responder| {
            let node = ctx.app_handle().state::<Arc<LocalNode>>().inner().clone();
            let path = request.uri().path().to_string();
            tauri::async_runtime::spawn(async move {
                responder.respond(serve_ui_file(node, path).await);
            });
        })
        .invoke_handler(tauri::generate_handler![
            node_status,
            node_call,
            session_attach,
            session_detach,
            restart_daemon,
            quit,
            app_pid
        ])
        .run(tauri::generate_context!());
    if let Err(e) = result {
        eprintln!("asterism: {e}");
        std::process::exit(1);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn splits_plugin_and_path_from_the_url_path() {
        assert_eq!(
            split_ui_path("/agents/ui/agents.html"),
            Some(("agents", "ui/agents.html"))
        );
        assert_eq!(split_ui_path("/agents"), None);
        assert_eq!(split_ui_path("/agents/"), None);
        assert_eq!(split_ui_path(""), None);
    }
}
