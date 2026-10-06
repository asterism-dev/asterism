mod gh;

use std::path::{Path, PathBuf};

use asterism_plugin::protocol::{method, CloneParams, CreateRemoteParams, InitializeResult, ListReposParams, ResolveOwnerParams, ResolveOwnerResult};
use asterism_plugin::{params, serve, to_value, ErrorKind, Host, RpcError};
use serde_json::Value;

fn gh_bin() -> PathBuf {
    std::env::var_os("ASTERISM_GH").map(PathBuf::from).unwrap_or_else(|| PathBuf::from("gh"))
}

fn handle(method_name: &str, raw: Value) -> Result<Value, RpcError> {
    let gh = gh_bin();
    match method_name {
        method::INITIALIZE => to_value(InitializeResult { capabilities: vec!["forge".into()] }),
        method::FORGE_STATUS => to_value(gh::status(&gh)),
        method::FORGE_LIST_REPOS => to_value(gh::repos(&gh, &params::<ListReposParams>(raw)?.owner)?),
        method::FORGE_RESOLVE_OWNER => {
            let p: ResolveOwnerParams = params(raw)?;
            to_value(ResolveOwnerResult { owner: gh::resolve_owner(&gh::status(&gh), &p.owner, p.visibility)? })
        }
        method::FORGE_CLONE => {
            let p: CloneParams = params(raw)?;
            gh::clone(&gh, &p.owner, &p.repo, Path::new(&p.target), &p.git_env)?;
            Ok(Value::Null)
        }
        method::FORGE_CREATE_REMOTE => {
            let p: CreateRemoteParams = params(raw)?;
            gh::create(&gh, &p.owner, &p.name, p.visibility, Path::new(&p.dir), &p.git_env)?;
            Ok(Value::Null)
        }
        other => Err(RpcError::new(ErrorKind::MethodNotFound, format!("unknown method {other}"))),
    }
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let served = serve(|_host: Host, method_name: String, raw: Value| async move {
        tokio::task::spawn_blocking(move || handle(&method_name, raw))
            .await
            .map_err(|e| RpcError::new(ErrorKind::Internal, e.to_string()))?
    })
    .await;
    if let Err(e) = served {
        eprintln!("asterism-plugin-github: {e}");
        std::process::exit(1);
    }
}
