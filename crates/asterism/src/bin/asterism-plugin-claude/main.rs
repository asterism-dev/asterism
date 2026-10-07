mod hook;
mod launch;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};

use asterism_plugin::protocol::{
    method, InitializeParams, InitializeResult, PrepareParams, SettingsChangedParams,
};
use asterism_plugin::{params, serve, to_value, ErrorKind, Host, RpcError};
use serde_json::{Map, Value};

#[derive(Default)]
struct State {
    data_dir: OnceLock<PathBuf>,
    trust_workspaces: AtomicBool,
}

impl State {
    fn apply(&self, settings: &Map<String, Value>) {
        let trust = settings
            .get("trust_workspaces")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        self.trust_workspaces.store(trust, Ordering::Relaxed);
    }
}

/// Where Claude Code keeps its global config, including per-folder trust.
fn claude_config() -> Option<PathBuf> {
    std::env::var_os("CLAUDE_CONFIG_DIR")
        .or_else(|| std::env::var_os("HOME"))
        .map(|dir| PathBuf::from(dir).join(".claude.json"))
}

fn handle(state: &State, method_name: &str, raw: Value) -> Result<Value, RpcError> {
    match method_name {
        method::INITIALIZE => {
            let p: InitializeParams = params(raw)?;
            let _ = state.data_dir.set(PathBuf::from(p.data_dir));
            state.apply(&p.settings);
            to_value(InitializeResult {
                capabilities: vec!["agent".into()],
            })
        }
        method::SETTINGS_CHANGED => {
            state.apply(&params::<SettingsChangedParams>(raw)?.settings);
            Ok(Value::Null)
        }
        method::AGENT_PREPARE => {
            let p: PrepareParams = params(raw)?;
            // A failed trust write only means Claude asks itself, so the launch goes ahead.
            if state.trust_workspaces.load(Ordering::Relaxed) {
                match claude_config() {
                    Some(config) => {
                        if let Err(e) = launch::trust_workspace(&config, &p.cwd) {
                            eprintln!("asterism-plugin-claude: could not trust {}: {e}", p.cwd);
                        }
                    }
                    None => eprintln!(
                        "asterism-plugin-claude: HOME is not set, cannot trust {}",
                        p.cwd
                    ),
                }
            }
            let dir = state
                .data_dir
                .get()
                .ok_or_else(|| RpcError::new(ErrorKind::Internal, "not initialized"))?;
            let exe = std::env::current_exe()
                .map_err(|e| RpcError::new(ErrorKind::Internal, e.to_string()))?;
            to_value(launch::prepare(dir, &exe, &p)?)
        }
        other => Err(RpcError::new(
            ErrorKind::MethodNotFound,
            format!("unknown method {other}"),
        )),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("hook") {
        hook::run(args.get(1).map(String::as_str).unwrap_or_default());
    }
    let state = Arc::new(State::default());
    let served = serve(move |_host: Host, method_name: String, raw: Value| {
        let state = state.clone();
        async move { handle(&state, &method_name, raw) }
    })
    .await;
    if let Err(e) = served {
        eprintln!("asterism-plugin-claude: {e}");
        std::process::exit(1);
    }
}
