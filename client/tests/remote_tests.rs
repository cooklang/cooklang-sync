//! Integration tests for `cooklang_sync_client::remote::Remote`.
//!
//! Every test spins up a fresh `wiremock::MockServer` and points a `Remote`
//! at its URL. Tests assert on URL shape, headers, and body — *not* on the
//! per-instance `uuid` that `Remote` mints at construction.

use cooklang_sync_client::remote::{CommitResultStatus, Remote, ResponseFileRecord};
use futures::StreamExt;
use wiremock::matchers::{
    body_string_contains, header, header_exists, method, path, query_param, query_param_contains,
};
use wiremock::{Mock, MockServer, ResponseTemplate};

const TOKEN: &str = "test-token";

/// Build a `Remote` wired to `server`'s base URL.
fn new_remote(server: &MockServer) -> Remote {
    Remote::new(&server.uri(), TOKEN)
}

#[tokio::test]
async fn commit_returns_success_on_2xx_with_success_payload() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/metadata/commit"))
        .and(header("authorization", format!("Bearer {}", TOKEN).as_str()))
        .and(header_exists("user-agent"))
        .and(header_exists("x-client-version"))
        .and(query_param_contains("uuid", "-")) // v4 UUID always contains hyphens
        .and(body_string_contains("path=recipes%2Fa.cook"))
        .and(body_string_contains("deleted=false"))
        .and(body_string_contains("chunk_ids=abc%2Cdef"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "Success": 42
        })))
        .expect(1)
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let result = remote
        .commit("recipes/a.cook", false, "abc,def")
        .await
        .expect("commit");
    assert!(matches!(result, CommitResultStatus::Success(42)));
}

#[tokio::test]
async fn commit_returns_need_chunks_on_2xx_with_need_chunks_payload() {
    let server = MockServer::start().await;

    Mock::given(method("POST"))
        .and(path("/metadata/commit"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "NeedChunks": "abc,def"
        })))
        .expect(1)
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let result = remote.commit("a.cook", false, "abc,def").await.expect("commit");
    match result {
        CommitResultStatus::NeedChunks(s) => assert_eq!(s, "abc,def"),
        other => panic!("expected NeedChunks, got {:?}", other),
    }
}

#[tokio::test]
async fn commit_maps_401_to_unauthorized() {
    use cooklang_sync_client::errors::SyncError;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/metadata/commit"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let err = remote.commit("a.cook", false, "").await.unwrap_err();
    assert!(
        matches!(err, SyncError::Unauthorized),
        "expected SyncError::Unauthorized on 401, got {:?}",
        err
    );
}

#[tokio::test]
async fn commit_maps_5xx_to_unknown_with_status_in_message() {
    use cooklang_sync_client::errors::SyncError;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/metadata/commit"))
        .respond_with(ResponseTemplate::new(503))
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let err = remote.commit("a.cook", false, "").await.unwrap_err();
    match err {
        SyncError::Unknown(msg) => assert!(msg.contains("503"), "expected status in message, got {msg:?}"),
        other => panic!("expected SyncError::Unknown on 5xx, got {:?}", other),
    }
}

#[tokio::test]
async fn list_parses_response_records_and_preserves_order() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .and(query_param("jid", "7"))
        .and(header("authorization", format!("Bearer {}", TOKEN).as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 8, "path": "a.cook", "deleted": false, "chunk_ids": "abc" },
            { "id": 9, "path": "b.cook", "deleted": true,  "chunk_ids": "" }
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let records: Vec<ResponseFileRecord> = remote.list(7).await.expect("list");
    assert_eq!(records.len(), 2);
    assert_eq!(records[0].id, 8);
    assert_eq!(records[0].path, "a.cook");
    assert!(!records[0].deleted);
    assert_eq!(records[0].chunk_ids, "abc");
    assert_eq!(records[1].id, 9);
    assert!(records[1].deleted);
}

#[tokio::test]
async fn list_returns_empty_vec_on_empty_json_array() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let records = remote.list(0).await.expect("list");
    assert!(records.is_empty());
}

#[tokio::test]
async fn list_maps_401_to_unauthorized() {
    use cooklang_sync_client::errors::SyncError;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let err = remote.list(0).await.unwrap_err();
    assert!(matches!(err, SyncError::Unauthorized));
}

#[tokio::test]
async fn poll_returns_ok_on_200() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metadata/poll"))
        .and(query_param_contains("uuid", "-"))
        .and(header("authorization", format!("Bearer {}", TOKEN).as_str()))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    remote.poll().await.expect("poll should succeed on 200");
}

#[tokio::test]
async fn poll_maps_401_to_unauthorized() {
    use cooklang_sync_client::errors::SyncError;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metadata/poll"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let err = remote.poll().await.unwrap_err();
    assert!(matches!(err, SyncError::Unauthorized));
}

#[tokio::test]
async fn poll_treats_client_timeout_as_ok() {
    use cooklang_sync_client::remote::REQUEST_TIMEOUT_SECS;
    use std::time::Duration;

    let server = MockServer::start().await;
    // Respond *after* the client's request timeout expires.
    Mock::given(method("GET"))
        .and(path("/metadata/poll"))
        .respond_with(
            ResponseTemplate::new(200)
                .set_delay(Duration::from_secs(REQUEST_TIMEOUT_SECS + 5)),
        )
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    // `poll` deliberately swallows reqwest::Error::is_timeout and returns Ok(()).
    remote.poll().await.expect("timeout should be mapped to Ok(())");
}

#[tokio::test]
async fn upload_posts_raw_body_to_chunk_path() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chunks/abc123"))
        .and(header("authorization", format!("Bearer {}", TOKEN).as_str()))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    remote.upload("abc123", b"hello".to_vec()).await.expect("upload");
}

#[tokio::test]
async fn upload_batch_posts_multipart_with_each_chunk_as_named_part() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chunks/upload"))
        .and(header("authorization", format!("Bearer {}", TOKEN).as_str()))
        // Content-Type includes the generated boundary.
        .and(header_exists("content-type"))
        // Each chunk is a form-data part whose `name=` is its chunk_id.
        .and(body_string_contains(r#"Content-Disposition: form-data; name="c1""#))
        .and(body_string_contains(r#"Content-Disposition: form-data; name="c2""#))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let chunks = vec![
        ("c1".to_string(), b"hello".to_vec()),
        ("c2".to_string(), b"world".to_vec()),
    ];
    remote.upload_batch(chunks).await.expect("upload_batch");
}

#[tokio::test]
async fn upload_batch_maps_401_to_unauthorized() {
    use cooklang_sync_client::errors::SyncError;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chunks/upload"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let err = remote.upload_batch(vec![("c1".into(), b"x".to_vec())]).await.unwrap_err();
    assert!(matches!(err, SyncError::Unauthorized));
}

#[tokio::test]
async fn download_returns_body_bytes_on_200() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chunks/xyz"))
        .and(header("authorization", format!("Bearer {}", TOKEN).as_str()))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"payload".to_vec()))
        .expect(1)
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let bytes = remote.download("xyz").await.expect("download");
    assert_eq!(bytes, b"payload");
}

#[tokio::test]
async fn download_maps_401_to_unauthorized() {
    use cooklang_sync_client::errors::SyncError;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/chunks/xyz"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let err = remote.download("xyz").await.unwrap_err();
    assert!(matches!(err, SyncError::Unauthorized));
}

#[tokio::test]
async fn download_batch_streams_parts_keyed_by_x_chunk_id_header() {
    let server = MockServer::start().await;

    // Hand-assemble a tiny multipart body that mirrors the server's format.
    // The parser in remote.rs looks for `--{boundary}` separators, then a
    // `X-Chunk-ID:` line inside the part headers, then the bytes up to the
    // next boundary.
    let boundary = "testboundary";
    let body = format!(
        "--{b}\r\n\
         X-Chunk-ID: c1\r\n\
         Content-Type: application/octet-stream\r\n\r\n\
         hello\r\n\
         --{b}\r\n\
         X-Chunk-ID: c2\r\n\
         Content-Type: application/octet-stream\r\n\r\n\
         world\r\n\
         --{b}--\r\n",
        b = boundary
    );

    Mock::given(method("POST"))
        .and(path("/chunks/download"))
        .and(body_string_contains("chunk_ids%5B%5D=c1"))
        .and(body_string_contains("chunk_ids%5B%5D=c2"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", format!("multipart/form-data; boundary={}", boundary).as_str())
                .set_body_bytes(body.into_bytes()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let mut stream = remote.download_batch(vec!["c1", "c2"]).await;

    let mut got: Vec<(String, Vec<u8>)> = Vec::new();
    while let Some(item) = stream.next().await {
        got.push(item.expect("part ok"));
    }

    // Order is not guaranteed by the parser, so sort by chunk id.
    got.sort_by(|a, b| a.0.cmp(&b.0));
    assert_eq!(got.len(), 2);
    assert_eq!(got[0].0, "c1");
    assert_eq!(got[0].1, b"hello");
    assert_eq!(got[1].0, "c2");
    assert_eq!(got[1].1, b"world");
}

#[tokio::test]
async fn download_batch_errors_when_content_type_is_missing() {
    use cooklang_sync_client::errors::SyncError;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/chunks/download"))
        .respond_with(ResponseTemplate::new(200).set_body_bytes(b"whatever".to_vec()))
        .mount(&server)
        .await;

    let remote = new_remote(&server);
    let mut stream = remote.download_batch(vec!["c1"]).await;
    let first = stream.next().await.expect("at least one item").unwrap_err();
    assert!(
        matches!(first, SyncError::BatchDownloadError(_)),
        "expected BatchDownloadError on missing content-type, got {:?}",
        first
    );
}
