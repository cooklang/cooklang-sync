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

use cooklang_sync_client::chunker::{Chunker, InMemoryCache};
use cooklang_sync_client::connection::get_connection;
use cooklang_sync_client::errors::SyncError;
use cooklang_sync_client::models::{CreateForm, DeleteForm, FileRecord};
use cooklang_sync_client::registry;
use cooklang_sync_client::remote::Remote;
use cooklang_sync_client::syncer::{check_download_once, check_upload_once};
use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use wiremock::matchers::{body_string_contains, method, path};
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

    let base = common::client_base();
    // Seed a real file so hashify can read it.
    tokio::fs::write(base.dir.path().join("a.cook"), b"Eggs\n").await.expect("write file");
    // Seed a registry row with jid=None so `updated_locally` picks it up.
    {
        let conn = &mut get_connection(&base.pool).expect("checkout");
        registry::create(conn, &[sample_create("a.cook", 5)]).expect("create");
    }

    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
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
        registry::create(conn, &[sample_create("a.cook", 5)]).expect("create");
    }

    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
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
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/metadata/commit"))
        .and(body_string_contains("deleted=true"))
        .and(body_string_contains("chunk_ids=&")) // empty chunk_ids, followed by next field
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!({ "Success": 7 })))
        .expect(1)
        .mount(&server)
        .await;

    let base = common::client_base();
    // Deliberately do NOT write the file to disk.
    {
        let conn = &mut get_connection(&base.pool).expect("checkout");
        // Seed a first record + a tombstone that supersedes it. `updated_locally`
        // returns the latest (id-max) row per path.
        registry::create(conn, &[sample_create("gone.cook", 10)]).expect("create");
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
        registry::delete(conn, &[tombstone]).expect("delete");
    }

    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
    let ok = check_upload_once(&base.pool, Arc::clone(&chunker_arc), &remote, NS)
        .await
        .expect("check_upload_once");
    assert!(ok, "tombstone commit is a Success => all_commited stays true");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_download_once_writes_file_and_inserts_registry_row() {
    let server = MockServer::start().await;

    // We need two mocks to be wired in a particular order:
    //   1. GET /metadata/list?jid=0 → return one record referencing chunk "c1"
    //   2. POST /chunks/download    → stream back a multipart body containing c1
    // The chunk id must be content-derived so the chunker accepts the save.
    // Easiest: we hashify a known text file *first* in a scratch chunker rooted
    // at a throwaway dir, grab the chunk id, then drive the real download against
    // a fresh base.
    let scratch_dir = tempfile::TempDir::new().unwrap();
    tokio::fs::write(scratch_dir.path().join("a.cook"), b"Eggs\n").await.unwrap();
    let scratch_cache = InMemoryCache::new(10, 1_000_000);
    let mut scratch = Chunker::new(scratch_cache, scratch_dir.path().to_path_buf());
    let ids = scratch.hashify("a.cook").await.unwrap();
    assert_eq!(ids.len(), 1, "text file produces one line-per-chunk id");
    let chunk_id = ids[0].clone();

    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 11, "path": "a.cook", "deleted": false, "chunk_ids": chunk_id }
        ])))
        .expect(1)
        .mount(&server)
        .await;

    let boundary = "downloadbound";
    let body = format!(
        "--{b}\r\nX-Chunk-ID: {id}\r\nContent-Type: application/octet-stream\r\n\r\nEggs\n\r\n--{b}--\r\n",
        b = boundary,
        id = chunk_id
    );
    Mock::given(method("POST"))
        .and(path("/chunks/download"))
        .respond_with(
            ResponseTemplate::new(200)
                .insert_header("content-type", format!("multipart/form-data; boundary={}", boundary).as_str())
                .set_body_bytes(body.into_bytes()),
        )
        .expect(1)
        .mount(&server)
        .await;

    let base = common::client_base();
    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
    let downloaded = check_download_once(
        &base.pool,
        Arc::clone(&chunker_arc),
        &remote,
        base.dir.path(),
        NS,
    )
    .await
    .expect("check_download_once");
    assert!(downloaded, "non-empty remote list => returns true");

    // File was written.
    let bytes = tokio::fs::read(base.dir.path().join("a.cook")).await.expect("read");
    assert_eq!(bytes, b"Eggs\n");

    // Registry has a live row with jid=11.
    let conn = &mut get_connection(&base.pool).expect("checkout");
    let rows = registry::non_deleted(conn, NS).expect("non_deleted");
    assert_eq!(rows.len(), 1);
    assert_eq!(rows[0].path, "a.cook");
    assert_eq!(rows[0].jid, Some(11));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_download_once_removes_local_file_and_appends_tombstone() {
    let server = MockServer::start().await;

    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 22, "path": "gone.cook", "deleted": true, "chunk_ids": "" }
        ])))
        .expect(1)
        .mount(&server)
        .await;
    // No /chunks/download is expected — deleted records skip the download queue.

    let base = common::client_base();
    tokio::fs::write(base.dir.path().join("gone.cook"), b"bye\n").await.unwrap();
    {
        let conn = &mut get_connection(&base.pool).expect("checkout");
        registry::create(conn, &[sample_create("gone.cook", 4)]).expect("create");
    }

    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
    check_download_once(
        &base.pool,
        Arc::clone(&chunker_arc),
        &remote,
        base.dir.path(),
        NS,
    )
    .await
    .expect("check_download_once");

    // File is gone on disk.
    assert!(
        !base.dir.path().join("gone.cook").exists(),
        "local file should be deleted when remote says deleted"
    );
    // Registry has a tombstone appended (latest row for this path has deleted=true).
    let conn = &mut get_connection(&base.pool).expect("checkout");
    let live = registry::non_deleted(conn, NS).expect("non_deleted");
    assert!(
        live.iter().all(|r| r.path != "gone.cook"),
        "gone.cook should not be in non_deleted after tombstone applied"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_download_once_empty_remote_list_is_noop() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([])))
        .expect(1)
        .mount(&server)
        .await;
    // No /chunks/download should be hit; wiremock will error on unmatched paths
    // when we assert on a mounted mock. We omit that mock deliberately.

    let base = common::client_base();
    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
    let downloaded = check_download_once(
        &base.pool,
        Arc::clone(&chunker_arc),
        &remote,
        base.dir.path(),
        NS,
    )
    .await
    .expect("check_download_once");
    assert!(!downloaded, "empty list => returns false");

    // Registry is still empty.
    let conn = &mut get_connection(&base.pool).expect("checkout");
    let rows = registry::non_deleted(conn, NS).expect("non_deleted");
    assert!(rows.is_empty());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_download_once_propagates_unauthorized_from_list() {
    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let base = common::client_base();
    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
    let err = check_download_once(
        &base.pool,
        Arc::clone(&chunker_arc),
        &remote,
        base.dir.path(),
        NS,
    )
    .await
    .unwrap_err();
    assert!(
        matches!(err, SyncError::Unauthorized),
        "expected SyncError::Unauthorized, got {:?}",
        err
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_upload_once_propagates_unauthorized_from_commit() {
    let server = MockServer::start().await;
    Mock::given(method("POST"))
        .and(path("/metadata/commit"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let base = common::client_base();
    tokio::fs::write(base.dir.path().join("a.cook"), b"Eggs\n").await.expect("write file");
    {
        let conn = &mut get_connection(&base.pool).expect("checkout");
        registry::create(conn, &[sample_create("a.cook", 5)]).expect("create");
    }

    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
    let err = check_upload_once(&base.pool, Arc::clone(&chunker_arc), &remote, NS)
        .await
        .unwrap_err();
    assert!(
        matches!(err, SyncError::Unauthorized),
        "expected SyncError::Unauthorized, got {:?}",
        err
    );
}

// ---------------------------------------------------------------------------
// Batched downloads
// ---------------------------------------------------------------------------

use cooklang_sync_client::syncer::DOWNLOAD_BATCH_MAX_CHUNKS;
use std::collections::HashMap;
use wiremock::{Request, Respond};

/// Stands in for `POST /chunks/download`: answers with exactly the chunks the
/// request asked for, or with a 500 when the request names `fail_on`.
struct ChunkStore {
    chunks: HashMap<String, Vec<u8>>,
    fail_on: Option<String>,
}

fn requested_chunk_ids(request: &Request) -> Vec<String> {
    String::from_utf8_lossy(&request.body)
        .split('&')
        .filter_map(|pair| pair.split_once('='))
        .map(|(_, id)| id.to_string())
        .collect()
}

impl Respond for ChunkStore {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        let ids = requested_chunk_ids(request);

        if let Some(fail_on) = &self.fail_on {
            if ids.contains(fail_on) {
                return ResponseTemplate::new(500);
            }
        }

        let boundary = "downloadbound";
        let mut body: Vec<u8> = Vec::new();
        for id in ids {
            let content = self.chunks.get(&id).expect("test asked for an unknown chunk");
            body.extend_from_slice(
                format!("--{boundary}\r\nX-Chunk-ID: {id}\r\nContent-Type: application/octet-stream\r\n\r\n")
                    .as_bytes(),
            );
            body.extend_from_slice(content);
            body.extend_from_slice(b"\r\n");
        }
        body.extend_from_slice(format!("--{boundary}--\r\n").as_bytes());

        ResponseTemplate::new(200)
            .insert_header("content-type", format!("multipart/form-data; boundary={boundary}").as_str())
            .set_body_bytes(body)
    }
}

/// Chunk ids of a text file with this content, plus the bytes behind each id.
async fn text_chunks(content: &str) -> (Vec<String>, HashMap<String, Vec<u8>>) {
    let scratch_dir = tempfile::TempDir::new().unwrap();
    tokio::fs::write(scratch_dir.path().join("f.cook"), content).await.unwrap();
    let mut scratch = Chunker::new(InMemoryCache::new(100_000, 100_000_000), scratch_dir.path().to_path_buf());
    let ids = scratch.hashify("f.cook").await.unwrap();
    let chunks = ids
        .iter()
        .map(|id| (id.clone(), scratch.read_chunk(id).unwrap()))
        .collect();
    (ids, chunks)
}

/// A text file of `lines` lines, no two alike, so every line is its own chunk.
fn unique_lines(prefix: &str, lines: usize) -> String {
    (0..lines).map(|i| format!("{prefix} line {i}\n")).collect()
}

async fn download_requests(server: &MockServer) -> Vec<Request> {
    server
        .received_requests()
        .await
        .expect("request recording is on")
        .into_iter()
        .filter(|r| r.url.path() == "/chunks/download")
        .collect()
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_download_once_splits_a_large_listing_into_several_requests() {
    let server = MockServer::start().await;

    // Two files that together exceed what one request may ask for.
    let lines = DOWNLOAD_BATCH_MAX_CHUNKS * 3 / 4;
    let content_a = unique_lines("a", lines);
    let content_b = unique_lines("b", lines);
    let (ids_a, mut chunks) = text_chunks(&content_a).await;
    let (ids_b, chunks_b) = text_chunks(&content_b).await;
    chunks.extend(chunks_b);

    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 11, "path": "a.cook", "deleted": false, "chunk_ids": ids_a.join(",") },
            { "id": 12, "path": "b.cook", "deleted": false, "chunk_ids": ids_b.join(",") }
        ])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chunks/download"))
        .respond_with(ChunkStore { chunks, fail_on: None })
        .mount(&server)
        .await;

    let base = common::client_base();
    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
    check_download_once(&base.pool, Arc::clone(&chunker_arc), &remote, base.dir.path(), NS)
        .await
        .expect("check_download_once");

    let requests = download_requests(&server).await;
    assert_eq!(requests.len(), 2, "one request per file-sized batch");
    for request in &requests {
        assert!(
            requested_chunk_ids(request).len() <= DOWNLOAD_BATCH_MAX_CHUNKS,
            "no request asks for more than the batch limit"
        );
    }

    assert_eq!(tokio::fs::read_to_string(base.dir.path().join("a.cook")).await.unwrap(), content_a);
    assert_eq!(tokio::fs::read_to_string(base.dir.path().join("b.cook")).await.unwrap(), content_b);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_download_once_keeps_finished_batches_when_a_later_one_fails() {
    let server = MockServer::start().await;

    let lines = DOWNLOAD_BATCH_MAX_CHUNKS * 3 / 4;
    let content_a = unique_lines("a", lines);
    let content_b = unique_lines("b", lines);
    let (ids_a, mut chunks) = text_chunks(&content_a).await;
    let (ids_b, chunks_b) = text_chunks(&content_b).await;
    chunks.extend(chunks_b);

    // The server lists the newer record first: the client has to put them in
    // order itself, or a failure would leave a gap below its resume point.
    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 12, "path": "b.cook", "deleted": false, "chunk_ids": ids_b.join(",") },
            { "id": 11, "path": "a.cook", "deleted": false, "chunk_ids": ids_a.join(",") }
        ])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chunks/download"))
        .respond_with(ChunkStore { chunks, fail_on: Some(ids_b[0].clone()) })
        .mount(&server)
        .await;

    let base = common::client_base();
    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
    let result = check_download_once(&base.pool, Arc::clone(&chunker_arc), &remote, base.dir.path(), NS).await;
    assert!(result.is_err(), "the failed batch is reported");

    // The first batch is on disk and in the registry...
    assert_eq!(tokio::fs::read_to_string(base.dir.path().join("a.cook")).await.unwrap(), content_a);
    assert!(!base.dir.path().join("b.cook").exists(), "the failed batch wrote nothing");

    // ...so the next attempt asks the server only for what is still missing.
    let conn = &mut get_connection(&base.pool).expect("checkout");
    assert_eq!(registry::latest_jid(conn, NS).expect("latest_jid"), 11);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_download_once_asks_for_a_repeated_chunk_once() {
    let server = MockServer::start().await;

    // Three identical lines are one chunk, used three times.
    let content = "salt\nsalt\nsalt\n";
    let (ids, chunks) = text_chunks(content).await;
    assert_eq!(ids.len(), 3);

    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 11, "path": "a.cook", "deleted": false, "chunk_ids": ids.join(",") }
        ])))
        .mount(&server)
        .await;
    Mock::given(method("POST"))
        .and(path("/chunks/download"))
        .respond_with(ChunkStore { chunks, fail_on: None })
        .mount(&server)
        .await;

    let base = common::client_base();
    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(base.chunker));
    check_download_once(&base.pool, Arc::clone(&chunker_arc), &remote, base.dir.path(), NS)
        .await
        .expect("check_download_once");

    let requests = download_requests(&server).await;
    assert_eq!(requests.len(), 1);
    assert_eq!(requested_chunk_ids(&requests[0]).len(), 1, "the chunk is requested once");
    assert_eq!(tokio::fs::read_to_string(base.dir.path().join("a.cook")).await.unwrap(), content);
}

// ---------------------------------------------------------------------------
// Stopping a download in flight
// ---------------------------------------------------------------------------

use cooklang_sync_client::models::SyncStatus;
use cooklang_sync_client::syncer;
use cooklang_sync_client::SyncStatusListener;
use std::time::{Duration, Instant};
use tokio_util::sync::CancellationToken;

#[derive(Default)]
struct RecordingListener {
    statuses: std::sync::Mutex<Vec<SyncStatus>>,
}

impl SyncStatusListener for RecordingListener {
    fn on_status_changed(&self, status: SyncStatus) {
        self.statuses.lock().unwrap().push(status);
    }

    fn on_complete(&self, _success: bool, _message: Option<String>) {}
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_stops_a_download_in_flight_without_reporting_idle() {
    let server = MockServer::start().await;

    let (ids, _) = text_chunks("Eggs\n").await;
    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 11, "path": "a.cook", "deleted": false, "chunk_ids": ids.join(",") }
        ])))
        .mount(&server)
        .await;
    // A download that hangs: nothing arrives for far longer than the test runs.
    Mock::given(method("POST"))
        .and(path("/chunks/download"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(120)))
        .mount(&server)
        .await;

    let base = common::client_base();
    let remote = Remote::new(&server.uri(), TOKEN);
    let token = CancellationToken::new();
    let listener = Arc::new(RecordingListener::default());
    let (_tx, rx) = futures::channel::mpsc::channel(1);

    let stopper = token.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        stopper.cancel();
    });

    let started = Instant::now();
    let result = tokio::time::timeout(
        Duration::from_secs(10),
        syncer::run(
            token,
            Some(listener.clone() as Arc<dyn SyncStatusListener>),
            &base.pool,
            base.dir.path(),
            NS,
            base.chunker,
            &remote,
            rx,
            true,
        ),
    )
    .await
    .expect("a stopped run returns promptly, not when the download times out");

    assert!(result.is_ok(), "stopping is not an error: {result:?}");
    assert!(started.elapsed() < Duration::from_secs(5));

    let statuses = listener.statuses.lock().unwrap();
    assert!(
        !statuses.iter().any(|s| matches!(s, SyncStatus::Idle)),
        "an unfinished download must not be reported as done: {statuses:?}"
    );
}

// ---------------------------------------------------------------------------
// Downloads go first
// ---------------------------------------------------------------------------

/// Answers like `inner` and notes when it was asked.
struct Timed<R: Respond> {
    inner: R,
    asked_at: Arc<std::sync::Mutex<Option<Instant>>>,
}

impl<R: Respond> Respond for Timed<R> {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.asked_at.lock().unwrap().get_or_insert_with(Instant::now);
        self.inner.respond(request)
    }
}

/// Answers like `inner`, after a pause.
struct Delayed<R: Respond> {
    inner: R,
    delay: Duration,
}

impl<R: Respond> Respond for Delayed<R> {
    fn respond(&self, request: &Request) -> ResponseTemplate {
        self.inner.respond(request).set_delay(self.delay)
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_commits_local_changes_only_after_the_download_has_finished() {
    // Two batches, each longer than the upload loop's start-up wait: an
    // upload that did not wait for the whole download would slip in between
    // them.
    const BATCH_TAKES: Duration = Duration::from_secs(6);

    let server = MockServer::start().await;

    let lines = DOWNLOAD_BATCH_MAX_CHUNKS * 3 / 4;
    let (ids_a, mut chunks) = text_chunks(&unique_lines("a", lines)).await;
    let (ids_b, chunks_b) = text_chunks(&unique_lines("b", lines)).await;
    chunks.extend(chunks_b);

    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(200).set_body_json(serde_json::json!([
            { "id": 11, "path": "a.cook", "deleted": false, "chunk_ids": ids_a.join(",") },
            { "id": 12, "path": "b.cook", "deleted": false, "chunk_ids": ids_b.join(",") }
        ])))
        .mount(&server)
        .await;
    Mock::given(method("GET"))
        .and(path("/metadata/poll"))
        .respond_with(ResponseTemplate::new(200).set_delay(Duration::from_secs(60)))
        .mount(&server)
        .await;

    let download_asked_at = Arc::new(std::sync::Mutex::new(None));
    Mock::given(method("POST"))
        .and(path("/chunks/download"))
        .respond_with(Timed {
            inner: Delayed { inner: ChunkStore { chunks, fail_on: None }, delay: BATCH_TAKES },
            asked_at: Arc::clone(&download_asked_at),
        })
        .mount(&server)
        .await;

    let commit_asked_at = Arc::new(std::sync::Mutex::new(None));
    Mock::given(method("POST"))
        .and(path("/metadata/commit"))
        .respond_with(Timed {
            inner: ResponseTemplate::new(200).set_body_json(serde_json::json!({ "Success": 100 })),
            asked_at: Arc::clone(&commit_asked_at),
        })
        .mount(&server)
        .await;

    // A local file waiting to be uploaded.
    let base = common::client_base();
    tokio::fs::write(base.dir.path().join("local.cook"), b"Milk\n").await.unwrap();
    {
        let conn = &mut get_connection(&base.pool).expect("checkout");
        registry::create(conn, &[sample_create("local.cook", 5)]).expect("create");
    }

    let remote = Remote::new(&server.uri(), TOKEN);
    let token = CancellationToken::new();
    let (_tx, rx) = futures::channel::mpsc::channel(1);

    let stopper = token.clone();
    let committed = Arc::clone(&commit_asked_at);
    tokio::spawn(async move {
        // Stop once the commit has been seen, or give up.
        for _ in 0..250 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if committed.lock().unwrap().is_some() {
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(300)).await;
        stopper.cancel();
    });

    syncer::run(token, None, &base.pool, base.dir.path(), NS, base.chunker, &remote, rx, false)
        .await
        .expect("run");

    let download_asked_at = download_asked_at.lock().unwrap().expect("the download was requested");
    let commit_asked_at = commit_asked_at.lock().unwrap().expect("the local file was committed");
    assert!(
        commit_asked_at >= download_asked_at + 2 * BATCH_TAKES,
        "committed {:?} after the download started; both batches take {:?}",
        commit_asked_at.duration_since(download_asked_at),
        2 * BATCH_TAKES
    );
}
