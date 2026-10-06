mod linear;

use std::sync::{Arc, Mutex};

use asterism_plugin::protocol::{method, GetIssueParams, InitializeParams, InitializeResult, SearchIssuesParams, SettingsChangedParams, TaskSourceCheck};
use asterism_plugin::{params, serve, to_value, ErrorKind, Host, RpcError};
use serde_json::{Map, Value};

type ApiKey = Arc<Mutex<Option<String>>>;

fn store_key(key: &ApiKey, settings: &Map<String, Value>) {
    *key.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = settings.get("api_key").and_then(Value::as_str).map(str::trim).filter(|k| !k.is_empty()).map(String::from);
}

fn api_key(key: &ApiKey) -> Result<String, RpcError> {
    key.lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .clone()
        .ok_or_else(|| RpcError::new(ErrorKind::NeedsSetup, "Linear API key is not set"))
}

fn handle(key: &ApiKey, method_name: &str, raw: Value) -> Result<Value, RpcError> {
    match method_name {
        method::INITIALIZE => {
            store_key(key, &params::<InitializeParams>(raw)?.settings);
            to_value(InitializeResult { capabilities: vec!["task_source".into()] })
        }
        method::SETTINGS_CHANGED => {
            store_key(key, &params::<SettingsChangedParams>(raw)?.settings);
            Ok(Value::Null)
        }
        method::TASK_SOURCE_CHECK => to_value(TaskSourceCheck { available: true, reason: None }),
        method::TASK_SOURCE_SEARCH => {
            let p: SearchIssuesParams = params(raw)?;
            to_value(linear::parse_hits(&linear::post(&api_key(key)?, &linear::search_request(&p.query, p.assigned_to_me))?)?)
        }
        method::TASK_SOURCE_GET => {
            let p: GetIssueParams = params(raw)?;
            to_value(linear::parse_issue(&linear::post(&api_key(key)?, &linear::get_request(&p.key))?, &p.key)?)
        }
        other => Err(RpcError::new(ErrorKind::MethodNotFound, format!("unknown method {other}"))),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let key: ApiKey = Arc::new(Mutex::new(None));
    let served = serve(move |_host: Host, method_name: String, raw: Value| {
        let key = key.clone();
        async move {
            tokio::task::spawn_blocking(move || handle(&key, &method_name, raw))
                .await
                .map_err(|e| RpcError::new(ErrorKind::Internal, e.to_string()))?
        }
    })
    .await;
    if let Err(e) = served {
        eprintln!("asterism-plugin-linear: {e}");
        std::process::exit(1);
    }
}
