//! Integration tests for `cooklang_sync_client::remote::Remote`.
//!
//! Every test spins up a fresh `wiremock::MockServer` and points a `Remote`
//! at its URL. Tests assert on URL shape, headers, and body — *not* on the
//! per-instance `uuid` that `Remote` mints at construction.

use cooklang_sync_client::remote::{CommitResultStatus, Remote};
use wiremock::matchers::{
    body_string_contains, header, header_exists, method, path, query_param_contains,
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
