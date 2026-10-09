mod checks;
mod gh;
mod review;

use std::path::{Path, PathBuf};

use asterism_plugin::protocol::{
    method, CheckForgeParams, CloneParams, CreateRemoteParams, GetIssueParams, InitializeResult,
    ListReposParams, PullRequestsParams, ResolveOwnerParams, ResolveOwnerResult,
    ReviewAddCommentForgeParams, ReviewCommentForgeParams, ReviewGetForgeParams,
    ReviewReplyForgeParams, ReviewResolveForgeParams, ReviewSubmitForgeParams,
    ReviewViewedForgeParams, SearchIssuesParams, SearchPullRequestsParams, TaskSourceCheck,
    TaskSourceCheckParams,
};
use asterism_plugin::{params, serve, to_value, ErrorKind, Host, RpcError};
use serde_json::Value;

fn gh_bin() -> PathBuf {
    std::env::var_os("ASTERISM_GH")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from("gh"))
}

fn handle(method_name: &str, raw: Value) -> Result<Value, RpcError> {
    let gh = gh_bin();
    match method_name {
        method::INITIALIZE => to_value(InitializeResult {
            capabilities: vec![
                "forge".into(),
                "task_source".into(),
                "pull_requests".into(),
                "reviews".into(),
            ],
        }),
        method::FORGE_REVIEW_GET => {
            let p: ReviewGetForgeParams = params(raw)?;
            to_value(review::get(
                &gh,
                Path::new(&p.pr.project_path),
                p.pr.number,
            )?)
        }
        method::FORGE_REVIEW_COMMENT => {
            let p: ReviewCommentForgeParams = params(raw)?;
            review::comment(
                &gh,
                Path::new(&p.pr.project_path),
                p.pr.number,
                &p.path,
                p.line,
                p.side,
                &p.body,
                p.mode,
            )?;
            Ok(Value::Null)
        }
        method::FORGE_REVIEW_REPLY => {
            let p: ReviewReplyForgeParams = params(raw)?;
            review::reply(&gh, Path::new(&p.pr.project_path), &p.thread_id, &p.body)?;
            Ok(Value::Null)
        }
        method::FORGE_REVIEW_RESOLVE => {
            let p: ReviewResolveForgeParams = params(raw)?;
            review::resolve(&gh, Path::new(&p.pr.project_path), &p.thread_id, p.resolved)?;
            Ok(Value::Null)
        }
        method::FORGE_REVIEW_SET_VIEWED => {
            let p: ReviewViewedForgeParams = params(raw)?;
            review::set_viewed(
                &gh,
                Path::new(&p.pr.project_path),
                p.pr.number,
                &p.path,
                p.viewed,
            )?;
            Ok(Value::Null)
        }
        method::FORGE_REVIEW_SUBMIT => {
            let p: ReviewSubmitForgeParams = params(raw)?;
            review::submit(
                &gh,
                Path::new(&p.pr.project_path),
                p.pr.number,
                p.event,
                &p.body,
            )?;
            Ok(Value::Null)
        }
        method::FORGE_REVIEW_ADD_COMMENT => {
            let p: ReviewAddCommentForgeParams = params(raw)?;
            review::add_comment(&gh, Path::new(&p.pr.project_path), p.pr.number, &p.body)?;
            Ok(Value::Null)
        }
        method::FORGE_CHECKS_LOG => {
            let p: CheckForgeParams = params(raw)?;
            to_value(checks::log(
                &gh,
                Path::new(&p.pr.project_path),
                &p.check_id,
            )?)
        }
        method::FORGE_CHECKS_RERUN => {
            let p: CheckForgeParams = params(raw)?;
            checks::rerun(&gh, Path::new(&p.pr.project_path), &p.check_id)?;
            Ok(Value::Null)
        }
        method::FORGE_STATUS => to_value(gh::status(&gh)),
        method::FORGE_LIST_REPOS => {
            to_value(gh::repos(&gh, &params::<ListReposParams>(raw)?.owner)?)
        }
        method::FORGE_RESOLVE_OWNER => {
            let p: ResolveOwnerParams = params(raw)?;
            to_value(ResolveOwnerResult {
                owner: gh::resolve_owner(&gh::status(&gh), &p.owner, p.visibility)?,
            })
        }
        method::FORGE_CLONE => {
            let p: CloneParams = params(raw)?;
            gh::clone(&gh, &p.owner, &p.repo, Path::new(&p.target), &p.git_env)?;
            Ok(Value::Null)
        }
        method::FORGE_CREATE_REMOTE => {
            let p: CreateRemoteParams = params(raw)?;
            gh::create(
                &gh,
                &p.owner,
                &p.name,
                p.visibility,
                Path::new(&p.dir),
                &p.git_env,
            )?;
            Ok(Value::Null)
        }
        method::FORGE_PULL_REQUESTS => {
            let p: PullRequestsParams = params(raw)?;
            to_value(gh::pull_requests(
                &gh,
                Path::new(&p.project_path),
                &p.branches,
            )?)
        }
        method::FORGE_SEARCH_PULL_REQUESTS => {
            let p: SearchPullRequestsParams = params(raw)?;
            to_value(gh::search_pull_requests(
                &gh,
                Path::new(&p.project_path),
                &p.query,
                p.state,
            )?)
        }
        method::TASK_SOURCE_CHECK => {
            let p: TaskSourceCheckParams = params(raw)?;
            to_value(match gh::origin_repo(Path::new(&p.project_path)) {
                Ok(_) => TaskSourceCheck {
                    available: true,
                    reason: None,
                },
                Err(e) => TaskSourceCheck {
                    available: false,
                    reason: Some(e.message),
                },
            })
        }
        method::TASK_SOURCE_SEARCH => {
            let p: SearchIssuesParams = params(raw)?;
            to_value(gh::search_issues(
                &gh,
                Path::new(&p.project_path),
                &p.query,
                p.assigned_to_me,
            )?)
        }
        method::TASK_SOURCE_GET => {
            let p: GetIssueParams = params(raw)?;
            to_value(gh::get_issue(&gh, Path::new(&p.project_path), &p.key)?)
        }
        other => Err(RpcError::new(
            ErrorKind::MethodNotFound,
            format!("unknown method {other}"),
        )),
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
