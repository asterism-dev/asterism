use std::time::Duration;

use asterism_plugin::{ErrorKind, RpcError};
use asterism_proto::types::{Issue, IssueHit};
use serde_json::{json, Value};

const ENDPOINT: &str = "https://api.linear.app/graphql";
const LIMIT: u32 = 50;
// Stays under the daemon's 20 s call cap.
const TIMEOUT: Duration = Duration::from_secs(15);
const MESSAGE_LIMIT: usize = 500;
const HIT_FIELDS: &str = "identifier title url updatedAt state { name } assignee { name }";

fn error(kind: ErrorKind, message: impl Into<String>) -> RpcError {
    RpcError::new(kind, message)
}

pub fn search_request(query: &str, assigned_to_me: bool) -> Value {
    let mut filter = json!({"state": {"type": {"nin": ["completed", "canceled"]}}});
    if assigned_to_me {
        filter["assignee"] = json!({"isMe": {"eq": true}});
    }
    let term = query.trim();
    if term.is_empty() {
        json!({
            "query": format!("query($filter: IssueFilter, $first: Int) {{ issues(filter: $filter, first: $first, orderBy: updatedAt) {{ nodes {{ {HIT_FIELDS} }} }} }}"),
            "variables": {"filter": filter, "first": LIMIT},
        })
    } else {
        json!({
            "query": format!("query($term: String!, $filter: IssueFilter, $first: Int) {{ searchIssues(term: $term, filter: $filter, first: $first) {{ nodes {{ {HIT_FIELDS} }} }} }}"),
            "variables": {"term": term, "filter": filter, "first": LIMIT},
        })
    }
}

pub fn get_request(key: &str) -> Value {
    json!({
        "query": "query($id: String!) { issue(id: $id) { identifier title url description branchName } }",
        "variables": {"id": key},
    })
}

fn data(response: &Value) -> Result<&Value, RpcError> {
    if let Some(message) = response["errors"][0]["message"].as_str() {
        return Err(error(ErrorKind::PluginError, format!("Linear: {message}")));
    }
    Ok(&response["data"])
}

pub fn parse_hits(response: &Value) -> Result<Vec<IssueHit>, RpcError> {
    let data = data(response)?;
    let nodes = data["searchIssues"]["nodes"].as_array().or_else(|| data["issues"]["nodes"].as_array()).cloned().unwrap_or_default();
    let mut hits: Vec<IssueHit> = nodes
        .iter()
        .filter_map(|n| {
            Some(IssueHit {
                key: n["identifier"].as_str()?.to_string(),
                title: n["title"].as_str()?.to_string(),
                url: n["url"].as_str()?.to_string(),
                state: n["state"]["name"].as_str().unwrap_or_default().to_string(),
                assignee: n["assignee"]["name"].as_str().map(String::from),
                updated_at: n["updatedAt"].as_str().map(String::from),
            })
        })
        .collect();
    hits.sort_by(|a, b| b.updated_at.cmp(&a.updated_at));
    Ok(hits)
}

pub fn parse_issue(response: &Value, key: &str) -> Result<Issue, RpcError> {
    let not_found = || error(ErrorKind::NotFound, format!("no Linear issue {key}"));
    if response["errors"][0]["message"].as_str().is_some_and(|m| m.starts_with("Entity not found")) {
        return Err(not_found());
    }
    let issue = &data(response)?["issue"];
    if issue.is_null() {
        return Err(not_found());
    }
    let field = |name: &str| issue[name].as_str().map(String::from).ok_or_else(|| error(ErrorKind::Internal, format!("Linear issue has no {name}")));
    Ok(Issue {
        key: field("identifier")?,
        title: field("title")?,
        url: field("url")?,
        description: issue["description"].as_str().unwrap_or_default().to_string(),
        branch: issue["branchName"].as_str().filter(|b| !b.is_empty()).map(String::from),
    })
}

pub fn post(api_key: &str, body: &Value) -> Result<Value, RpcError> {
    let response = ureq::post(ENDPOINT).timeout(TIMEOUT).set("Authorization", api_key).send_json(body.clone());
    match response {
        Ok(r) => r.into_json().map_err(|e| error(ErrorKind::Internal, format!("invalid Linear response: {e}"))),
        Err(ureq::Error::Status(401, _)) => Err(error(ErrorKind::PluginError, "Linear API key is invalid")),
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            if let Ok(json) = serde_json::from_str::<Value>(&body) {
                data(&json)?;
            }
            let text: String = body.chars().take(MESSAGE_LIMIT).collect();
            Err(error(ErrorKind::PluginError, format!("Linear returned HTTP {code}: {}", text.trim())))
        }
        Err(e) => Err(error(ErrorKind::PluginError, format!("cannot reach Linear: {e}"))),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn node(key: &str, updated: &str, assignee: Option<&str>) -> Value {
        json!({"identifier": key, "title": format!("Title {key}"), "url": format!("https://linear.app/bna/issue/{key}"),
               "updatedAt": updated, "state": {"name": "In Progress"}, "assignee": assignee.map(|a| json!({"name": a}))})
    }

    #[test]
    fn empty_queries_list_issues_and_text_searches() {
        let mine = search_request("", true);
        assert!(mine["query"].as_str().unwrap().contains("issues(filter: $filter"));
        assert_eq!(mine["variables"]["filter"]["assignee"], json!({"isMe": {"eq": true}}));
        assert_eq!(mine["variables"]["filter"]["state"], json!({"type": {"nin": ["completed", "canceled"]}}));
        assert_eq!(mine["variables"]["first"], 50);
        let all = search_request("  login ", false);
        assert!(all["query"].as_str().unwrap().contains("searchIssues(term: $term"));
        assert_eq!(all["variables"]["term"], "login");
        assert!(all["variables"]["filter"].get("assignee").is_none());
    }

    #[test]
    fn hits_are_mapped_and_newest_first() {
        let response = json!({"data": {"searchIssues": {"nodes": [node("TRA-1", "2026-09-01T00:00:00.000Z", None), node("TRA-2", "2026-10-01T00:00:00.000Z", Some("Chris"))]}}});
        let hits = parse_hits(&response).unwrap();
        assert_eq!(hits.iter().map(|h| h.key.as_str()).collect::<Vec<_>>(), ["TRA-2", "TRA-1"]);
        assert_eq!((hits[0].state.as_str(), hits[0].assignee.as_deref()), ("In Progress", Some("Chris")));
        assert_eq!(hits[1].assignee, None);
        let listed = json!({"data": {"issues": {"nodes": [node("TRA-3", "2026-10-02T00:00:00.000Z", None)]}}});
        assert_eq!(parse_hits(&listed).unwrap()[0].url, "https://linear.app/bna/issue/TRA-3");
    }

    #[test]
    fn graphql_errors_surface_even_with_http_200() {
        let response = json!({"errors": [{"message": "Argument Validation Error"}], "data": null});
        assert!(parse_hits(&response).unwrap_err().message.contains("Argument Validation Error"));
        assert!(parse_issue(&response, "TRA-1").unwrap_err().message.contains("Argument Validation Error"));
    }

    #[test]
    fn issues_carry_linears_branch_name() {
        let request = get_request("TRA-1343");
        assert_eq!(request["variables"]["id"], "TRA-1343");
        let response = json!({"data": {"issue": {"identifier": "TRA-1343", "title": "Migrate", "url": "https://linear.app/bna/issue/TRA-1343",
            "description": null, "branchName": "feature/tra-1343-migrate-trading-infrastructure-unit-conversions-onto-bbase"}}});
        let issue = parse_issue(&response, "TRA-1343").unwrap();
        assert_eq!(issue.branch.as_deref(), Some("feature/tra-1343-migrate-trading-infrastructure-unit-conversions-onto-bbase"));
        assert_eq!(issue.description, "");
        let missing = parse_issue(&json!({"data": {"issue": null}}), "TRA-9").unwrap_err();
        assert_eq!(missing.kind(), ErrorKind::NotFound);
        let unknown = json!({"errors": [{"message": "Entity not found: Issue", "extensions": {"code": "INVALID_INPUT"}}], "data": null});
        assert_eq!(parse_issue(&unknown, "TRA-9").unwrap_err().kind(), ErrorKind::NotFound);
    }
}
