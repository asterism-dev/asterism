use asterism_plugin::{serve_io, ErrorKind, Host, RpcError};
use asterism_proto::rpc::{Request, Response};
use serde_json::{json, Value};
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

async fn send(write: &mut (impl AsyncWriteExt + Unpin), value: Value) {
    write
        .write_all(format!("{value}\n").as_bytes())
        .await
        .unwrap();
}

#[tokio::test]
async fn answers_requests_and_reaches_the_host_api() {
    let (daemon_side, plugin_side) = tokio::io::duplex(64 * 1024);
    let (plugin_read, plugin_write) = tokio::io::split(plugin_side);
    tokio::spawn(serve_io(
        plugin_read,
        plugin_write,
        |host: Host, method: String, params: Value| async move {
            match method.as_str() {
                "echo" => Ok(params),
                "ask" => host.call::<_, Value>("project.list", Value::Null).await,
                other => Err(RpcError::new(ErrorKind::MethodNotFound, other.to_string())),
            }
        },
    ));
    let (read, mut write) = tokio::io::split(daemon_side);
    let mut lines = BufReader::new(read).lines();

    send(
        &mut write,
        json!({"jsonrpc": "2.0", "id": 1, "method": "echo", "params": {"a": 1}}),
    )
    .await;
    let reply: Response = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!((reply.id, reply.result), (1, Some(json!({"a": 1}))));

    send(
        &mut write,
        json!({"jsonrpc": "2.0", "id": 2, "method": "ask", "params": null}),
    )
    .await;
    let host_request: Request =
        serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(host_request.method, "host.project.list");
    send(
        &mut write,
        serde_json::to_value(Response::ok(host_request.id, json!([{"id": 7}]))).unwrap(),
    )
    .await;
    let reply: Response = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!((reply.id, reply.result), (2, Some(json!([{"id": 7}]))));
}

#[tokio::test]
async fn garbage_is_skipped_and_errors_are_returned() {
    let (daemon_side, plugin_side) = tokio::io::duplex(64 * 1024);
    let (plugin_read, plugin_write) = tokio::io::split(plugin_side);
    tokio::spawn(serve_io(
        plugin_read,
        plugin_write,
        |_host: Host, method: String, _params: Value| async move {
            Err::<Value, _>(RpcError::new(
                ErrorKind::MethodNotFound,
                format!("unknown method {method}"),
            ))
        },
    ));
    let (read, mut write) = tokio::io::split(daemon_side);
    let mut lines = BufReader::new(read).lines();
    write.write_all(b"not json\n").await.unwrap();
    send(
        &mut write,
        json!({"jsonrpc": "2.0", "id": 5, "method": "nope", "params": null}),
    )
    .await;
    let reply: Response = serde_json::from_str(&lines.next_line().await.unwrap().unwrap()).unwrap();
    assert_eq!(reply.id, 5);
    assert_eq!(reply.error.unwrap().kind(), ErrorKind::MethodNotFound);
}
