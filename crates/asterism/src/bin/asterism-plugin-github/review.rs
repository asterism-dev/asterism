use std::path::Path;

use asterism_plugin::{ErrorKind, RpcError};
use asterism_proto::types::{DiffSide, ForgeReview, PendingReview, ReviewComment, ReviewThread};
use serde_json::Value;

use crate::gh::{origin_repo, parse, run_with_timeout, ISSUE_TIMEOUT};

// ponytail: first 100 files/threads/comments only; paginate with pageInfo if PRs grow beyond that.
const REVIEW_QUERY: &str = "query($owner:String!,$name:String!,$number:Int!){repository(owner:$owner,name:$name){pullRequest(number:$number){
id headRefOid baseRefOid
files(first:100){nodes{path viewerViewedState}}
reviewThreads(first:100){nodes{id path line originalLine diffSide isResolved isOutdated
 comments(first:100){nodes{id author{login} body createdAt pullRequestReview{id state}}}}}
comments(first:100){nodes{id author{login} body createdAt}}
reviews(first:100){nodes{id author{login} body state submittedAt viewerDidAuthor}}
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

fn comment(c: &Value, created_at: &Value) -> ReviewComment {
    ReviewComment {
        id: text(&c["id"]),
        author: text(&c["author"]["login"]),
        body: text(&c["body"]),
        created_at: text(created_at),
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
                    .map(|c| comment(c, &c["createdAt"]))
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
    let mut conversation: Vec<ReviewComment> = nodes(&pr["comments"])
        .iter()
        .map(|c| comment(c, &c["createdAt"]))
        .chain(
            nodes(&pr["reviews"])
                .iter()
                .filter(|r| r["state"] != "PENDING" && !text(&r["body"]).trim().is_empty())
                .map(|r| comment(r, &r["submittedAt"])),
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
    })
}

#[cfg(test)]
mod tests {
    use super::*;
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
}
