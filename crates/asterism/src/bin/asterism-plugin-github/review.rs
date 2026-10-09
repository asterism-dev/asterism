use std::path::Path;

use asterism_plugin::protocol::ForgeCommentMode;
use asterism_plugin::{ErrorKind, RpcError};
use asterism_proto::types::{
    CheckRun, CheckStatus, ConversationItem, ConversationKind, DiffSide, ForgeReview,
    PendingReview, ReviewComment, ReviewEvent, ReviewThread,
};
use serde_json::Value;

use crate::gh::{origin_repo, parse, run_with_timeout, ISSUE_TIMEOUT};

// ponytail: first 100 files/threads/comments/commits/checks only; paginate with pageInfo if PRs grow beyond that.
const REVIEW_QUERY: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){
id headRefOid baseRefOid title body url author{login}
files(first:100){nodes{path viewerViewedState}}
reviewThreads(first:100){nodes{id path line originalLine diffSide isResolved isOutdated
 comments(first:100){nodes{id author{login} body createdAt pullRequestReview{id state}}}}}
comments(first:100){nodes{id author{login} body createdAt}}
reviews(first:100){nodes{id author{login} body state submittedAt viewerDidAuthor}}
commits(last:100){nodes{commit{oid statusCheckRollup{state}}}}
head:commits(last:1){nodes{commit{statusCheckRollup{contexts(first:100){nodes{__typename
 ...on CheckRun{databaseId name status conclusion startedAt completedAt detailsUrl checkSuite{workflowRun{databaseId workflow{name}}}}
 ...on StatusContext{context state targetUrl createdAt}}}}}}}
}}}";

pub enum GqlVar {
    Str(String),
    Int(u64),
}

pub fn graphql(
    gh: &Path,
    project: &Path,
    query: &str,
    vars: &[(&str, GqlVar)],
) -> Result<Value, RpcError> {
    let mut args = vec![
        "api".to_string(),
        "graphql".into(),
        "-f".into(),
        format!("query={query}"),
    ];
    for (name, value) in vars {
        match value {
            GqlVar::Str(s) => args.extend(["-f".into(), format!("{name}={s}")]),
            GqlVar::Int(n) => args.extend(["-F".into(), format!("{name}={n}")]),
        }
    }
    let args: Vec<&str> = args.iter().map(String::as_str).collect();
    let value: Value = parse(&run_with_timeout(gh, &args, ISSUE_TIMEOUT, Some(project))?)?;
    if let Some(errors) = value["errors"].as_array().filter(|e| !e.is_empty()) {
        let message = errors[0]["message"].as_str().unwrap_or("GraphQL error");
        return Err(RpcError::new(ErrorKind::PluginError, message.to_string()));
    }
    Ok(value)
}

/// `owner`, `name` and `number` variables for queries about one pull request.
pub fn pr_vars(project: &Path, number: u64) -> Result<Vec<(&'static str, GqlVar)>, RpcError> {
    let repo = origin_repo(project)?;
    let (owner, name) = repo
        .split_once('/')
        .ok_or_else(|| RpcError::new(ErrorKind::Internal, format!("unexpected repo {repo}")))?;
    Ok(vec![
        ("owner", GqlVar::Str(owner.into())),
        ("name", GqlVar::Str(name.into())),
        ("number", GqlVar::Int(number)),
    ])
}

pub fn get(gh: &Path, project: &Path, number: u64) -> Result<ForgeReview, RpcError> {
    let value = graphql(gh, project, REVIEW_QUERY, &pr_vars(project, number)?)?;
    parse_review(&value, number)
}

fn nodes(v: &Value) -> &[Value] {
    v["nodes"].as_array().map(Vec::as_slice).unwrap_or_default()
}

fn text(v: &Value) -> String {
    v.as_str().unwrap_or_default().to_string()
}

fn lower(v: &Value) -> Option<String> {
    v.as_str().map(str::to_ascii_lowercase)
}

fn opt_text(v: &Value) -> Option<String> {
    v.as_str().map(String::from)
}

fn check_run(c: &Value) -> Option<CheckRun> {
    if c["__typename"] == "StatusContext" {
        let state = c["state"].as_str().unwrap_or_default();
        let context = text(&c["context"]);
        return Some(CheckRun {
            id: format!("status:{context}"),
            name: context,
            workflow: String::new(),
            status: match state {
                "PENDING" => CheckStatus::Running,
                "EXPECTED" => CheckStatus::Queued,
                _ => CheckStatus::Done,
            },
            conclusion: (!matches!(state, "PENDING" | "EXPECTED"))
                .then(|| state.to_ascii_lowercase()),
            started_at: opt_text(&c["createdAt"]),
            completed_at: None,
            url: text(&c["targetUrl"]),
            has_log: false,
            rerunnable: false,
        });
    }
    let run = &c["checkSuite"]["workflowRun"];
    let actions = !run.is_null();
    Some(CheckRun {
        id: c["databaseId"].as_u64()?.to_string(),
        name: text(&c["name"]),
        workflow: text(&run["workflow"]["name"]),
        status: match c["status"].as_str().unwrap_or_default() {
            "COMPLETED" => CheckStatus::Done,
            "IN_PROGRESS" => CheckStatus::Running,
            _ => CheckStatus::Queued,
        },
        conclusion: lower(&c["conclusion"]),
        started_at: opt_text(&c["startedAt"]),
        completed_at: opt_text(&c["completedAt"]),
        url: text(&c["detailsUrl"]),
        has_log: actions,
        rerunnable: actions,
    })
}

fn to_comment(c: &Value, created_at: &Value) -> ReviewComment {
    ReviewComment {
        id: text(&c["id"]),
        author: text(&c["author"]["login"]),
        body: text(&c["body"]),
        created_at: text(created_at),
    }
}

fn conversation_item(
    c: &Value,
    created_at: &Value,
    kind: ConversationKind,
    state: Option<String>,
) -> ConversationItem {
    ConversationItem {
        id: text(&c["id"]),
        kind,
        author: text(&c["author"]["login"]),
        body: text(&c["body"]),
        created_at: text(created_at),
        state,
    }
}

pub fn parse_review(value: &Value, number: u64) -> Result<ForgeReview, RpcError> {
    let pr = &value["data"]["repository"]["pullRequest"];
    if pr.is_null() {
        return Err(RpcError::new(
            ErrorKind::NotFound,
            format!("pull request #{number} not found"),
        ));
    }
    let threads = nodes(&pr["reviewThreads"])
        .iter()
        .map(|t| {
            let comments = nodes(&t["comments"]);
            ReviewThread {
                id: text(&t["id"]),
                path: text(&t["path"]),
                line: t["line"]
                    .as_u64()
                    .or(t["originalLine"].as_u64())
                    .unwrap_or(0) as u32,
                side: if t["diffSide"] == "LEFT" {
                    DiffSide::Old
                } else {
                    DiffSide::New
                },
                outdated: t["isOutdated"].as_bool().unwrap_or(false),
                resolved: t["isResolved"].as_bool().unwrap_or(false),
                local: false,
                pending: !comments.is_empty()
                    && comments
                        .iter()
                        .all(|c| c["pullRequestReview"]["state"] == "PENDING"),
                comments: comments
                    .iter()
                    .map(|c| to_comment(c, &c["createdAt"]))
                    .collect(),
            }
        })
        .collect::<Vec<_>>();
    let pending_review = nodes(&pr["reviews"])
        .iter()
        .find(|r| r["state"] == "PENDING" && r["viewerDidAuthor"] == true)
        .map(|r| {
            let id = text(&r["id"]);
            PendingReview {
                comments: nodes(&pr["reviewThreads"])
                    .iter()
                    .flat_map(|t| nodes(&t["comments"]))
                    .filter(|c| c["pullRequestReview"]["id"] == id.as_str())
                    .count() as u32,
                id,
            }
        });
    let checks = nodes(&pr["head"])
        .first()
        .map(|n| nodes(&n["commit"]["statusCheckRollup"]["contexts"]))
        .unwrap_or_default()
        .iter()
        .filter_map(check_run)
        .collect();
    let commit_checks = nodes(&pr["commits"])
        .iter()
        .filter_map(|n| {
            Some((
                text(&n["commit"]["oid"]),
                lower(&n["commit"]["statusCheckRollup"]["state"])?,
            ))
        })
        .collect();
    let mut conversation: Vec<ConversationItem> = nodes(&pr["comments"])
        .iter()
        .map(|c| conversation_item(c, &c["createdAt"], ConversationKind::Comment, None))
        .chain(
            nodes(&pr["reviews"])
                .iter()
                .filter(|r| {
                    r["state"] != "PENDING"
                        && (!text(&r["body"]).trim().is_empty()
                            || matches!(
                                r["state"].as_str(),
                                Some("APPROVED" | "CHANGES_REQUESTED" | "DISMISSED")
                            ))
                })
                .map(|r| {
                    let state = text(&r["state"]).to_lowercase();
                    conversation_item(r, &r["submittedAt"], ConversationKind::Review, Some(state))
                }),
        )
        .collect();
    conversation.sort_by(|a, b| a.created_at.cmp(&b.created_at));
    Ok(ForgeReview {
        head_sha: text(&pr["headRefOid"]),
        base_sha: text(&pr["baseRefOid"]),
        head_ref: format!("refs/pull/{number}/head"),
        threads,
        conversation,
        viewed_files: nodes(&pr["files"])
            .iter()
            .filter(|f| f["viewerViewedState"] == "VIEWED")
            .map(|f| text(&f["path"]))
            .collect(),
        pending_review,
        title: text(&pr["title"]),
        body: text(&pr["body"]),
        author: text(&pr["author"]["login"]),
        url: text(&pr["url"]),
        checks,
        commit_checks,
    })
}

const IDS_QUERY: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){id headRefOid reviews(first:20,states:PENDING){nodes{id viewerDidAuthor}}}}}";

struct PrIds {
    pr: String,
    pending: Option<String>,
}

fn pr_ids(gh: &Path, project: &Path, number: u64) -> Result<PrIds, RpcError> {
    let v = graphql(gh, project, IDS_QUERY, &pr_vars(project, number)?)?;
    let pr = &v["data"]["repository"]["pullRequest"];
    Ok(PrIds {
        pr: text(&pr["id"]),
        pending: nodes(&pr["reviews"])
            .iter()
            .find(|r| r["viewerDidAuthor"] == true)
            .map(|r| text(&r["id"])),
    })
}

fn side_name(side: DiffSide) -> &'static str {
    match side {
        DiffSide::Old => "LEFT",
        DiffSide::New => "RIGHT",
    }
}

fn event_name(event: ReviewEvent) -> &'static str {
    match event {
        ReviewEvent::Comment => "COMMENT",
        ReviewEvent::Approve => "APPROVE",
        ReviewEvent::RequestChanges => "REQUEST_CHANGES",
    }
}

fn s(v: &str) -> GqlVar {
    GqlVar::Str(v.to_string())
}

#[allow(clippy::too_many_arguments)]
pub fn comment(
    gh: &Path,
    project: &Path,
    number: u64,
    path: &str,
    line: u32,
    side: DiffSide,
    body: &str,
    mode: ForgeCommentMode,
) -> Result<(), RpcError> {
    let ids = pr_ids(gh, project, number)?;
    let thread = [
        ("path", s(path)),
        ("line", GqlVar::Int(line.into())),
        ("side", s(side_name(side))),
        ("body", s(body)),
    ];
    match (mode, ids.pending) {
        (ForgeCommentMode::Review, Some(review)) => {
            let q = "mutation($review:ID!,$path:String!,$line:Int!,$side:DiffSide!,$body:String!){addPullRequestReviewThread(input:{pullRequestReviewId:$review,path:$path,line:$line,side:$side,body:$body}){thread{id}}}";
            let mut vars = vec![("review", GqlVar::Str(review))];
            vars.extend(thread);
            graphql(gh, project, q, &vars)?;
        }
        (mode, _) => {
            // A review with an event is submitted at once, which is how GitHub posts a single comment.
            let event = matches!(mode, ForgeCommentMode::Single);
            let q = if event {
                "mutation($pr:ID!,$event:PullRequestReviewEvent!,$path:String!,$line:Int!,$side:DiffSide!,$body:String!){addPullRequestReview(input:{pullRequestId:$pr,event:$event,threads:[{path:$path,line:$line,side:$side,body:$body}]}){pullRequestReview{id}}}"
            } else {
                "mutation($pr:ID!,$path:String!,$line:Int!,$side:DiffSide!,$body:String!){addPullRequestReview(input:{pullRequestId:$pr,threads:[{path:$path,line:$line,side:$side,body:$body}]}){pullRequestReview{id}}}"
            };
            let mut vars = vec![("pr", GqlVar::Str(ids.pr))];
            if event {
                vars.push(("event", s("COMMENT")));
            }
            vars.extend(thread);
            graphql(gh, project, q, &vars)?;
        }
    }
    Ok(())
}

pub fn reply(gh: &Path, project: &Path, thread_id: &str, body: &str) -> Result<(), RpcError> {
    let q = "mutation($thread:ID!,$body:String!){addPullRequestReviewThreadReply(input:{pullRequestReviewThreadId:$thread,body:$body}){comment{id}}}";
    graphql(
        gh,
        project,
        q,
        &[("thread", s(thread_id)), ("body", s(body))],
    )
    .map(|_| ())
}

pub fn resolve(gh: &Path, project: &Path, thread_id: &str, resolved: bool) -> Result<(), RpcError> {
    let q = if resolved {
        "mutation($thread:ID!){resolveReviewThread(input:{threadId:$thread}){thread{id}}}"
    } else {
        "mutation($thread:ID!){unresolveReviewThread(input:{threadId:$thread}){thread{id}}}"
    };
    graphql(gh, project, q, &[("thread", s(thread_id))]).map(|_| ())
}

pub fn set_viewed(
    gh: &Path,
    project: &Path,
    number: u64,
    path: &str,
    viewed: bool,
) -> Result<(), RpcError> {
    let ids = pr_ids(gh, project, number)?;
    let q = if viewed {
        "mutation($pr:ID!,$path:String!){markFileAsViewed(input:{pullRequestId:$pr,path:$path}){clientMutationId}}"
    } else {
        "mutation($pr:ID!,$path:String!){unmarkFileAsViewed(input:{pullRequestId:$pr,path:$path}){clientMutationId}}"
    };
    graphql(
        gh,
        project,
        q,
        &[("pr", GqlVar::Str(ids.pr)), ("path", s(path))],
    )
    .map(|_| ())
}

pub fn submit(
    gh: &Path,
    project: &Path,
    number: u64,
    event: ReviewEvent,
    body: &str,
) -> Result<(), RpcError> {
    let ids = pr_ids(gh, project, number)?;
    let event = s(event_name(event));
    match ids.pending {
        Some(review) => {
            let q = "mutation($review:ID!,$event:PullRequestReviewEvent!,$body:String){submitPullRequestReview(input:{pullRequestReviewId:$review,event:$event,body:$body}){pullRequestReview{id}}}";
            graphql(gh, project, q, &[("review", GqlVar::Str(review)), ("event", event), ("body", s(body))])
        }
        None => {
            let q = "mutation($pr:ID!,$event:PullRequestReviewEvent!,$body:String){addPullRequestReview(input:{pullRequestId:$pr,event:$event,body:$body}){pullRequestReview{id}}}";
            graphql(gh, project, q, &[("pr", GqlVar::Str(ids.pr)), ("event", event), ("body", s(body))])
        }
    }
    .map(|_| ())
}

#[cfg(test)]
mod tests {
    use super::*;
    use asterism_proto::types::CheckStatus;
    use serde_json::json;

    fn sample() -> Value {
        json!({"data": {"repository": {"pullRequest": {
            "id": "PR_1", "headRefOid": "h1", "baseRefOid": "b1",
            "files": {"nodes": [
                {"path": "a.rs", "viewerViewedState": "VIEWED"},
                {"path": "b.rs", "viewerViewedState": "UNVIEWED"}
            ]},
            "reviewThreads": {"nodes": [
                {"id": "T1", "path": "a.rs", "line": 4, "originalLine": 3, "diffSide": "RIGHT",
                 "isResolved": false, "isOutdated": false,
                 "comments": {"nodes": [
                    {"id": "C1", "author": {"login": "alice"}, "body": "why?", "createdAt": "2026-10-01T10:00:00Z",
                     "pullRequestReview": {"id": "R1", "state": "COMMENTED"}}
                 ]}},
                {"id": "T2", "path": "b.rs", "line": null, "originalLine": 9, "diffSide": "LEFT",
                 "isResolved": true, "isOutdated": true,
                 "comments": {"nodes": [
                    {"id": "C2", "author": {"login": "me"}, "body": "draft", "createdAt": "2026-10-02T10:00:00Z",
                     "pullRequestReview": {"id": "R2", "state": "PENDING"}}
                 ]}}
            ]},
            "comments": {"nodes": [
                {"id": "I1", "author": {"login": "bob"}, "body": "LGTM overall", "createdAt": "2026-10-01T09:00:00Z"}
            ]},
            "reviews": {"nodes": [
                {"id": "R1", "author": {"login": "alice"}, "body": "Some notes", "state": "COMMENTED",
                 "submittedAt": "2026-10-01T10:05:00Z", "viewerDidAuthor": false},
                {"id": "R2", "author": {"login": "me"}, "body": "", "state": "PENDING",
                 "submittedAt": null, "viewerDidAuthor": true}
            ]}
        }}}})
    }

    #[test]
    fn a_review_is_parsed_into_threads_viewed_files_and_conversation() {
        let r = parse_review(&sample(), 7).unwrap();
        assert_eq!((r.head_sha.as_str(), r.base_sha.as_str()), ("h1", "b1"));
        assert_eq!(r.head_ref, "refs/pull/7/head");
        assert_eq!(r.viewed_files, vec!["a.rs".to_string()]);
        assert_eq!(r.threads.len(), 2);
        let t1 = &r.threads[0];
        assert_eq!((t1.line, t1.side, t1.pending), (4, DiffSide::New, false));
        assert_eq!(t1.comments[0].author, "alice");
        let t2 = &r.threads[1];
        // Outdated threads fall back to their original line.
        assert_eq!(
            (t2.line, t2.side, t2.outdated, t2.resolved, t2.pending),
            (9, DiffSide::Old, true, true, true)
        );
        assert_eq!(
            r.pending_review,
            Some(PendingReview {
                id: "R2".into(),
                comments: 1
            })
        );
        let bodies: Vec<&str> = r.conversation.iter().map(|c| c.body.as_str()).collect();
        assert_eq!(bodies, vec!["LGTM overall", "Some notes"]);
    }

    fn sample_with_checks() -> Value {
        let mut v = sample();
        let pr = &mut v["data"]["repository"]["pullRequest"];
        pr["title"] = json!("Add review pane");
        pr["body"] = json!("Implements the pane.");
        pr["url"] = json!("https://github.com/acme/api/pull/7");
        pr["author"] = json!({"login": "carol"});
        pr["commits"] = json!({"nodes": [
            {"commit": {"oid": "c1", "statusCheckRollup": {"state": "SUCCESS"}}},
            {"commit": {"oid": "h1", "statusCheckRollup": null}}
        ]});
        pr["head"] = json!({"nodes": [{"commit": {"statusCheckRollup": {"contexts": {"nodes": [
            {"__typename": "CheckRun", "databaseId": 4242, "name": "test", "status": "COMPLETED",
             "conclusion": "FAILURE", "startedAt": "2026-10-09T10:00:00Z", "completedAt": "2026-10-09T10:03:00Z",
             "detailsUrl": "https://github.com/acme/api/actions/runs/9/job/4242",
             "checkSuite": {"workflowRun": {"databaseId": 9, "workflow": {"name": "CI"}}}},
            {"__typename": "CheckRun", "databaseId": 4243, "name": "lint", "status": "IN_PROGRESS",
             "conclusion": null, "startedAt": "2026-10-09T10:00:00Z", "completedAt": null,
             "detailsUrl": "https://x", "checkSuite": {"workflowRun": {"databaseId": 9, "workflow": {"name": "CI"}}}},
            {"__typename": "StatusContext", "context": "ci/external", "state": "PENDING",
             "targetUrl": "https://ext", "createdAt": "2026-10-09T10:00:00Z"}
        ]}}}}]});
        pr["reviews"]["nodes"].as_array_mut().unwrap().push(json!(
            {"id": "R3", "author": {"login": "dave"}, "body": "", "state": "APPROVED",
             "submittedAt": "2026-10-03T10:00:00Z", "viewerDidAuthor": false}));
        v
    }

    #[test]
    fn pr_metadata_checks_and_commit_states_are_parsed() {
        let r = parse_review(&sample_with_checks(), 7).unwrap();
        assert_eq!(
            (r.title.as_str(), r.body.as_str(), r.author.as_str()),
            ("Add review pane", "Implements the pane.", "carol")
        );
        assert_eq!(r.url, "https://github.com/acme/api/pull/7");
        assert_eq!(
            r.commit_checks.get("c1").map(String::as_str),
            Some("success")
        );
        assert!(!r.commit_checks.contains_key("h1"));
        let test = &r.checks[0];
        assert_eq!(
            (test.id.as_str(), test.workflow.as_str(), test.status),
            ("4242", "CI", CheckStatus::Done)
        );
        assert_eq!(test.conclusion.as_deref(), Some("failure"));
        assert!(test.has_log && test.rerunnable);
        assert_eq!(r.checks[1].status, CheckStatus::Running);
        assert!(r.checks[1].conclusion.is_none());
    }

    #[test]
    fn status_contexts_have_no_log_or_rerun() {
        let r = parse_review(&sample_with_checks(), 7).unwrap();
        let ext = &r.checks[2];
        assert_eq!(
            (ext.id.as_str(), ext.name.as_str(), ext.status),
            ("status:ci/external", "ci/external", CheckStatus::Running)
        );
        assert!(!ext.has_log && !ext.rerunnable);
        assert_eq!(ext.url, "https://ext");
    }

    #[test]
    fn approvals_without_a_body_are_conversation_events() {
        let r = parse_review(&sample_with_checks(), 7).unwrap();
        let approved = r.conversation.iter().find(|c| c.id == "R3").unwrap();
        assert_eq!(
            (
                approved.kind,
                approved.state.as_deref(),
                approved.author.as_str()
            ),
            (ConversationKind::Review, Some("approved"), "dave")
        );
        assert!(!r.conversation.iter().any(|c| c.id == "R2"));
    }

    use std::os::unix::fs::PermissionsExt;
    use std::path::PathBuf;

    fn fake_gh(dir: &Path, pending: bool) -> PathBuf {
        let log = dir.join("gh.log");
        let reviews = if pending {
            r#"[{"id":"R9","viewerDidAuthor":true}]"#
        } else {
            "[]"
        };
        let script = format!(
            r#"#!/bin/sh
for a in "$@"; do echo "$a" >> "{log}"; done
echo "--" >> "{log}"
echo '{{"data":{{"repository":{{"pullRequest":{{"id":"PR_1","headRefOid":"h1","reviews":{{"nodes":{reviews}}}}}}}}}}}'
"#,
            log = log.display()
        );
        let bin = dir.join("gh");
        std::fs::write(&bin, script).unwrap();
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();
        bin
    }

    fn project(dir: &Path) -> PathBuf {
        let p = dir.join("repo");
        std::fs::create_dir_all(&p).unwrap();
        for args in [
            vec!["init", "-q"],
            vec!["remote", "add", "origin", "https://github.com/acme/api.git"],
        ] {
            assert!(std::process::Command::new("git")
                .arg("-C")
                .arg(&p)
                .args(&args)
                .status()
                .unwrap()
                .success());
        }
        p
    }

    fn calls(dir: &Path) -> Vec<String> {
        std::fs::read_to_string(dir.join("gh.log"))
            .unwrap()
            .split("--\n")
            .filter(|c| !c.is_empty())
            .map(String::from)
            .collect()
    }

    #[test]
    fn a_single_comment_is_a_submitted_comment_review() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake_gh(dir.path(), false);
        comment(
            &gh,
            &project(dir.path()),
            7,
            "a.rs",
            4,
            DiffSide::New,
            "why?",
            ForgeCommentMode::Single,
        )
        .unwrap();
        let last = calls(dir.path()).pop().unwrap();
        assert!(last.contains("addPullRequestReview("), "{last}");
        assert!(
            last.contains("event=COMMENT")
                && last.contains("side=RIGHT")
                && last.contains("line=4")
        );
    }

    #[test]
    fn a_review_comment_joins_the_pending_review_or_starts_one() {
        let dir = tempfile::tempdir().unwrap();
        let repo = project(dir.path());
        comment(
            &fake_gh(dir.path(), true),
            &repo,
            7,
            "a.rs",
            4,
            DiffSide::Old,
            "x",
            ForgeCommentMode::Review,
        )
        .unwrap();
        let joined = calls(dir.path()).pop().unwrap();
        assert!(
            joined.contains("addPullRequestReviewThread(")
                && joined.contains("review=R9")
                && joined.contains("side=LEFT")
        );
        std::fs::remove_file(dir.path().join("gh.log")).unwrap();
        comment(
            &fake_gh(dir.path(), false),
            &repo,
            7,
            "a.rs",
            4,
            DiffSide::New,
            "x",
            ForgeCommentMode::Review,
        )
        .unwrap();
        let started = calls(dir.path()).pop().unwrap();
        assert!(
            started.contains("addPullRequestReview(") && !started.contains("event="),
            "{started}"
        );
    }

    #[test]
    fn submit_uses_the_pending_review_when_there_is_one() {
        let dir = tempfile::tempdir().unwrap();
        let repo = project(dir.path());
        submit(
            &fake_gh(dir.path(), true),
            &repo,
            7,
            ReviewEvent::RequestChanges,
            "fix it",
        )
        .unwrap();
        let last = calls(dir.path()).pop().unwrap();
        assert!(
            last.contains("submitPullRequestReview(")
                && last.contains("event=REQUEST_CHANGES")
                && last.contains("review=R9")
        );
    }

    #[test]
    fn reply_resolve_and_viewed_call_their_mutations() {
        let dir = tempfile::tempdir().unwrap();
        let gh = fake_gh(dir.path(), false);
        let repo = project(dir.path());
        reply(&gh, &repo, "T1", "ok").unwrap();
        resolve(&gh, &repo, "T1", true).unwrap();
        resolve(&gh, &repo, "T1", false).unwrap();
        set_viewed(&gh, &repo, 7, "a.rs", true).unwrap();
        set_viewed(&gh, &repo, 7, "a.rs", false).unwrap();
        let all = calls(dir.path()).join("\n");
        for m in [
            "addPullRequestReviewThreadReply(",
            "resolveReviewThread(",
            "unresolveReviewThread(",
            "markFileAsViewed(",
            "unmarkFileAsViewed(",
        ] {
            assert!(all.contains(m), "missing {m}");
        }
    }
}
