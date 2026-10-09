use asterism_proto::rpc::*;
use asterism_proto::types::*;
use serde_json::json;

fn sample_task() -> Task {
    Task {
        id: 1,
        project_id: 2,
        title: "Fix login".into(),
        slug: "1-fix-login".into(),
        branch: "asterism/1-fix-login".into(),
        base_branch: "main".into(),
        worktree_path: "/tmp/wt".into(),
        prompt: Some("go".into()),
        archived: false,
        created_at: 1_700_000_000,
        last_activity_at: 1_700_000_060,
        issue: None,
    }
}

#[test]
fn server_messages_decode_by_shape() {
    let resp: ServerMessage =
        serde_json::from_value(json!({"jsonrpc": "2.0", "id": 7, "result": {"ok": true}})).unwrap();
    assert_eq!(
        resp,
        ServerMessage::Response(Response::ok(7, json!({"ok": true})))
    );

    let note: ServerMessage = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "method": "session.status_changed",
        "params": {"session_id": 3, "status": "idle"}
    }))
    .unwrap();
    let ServerMessage::Notification(n) = note else {
        panic!("expected a notification")
    };
    assert_eq!(
        Event::from_notification(&n),
        Some(Event::SessionStatusChanged {
            session_id: 3,
            status: SessionStatus::Idle
        })
    );
}

#[test]
fn events_roundtrip_through_notifications() {
    let events = [
        Event::SessionOutput {
            session_id: 1,
            data: "aGk=".into(),
        },
        Event::TaskChanged(sample_task()),
        Event::ProjectRemoved { project_id: 4 },
    ];
    for event in events {
        assert_eq!(
            Event::from_notification(&event.to_notification()),
            Some(event.clone())
        );
    }
}

#[test]
fn session_kind_is_tagged_by_type() {
    assert_eq!(
        serde_json::to_value(SessionKind::Agent {
            name: "claude".into()
        })
        .unwrap(),
        json!({"type": "agent", "name": "claude"})
    );
    assert_eq!(
        serde_json::to_value(SessionKind::Shell).unwrap(),
        json!({"type": "shell"})
    );
    assert_eq!(
        serde_json::to_value(SessionStatus::WaitingInput).unwrap(),
        json!("waiting_input")
    );
}

#[test]
fn errors_carry_kind_and_stable_code() {
    let err = RpcError::new(ErrorKind::DirtyWorktree, "dirty");
    assert_eq!(
        serde_json::to_value(&err).unwrap(),
        json!({"code": -32004, "message": "dirty", "data": {"kind": "dirty_worktree"}})
    );
    assert_eq!(err.kind(), ErrorKind::DirtyWorktree);
}

#[test]
fn optional_params_have_defaults() {
    let list: TaskListParams = serde_json::from_value(json!({})).unwrap();
    assert_eq!(list.project_id, None);
    assert!(!list.include_archived);
    let read: SessionReadParams = serde_json::from_value(json!({"session_id": 1})).unwrap();
    assert_eq!(read.lines, 50);
}

#[test]
fn agent_config_defaults_and_roundtrips() {
    let empty: AgentConfig = serde_json::from_value(json!({})).unwrap();
    assert_eq!(empty, AgentConfig::default());

    let config: AgentConfig = serde_json::from_value(json!({
        "args": ["--model", "opus"],
        "env": {"remove": ["AWS_*"], "set": {"FOO": "bar"}},
        "mcp": {"mcpServers": {}},
        "hooks": null
    }))
    .unwrap();
    assert_eq!(config.args, ["--model", "opus"]);
    assert_eq!(config.env.set.get("FOO").map(String::as_str), Some("bar"));
    assert!(config.hooks.is_none());
    let back: AgentConfig = serde_json::from_value(serde_json::to_value(&config).unwrap()).unwrap();
    assert_eq!(back, config);
}

#[test]
fn session_removed_event_roundtrips() {
    let event = Event::SessionRemoved { session_id: 9 };
    let notification = event.to_notification();
    assert_eq!(notification.method, "session.removed");
    assert_eq!(Event::from_notification(&notification), Some(event));
}

#[test]
fn project_and_path_types_roundtrip() {
    let create: ProjectCreateParams = serde_json::from_value(json!({"name": "demo"})).unwrap();
    assert!(create.remote.is_none());
    let create: ProjectCreateParams = serde_json::from_value(json!({
        "name": "demo", "remote": {"forge": "github", "owner": "acme", "visibility": "internal"}
    }))
    .unwrap();
    assert_eq!(create.remote.unwrap().visibility, Visibility::Internal);

    let info = NodeConfigInfo {
        config: NodeConfig {
            paths: PathSettings {
                repos: "~/r".into(),
                worktrees: "~/w".into(),
            },
        },
        defaults: PathSettings {
            repos: "/h/repos".into(),
            worktrees: "/h/worktrees".into(),
        },
    };
    let back: NodeConfigInfo =
        serde_json::from_value(serde_json::to_value(&info).unwrap()).unwrap();
    assert_eq!(back, info);

    let status: ForgeStatus = serde_json::from_value(json!({
        "available": true, "authenticated": false, "account": null, "owners": [], "error": "not logged in"
    }))
    .unwrap();
    assert!(status.available && !status.authenticated);
}

#[test]
fn plugin_types_roundtrip() {
    use asterism_proto::rpc::{ErrorKind, RpcError};
    use asterism_proto::types::*;
    use serde_json::json;

    let info = PluginInfo {
        name: "github".into(),
        version: Some("0.2.0".into()),
        description: "GitHub".into(),
        origin: PluginOrigin::Builtin,
        path: "/bin".into(),
        capabilities: vec![Capability {
            kind: CapabilityKind::Forge,
            id: "github".into(),
            description: String::new(),
        }],
        permissions: vec!["network".into()],
        state: PluginState::NeedsSetup {
            missing: vec!["Token".into()],
        },
        backend: Some(vec!["/bin/asterism-plugin-github".into()]),
        store: None,
        update_available: false,
        previous_version: None,
        panels: Vec::new(),
    };
    let value = serde_json::to_value(&info).unwrap();
    assert_eq!(
        value["state"],
        json!({"state": "needs_setup", "missing": ["Token"]})
    );
    assert_eq!(value["capabilities"][0]["kind"], "forge");
    assert_eq!(serde_json::from_value::<PluginInfo>(value).unwrap(), info);

    let spec: SettingSpec = serde_json::from_value(
        json!({"key": "api_key", "title": "API key", "type": "secret", "required": true}),
    )
    .unwrap();
    assert_eq!(spec.kind, SettingType::Secret);
    assert!(spec.options.is_empty() && spec.default.is_none());

    let event = Event::PluginsChanged {};
    assert_eq!(event.to_notification().method, "plugins.changed");
    assert_eq!(
        Event::from_notification(&event.to_notification()),
        Some(event)
    );

    // Plain JSON-RPC errors from third-party plugins carry no `data`.
    let bare: RpcError =
        serde_json::from_value(json!({"code": -32000, "message": "boom"})).unwrap();
    assert_eq!(bare.kind(), ErrorKind::Unknown);
    assert_eq!(ErrorKind::PluginError.code(), -32009);
}

#[test]
fn store_types_roundtrip() {
    use asterism_proto::rpc::ErrorKind;
    use asterism_proto::types::*;
    use serde_json::json;

    let list = StoreList {
        auto_update: true,
        stores: vec![StoreInfo {
            name: "asterism-dev".into(),
            source: "https://example.com/s.git".into(),
            official: true,
            last_refreshed: Some(1_700_000_000),
            last_error: None,
            plugin_count: 2,
        }],
        error: None,
    };
    assert_eq!(
        serde_json::from_value::<StoreList>(serde_json::to_value(&list).unwrap()).unwrap(),
        list
    );

    let search: PluginSearchParams =
        serde_json::from_value(json!({"capability": "task_source"})).unwrap();
    assert_eq!(
        (search.query, search.capability, search.store),
        (None, Some(CapabilityKind::TaskSource), None)
    );
    let update: PluginUpdateParams = serde_json::from_value(json!({"name": "gitlab"})).unwrap();
    assert_eq!(update.accept_permissions, None);
    let remove: StoreRemoveParams = serde_json::from_value(json!({"name": "acme"})).unwrap();
    assert!(!remove.uninstall_plugins);

    // Older daemons' PluginInfo has no store fields.
    let info: PluginInfo = serde_json::from_value(json!({
        "name": "github", "version": "0.2.0", "description": "", "origin": "installed", "path": "/p",
        "capabilities": [], "permissions": [], "state": {"state": "disabled"}, "backend": null
    }))
    .unwrap();
    assert_eq!(
        (info.origin, info.state, info.store, info.update_available),
        (PluginOrigin::Installed, PluginState::Disabled, None, false)
    );

    let event = Event::StoresChanged {};
    assert_eq!(event.to_notification().method, "stores.changed");
    assert_eq!(ErrorKind::PermissionsChanged.code(), -32010);
}

#[test]
fn needs_setup_has_its_own_code() {
    let e = RpcError::new(ErrorKind::NeedsSetup, "linear: API key is not set");
    let v = serde_json::to_value(&e).unwrap();
    assert_eq!(v["code"], -32011);
    assert_eq!(v["data"]["kind"], "needs_setup");
}

#[test]
fn tasks_carry_an_optional_issue() {
    let mut task = sample_task();
    assert_eq!(
        serde_json::to_value(&task).unwrap()["issue"],
        serde_json::Value::Null
    );
    task.issue = Some(IssueRef {
        source: "linear".into(),
        key: "TRA-1".into(),
        url: "https://linear.app/x/issue/TRA-1".into(),
    });
    let back: Task = serde_json::from_value(serde_json::to_value(&task).unwrap()).unwrap();
    assert_eq!(back, task);
    let mut old = serde_json::to_value(sample_task()).unwrap();
    old.as_object_mut().unwrap().remove("issue");
    assert_eq!(serde_json::from_value::<Task>(old).unwrap().issue, None);
}

#[test]
fn task_create_accepts_an_issue_and_defaults_it() {
    let p: TaskCreateParams =
        serde_json::from_value(json!({"project_id": 1, "title": "x"})).unwrap();
    assert_eq!(p.issue, None);
    let p: TaskCreateParams = serde_json::from_value(json!({
        "project_id": 1, "title": "", "issue": {"source": "github-issues", "key": "#4", "title": "T", "url": "u"}
    }))
    .unwrap();
    assert_eq!(p.issue.unwrap().branch, None);
}

#[test]
fn task_source_params_default_query_and_assignment() {
    let p: TaskSourceSearchParams =
        serde_json::from_value(json!({"project_id": 1, "source": "linear"})).unwrap();
    assert_eq!((p.query.as_str(), p.assigned_to_me), ("", false));
    let l: TaskSourceListParams = serde_json::from_value(json!({})).unwrap();
    assert_eq!(l.project_id, None);
    let hit: IssueHit =
        serde_json::from_value(json!({"key": "#1", "title": "t", "url": "u", "state": "open"}))
            .unwrap();
    assert_eq!((hit.assignee, hit.updated_at), (None, None));
}

#[test]
fn pull_requests_use_snake_case_states() {
    let pr = PullRequest {
        number: 7,
        url: "https://github.com/acme/api/pull/7".into(),
        title: "Fix login".into(),
        state: PrState::Draft,
        review: ReviewState::ChangesRequested,
        checks: PrChecks {
            state: ChecksState::Failure,
            failing: vec!["lint".into()],
        },
    };
    let v = serde_json::to_value(&pr).unwrap();
    assert_eq!(
        (
            v["state"].as_str(),
            v["review"].as_str(),
            v["checks"]["state"].as_str()
        ),
        (Some("draft"), Some("changes_requested"), Some("failure"))
    );
    assert_eq!(serde_json::from_value::<PullRequest>(v).unwrap(), pr);
    let none: PrChecks = serde_json::from_value(json!({"state": "none"})).unwrap();
    assert!(none.failing.is_empty());
}

#[test]
fn pr_changed_events_round_trip_with_and_without_a_pr() {
    let cleared = Event::PrChanged {
        task_id: 3,
        pr: None,
    };
    let n = cleared.to_notification();
    assert_eq!(n.method, "pr.changed");
    assert_eq!(n.params, json!({"task_id": 3, "pr": null}));
    assert_eq!(Event::from_notification(&n), Some(cleared));
    let list: PrList = serde_json::from_value(
        json!({"prs": [], "errors": [{"project_id": 1, "message": "gh: not logged in"}]}),
    )
    .unwrap();
    assert_eq!(list.errors[0].project_id, 1);
    let p: PrListParams = serde_json::from_value(json!({})).unwrap();
    assert_eq!(p.project_id, None);
}

#[test]
fn older_clients_omit_base_and_default_base() {
    let params: TaskCreateParams = serde_json::from_str(r#"{"project_id":1,"title":"t"}"#).unwrap();
    assert_eq!(params.base, None);
    let project: Project = serde_json::from_str(r#"{"id":1,"name":"n","path":"/p"}"#).unwrap();
    assert_eq!(project.default_base, None);
}

#[test]
fn subagent_events_round_trip() {
    let subagent = Subagent {
        id: "toolu_1".into(),
        parent_id: None,
        kind: "Explore".into(),
        description: "find callers".into(),
        status: SubagentStatus::Running,
        started_at: 10,
        ended_at: None,
    };
    let started = Event::SubagentStarted {
        session_id: 3,
        subagent: subagent.clone(),
    };
    let n = started.to_notification();
    assert_eq!(n.method, "subagent.started");
    assert_eq!(n.params["subagent"]["status"], "running");
    assert_eq!(Event::from_notification(&n), Some(started));
    let updated = Event::SubagentUpdated {
        session_id: 3,
        subagent: Subagent {
            status: SubagentStatus::Done,
            ended_at: Some(12),
            ..subagent
        },
    };
    assert_eq!(
        Event::from_notification(&updated.to_notification()),
        Some(updated)
    );
}

#[test]
fn hook_params_carry_an_optional_subagent() {
    let old: SessionHookParams =
        serde_json::from_value(json!({"session_id": 1, "event": "tool"})).unwrap();
    assert_eq!(old.subagent, None);
    let p: SessionHookParams = serde_json::from_value(json!({
        "session_id": 1, "event": "subagent-start",
        "subagent": {"id": "a", "kind": "Explore", "description": "d"}
    }))
    .unwrap();
    assert_eq!(p.event, HookEvent::SubagentStart);
    let s = p.subagent.unwrap();
    assert_eq!((s.id.as_str(), s.parent_id, s.failed), ("a", None, false));
    assert_eq!(s.alias, None);
    let aliased: SubagentHook = serde_json::from_value(json!({"id": "t1", "alias": "a1"})).unwrap();
    assert_eq!(aliased.alias.as_deref(), Some("a1"));
    assert_eq!(serde_json::to_value(&aliased).unwrap()["alias"], "a1");
}

#[test]
fn plugin_info_defaults_to_no_panels_and_panels_are_a_capability() {
    let info: PluginInfo = serde_json::from_value(json!({
        "name": "x", "version": "1.0.0", "description": "", "origin": "builtin", "path": "/p",
        "capabilities": [{"kind": "panel", "id": "agents"}], "permissions": [],
        "state": {"state": "ok"}, "backend": null
    }))
    .unwrap();
    assert!(info.panels.is_empty());
    assert_eq!(info.capabilities[0].kind, CapabilityKind::Panel);
}

#[test]
fn review_types_use_snake_case_on_the_wire() {
    let params = ReviewCommentParams {
        task_id: 1,
        source: ReviewSource::Pr,
        path: "src/a.rs".into(),
        line: 3,
        side: DiffSide::New,
        body: "hi".into(),
        target: CommentTarget::Single,
    };
    let v = serde_json::to_value(&params).unwrap();
    assert_eq!(v["source"], "pr");
    assert_eq!(v["side"], "new");
    assert_eq!(v["target"], "single");
    assert_eq!(
        serde_json::to_value(ReviewEvent::RequestChanges).unwrap(),
        "request_changes"
    );
    let event = Event::ReviewChanged { task_id: 4 };
    let n = serde_json::to_value(&event).unwrap();
    assert_eq!(n["method"], "review.changed");
    assert_eq!(n["params"]["task_id"], 4);
    let thread: ReviewThread = serde_json::from_value(serde_json::json!({
        "id": "t", "path": "a", "line": 1, "side": "old", "outdated": false,
        "resolved": false, "comments": []
    }))
    .unwrap();
    assert!(!thread.local && !thread.pending);
}

#[test]
fn check_and_conversation_types_use_snake_case() {
    let check: CheckRun = serde_json::from_value(serde_json::json!({
        "id": "11", "name": "test", "status": "running", "url": "https://x"
    }))
    .unwrap();
    assert_eq!(check.status, CheckStatus::Running);
    assert!(
        check.conclusion.is_none()
            && !check.has_log
            && !check.rerunnable
            && check.workflow.is_empty()
    );
    let item = ConversationItem {
        id: "R1".into(),
        kind: ConversationKind::Review,
        author: "alice".into(),
        body: String::new(),
        created_at: "2026-10-09T10:00:00Z".into(),
        state: Some("approved".into()),
    };
    let v = serde_json::to_value(&item).unwrap();
    assert_eq!(
        (v["kind"].as_str(), v["state"].as_str()),
        (Some("review"), Some("approved"))
    );
    // Older forge payloads without the new fields still parse.
    let old: ForgeReview = serde_json::from_value(serde_json::json!({
        "head_sha": "h", "base_sha": "", "head_ref": "r", "threads": [], "conversation": [],
        "viewed_files": [], "pending_review": null
    }))
    .unwrap();
    assert!(old.checks.is_empty() && old.title.is_empty() && old.commit_checks.is_empty());
    let diff = ReviewCommitDiffParams {
        task_id: 1,
        source: ReviewSource::Local,
        sha: None,
    };
    assert_eq!(
        serde_json::to_value(&diff).unwrap()["sha"],
        serde_json::Value::Null
    );
}
