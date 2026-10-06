mod hook;
mod launch;

use std::path::PathBuf;
use std::sync::{Arc, OnceLock};

use asterism_plugin::protocol::{method, InitializeParams, InitializeResult, PrepareParams};
use asterism_plugin::{params, serve, to_value, ErrorKind, Host, RpcError};
use serde_json::Value;

fn handle(data_dir: &OnceLock<PathBuf>, method_name: &str, raw: Value) -> Result<Value, RpcError> {
    match method_name {
        method::INITIALIZE => {
            let p: InitializeParams = params(raw)?;
            let _ = data_dir.set(PathBuf::from(p.data_dir));
            to_value(InitializeResult { capabilities: vec!["agent".into()] })
        }
        method::AGENT_PREPARE => {
            let p: PrepareParams = params(raw)?;
            let dir = data_dir.get().ok_or_else(|| RpcError::new(ErrorKind::Internal, "not initialized"))?;
            let exe = std::env::current_exe().map_err(|e| RpcError::new(ErrorKind::Internal, e.to_string()))?;
            to_value(launch::prepare(dir, &exe, &p)?)
        }
        other => Err(RpcError::new(ErrorKind::MethodNotFound, format!("unknown method {other}"))),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(String::as_str) == Some("hook") {
        hook::run(args.get(1).map(String::as_str).unwrap_or_default());
    }
    let data_dir = Arc::new(OnceLock::new());
    let served = serve(move |_host: Host, method_name: String, raw: Value| {
        let data_dir = data_dir.clone();
        async move { handle(&data_dir, &method_name, raw) }
    })
    .await;
    if let Err(e) = served {
        eprintln!("asterism-plugin-claude: {e}");
        std::process::exit(1);
    }
}
