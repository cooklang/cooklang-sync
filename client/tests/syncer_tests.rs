//! Integration tests for `syncer::check_upload_once` and `check_download_once`.
//!
//! Drives each function with:
//! * real SQLite pool via `common::fresh_client_pool()`
//! * real `Chunker` rooted at a tempdir (`common::client_base()`)
//! * `wiremock::MockServer` posing as the remote
//!
//! Tests pin observable side effects (DB rows, files on disk, HTTP requests
//! the server actually received) rather than internals.

mod common;

use cooklang_sync_client::connection::get_connection;
use cooklang_sync_client::models::{CreateForm, DeleteForm, FileRecord};
use cooklang_sync_client::registry;
use cooklang_sync_client::remote::Remote;
use cooklang_sync_client::syncer::check_upload_once;
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use wiremock::matchers::{method, path};
use wiremock::{Mock, MockServer, ResponseTemplate};

const NS: i32 = 1;
const TOKEN: &str = "test-token";

fn sample_create(path: &str, size: i64) -> CreateForm {
    CreateForm {
        jid: None,
        path: path.to_string(),
        deleted: false,
        size,
        modified_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        namespace_id: NS,
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_upload_once_commits_success_and_marks_jid() {
    let server = MockServer::start().await;
    // Always return Success(100). No NeedChunks branch => no upload_batch.
    Mock::given(method("POST"))
        .and(path("/metadata/commit"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "Success": 100 })))
        .expect(1)
        .mount(&server)
        .await;

    let mut base = common::client_base();
    // Seed a real file so hashify can read it.
    tokio::fs::write(base.dir.path().join("a.cook"), b"Eggs\n").await.expect("write file");
    // Seed a registry row with jid=None so `updated_locally` picks it up.
    {
        let conn = &mut get_connection(&base.pool).expect("checkout");
        registry::create(conn, &vec![sample_create("a.cook", 5)]).expect("create");
    }

    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(&mut base.chunker));
    let all_committed = check_upload_once(&base.pool, Arc::clone(&chunker_arc), &remote, NS)
        .await
        .expect("check_upload_once");
    assert!(all_committed, "all rows should commit in one pass");

    // Row now has the returned jid.
    let conn = &mut get_connection(&base.pool).expect("checkout");
    let after: Vec<FileRecord> = registry::updated_locally(conn, NS).expect("updated_locally");
    assert!(after.is_empty(), "no rows should remain unsynced after Success");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_upload_once_triggers_upload_batch_when_server_asks_for_chunks() {
    let server = MockServer::start().await;

    // First /metadata/commit returns NeedChunks. We don't care which chunk ids
    // are listed — we just need *some* comma-separated ids for the client to
    // push through. To keep it simple, we echo back "CHUNK1" and expect the
    // client to attempt to read that chunk from its cache (the hashify call
    // warms the cache, so the id we echo must be one of the hashes the
    // chunker computed). Workaround: the test file is *text*, so only one
    // chunk id is produced; we read it out, then program the mock to return
    // exactly that id in NeedChunks.
    //
    // This two-phase mocking is unavoidable because chunk ids are content-
    // derived and we don't want to hard-code them.

    let mut base = common::client_base();
    tokio::fs::write(base.dir.path().join("a.cook"), b"Eggs\n").await.expect("write file");
    let computed_ids = base.chunker.hashify("a.cook").await.expect("hashify");
    assert!(!computed_ids.is_empty(), "text chunker must produce ids");
    let chunk_id = computed_ids.first().expect("first id").clone();

    Mock::given(method("POST"))
        .and(path("/metadata/commit"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({
            "NeedChunks": chunk_id.clone()
        })))
        .expect(1)
        .mount(&server)
        .await;

    Mock::given(method("POST"))
        .and(path("/chunks/upload"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&server)
        .await;

    {
        let conn = &mut get_connection(&base.pool).expect("checkout");
        registry::create(conn, &vec![sample_create("a.cook", 5)]).expect("create");
    }

    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(&mut base.chunker));
    let all_committed = check_upload_once(&base.pool, Arc::clone(&chunker_arc), &remote, NS)
        .await
        .expect("check_upload_once");
    // NeedChunks path means we did *not* fully commit this pass — caller will
    // retry on the next loop iteration (that's why `lib::run_upload_once`
    // calls this function twice).
    assert!(!all_committed, "NeedChunks path should return false");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_upload_once_commits_tombstone_without_hashifying() {
    use wiremock::matchers::body_string_contains;

    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/metadata/commit"))
        .and(body_string_contains("deleted=true"))
        .and(body_string_contains("chunk_ids=&")) // empty chunk_ids, followed by next field
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "Success": 7 })))
        .expect(1)
        .mount(&server)
        .await;

    let mut base = common::client_base();
    // Deliberately do NOT write the file to disk.
    {
        let conn = &mut get_connection(&base.pool).expect("checkout");
        // Seed a first record + a tombstone that supersedes it. `updated_locally`
        // returns the latest (id-max) row per path.
        registry::create(conn, &vec![sample_create("gone.cook", 10)]).expect("create");
        let live: Vec<FileRecord> = registry::non_deleted(conn, NS).expect("non_deleted");
        let latest = live.first().expect("live row").clone();
        let tombstone = DeleteForm {
            path: latest.path.clone(),
            jid: None,
            size: 0,
            modified_at: latest.modified_at,
            deleted: true,
            namespace_id: NS,
        };
        registry::delete(conn, &vec![tombstone]).expect("delete");
    }

    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(&mut base.chunker));
    let ok = check_upload_once(&base.pool, Arc::clone(&chunker_arc), &remote, NS)
        .await
        .expect("check_upload_once");
    assert!(ok, "tombstone commit is a Success => all_commited stays true");
}
