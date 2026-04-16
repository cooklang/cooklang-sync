//! Integration tests for `cooklang_sync_client::remote::Remote`.
//!
//! Every test spins up a fresh `wiremock::MockServer` and points a `Remote`
//! at its URL. Tests assert on URL shape, headers, and body — *not* on the
//! per-instance `uuid` that `Remote` mints at construction.

use cooklang_sync_client::remote::{CommitResultStatus, Remote, ResponseFileRecord};
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
