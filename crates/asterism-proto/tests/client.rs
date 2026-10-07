use std::time::Duration;

use asterism_proto::client::{Client, ClientError};
use asterism_proto::rpc::ErrorKind;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

/// Answers each request line with the next canned reply.
fn fake_server(replies: &'static [&'static str]) -> Client {
    let (client_side, server_side) = tokio::io::duplex(4096);
    let (client_read, client_write) = tokio::io::split(client_side);
    let (server_read, mut server_write) = tokio::io::split(server_side);
    tokio::spawn(async move {
        let mut lines = BufReader::new(server_read).lines();
        for reply in replies {
            if lines.next_line().await.ok().flatten().is_none() {
                return;
            }
            let _ = server_write
                .write_all(format!("{reply}\n").as_bytes())
                .await;
        }
        // Keep the connection open so a hang would show as a timeout, not a close.
        while let Ok(Some(_)) = lines.next_line().await {}
    });
    Client::new(client_read, client_write)
}

#[tokio::test]
async fn unknown_error_kinds_and_garbage_responses_fail_the_call() {
    let client = fake_server(&[
        r#"{"jsonrpc":"2.0","id":1,"error":{"code":-1,"message":"x","data":{"kind":"brand_new_kind"}}}"#,
        r#"{"jsonrpc":"2.0","id":2,"result":5,"error":"bogus"}"#,
    ]);
    let first = tokio::time::timeout(Duration::from_secs(2), client.call::<_, ()>("a", ()))
        .await
        .unwrap();
    match first {
        Err(ClientError::Rpc(e)) => assert_eq!(e.kind(), ErrorKind::Unknown),
        other => panic!("expected rpc error, got {other:?}"),
    }
    let second = tokio::time::timeout(Duration::from_secs(2), client.call::<_, ()>("b", ()))
        .await
        .unwrap();
    assert!(
        matches!(second, Err(ClientError::Protocol(_))),
        "got {second:?}"
    );
}
