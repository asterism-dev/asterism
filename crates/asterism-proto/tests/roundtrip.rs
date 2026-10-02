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
    }
}

#[test]
fn server_messages_decode_by_shape() {
    let resp: ServerMessage =
        serde_json::from_value(json!({"jsonrpc": "2.0", "id": 7, "result": {"ok": true}})).unwrap();
    assert_eq!(resp, ServerMessage::Response(Response::ok(7, json!({"ok": true}))));

    let note: ServerMessage = serde_json::from_value(json!({
        "jsonrpc": "2.0",
        "method": "session.status_changed",
        "params": {"session_id": 3, "status": "idle"}
    }))
    .unwrap();
    let ServerMessage::Notification(n) = note else { panic!("expected a notification") };
    assert_eq!(
        Event::from_notification(&n),
        Some(Event::SessionStatusChanged { session_id: 3, status: SessionStatus::Idle })
    );
}

#[test]
fn events_roundtrip_through_notifications() {
    let events = [
        Event::SessionOutput { session_id: 1, data: "aGk=".into() },
        Event::TaskChanged(sample_task()),
        Event::ProjectRemoved { project_id: 4 },
    ];
    for event in events {
        assert_eq!(Event::from_notification(&event.to_notification()), Some(event.clone()));
    }
}

#[test]
fn session_kind_is_tagged_by_type() {
    assert_eq!(
        serde_json::to_value(SessionKind::Agent { name: "claude".into() }).unwrap(),
        json!({"type": "agent", "name": "claude"})
    );
    assert_eq!(serde_json::to_value(SessionKind::Shell).unwrap(), json!({"type": "shell"}));
    assert_eq!(serde_json::to_value(SessionStatus::WaitingInput).unwrap(), json!("waiting_input"));
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
