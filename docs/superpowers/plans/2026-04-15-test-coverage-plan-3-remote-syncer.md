# Test Coverage Plan 3 — Client `remote` + `syncer` Integration Tests

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integration coverage for `client/src/remote.rs` (driven by `wiremock`) and for the two public syncer entry points `syncer::check_upload_once` / `syncer::check_download_once` (driven by a real SQLite pool, a real `Chunker` rooted at a tempdir, and a `wiremock` server acting as the remote). Also absorbs three follow-ups deferred from the PR #16 review that belong to this layer.

**Architecture:** New integration test files under `client/tests/` — `remote_tests.rs` and `syncer_tests.rs`. Both reuse `common::fresh_client_pool()` from Plan 1. A new `common::client_base()` helper is added alongside to build a real `Chunker` + `storage_dir` tempdir for syncer tests. Tests use `wiremock::MockServer` with HTTP mocks and match the exact URL shapes implemented in `remote.rs`. No trait mocks, no mockall, no production code changes.

**Tech Stack:** `wiremock = "0.6"`, `diesel` (SQLite + r2d2), `tempfile::TempDir`, `tokio` (multi-threaded test runtime), `time::OffsetDateTime`, `futures::StreamExt`, `serde_json` (already transitive — used for `set_body_json` fixtures).

**Companion spec:** `docs/superpowers/specs/2026-04-15-library-test-coverage-design.md`

**Sibling plans:**
- Plan 1: foundation + client unit tests (landed in PR #15)
- Plan 2: registry + indexer integration (landed in PR #16)
- Plan 4: server unit tests
- Plan 5: server route tests
- Plan 6: end-to-end tests

---

## Preamble: surface being pinned

### `remote.rs` endpoints the tests cover

The client issues the exact requests below; the test suite pins them with `wiremock`:

| Method | URL template | Content-Type | Response shape | Auth header required |
|---|---|---|---|---|
| `POST` | `/chunks/{chunk}` | raw bytes | `200 OK` / `401` / other | yes |
| `POST` | `/chunks/upload` | `multipart/form-data; boundary=...` | `200` / `401` / other | yes |
| `GET`  | `/chunks/{chunk}` | — | `200` (body=bytes) / `401` / other | yes |
| `POST` | `/chunks/download` | form-encoded `chunk_ids[]=a&chunk_ids[]=b` | `200` multipart with `X-Chunk-ID` per part / `401` / other | yes |
| `GET`  | `/metadata/list?jid={n}` | — | `200 [ResponseFileRecord...]` / `401` / other | yes |
| `GET`  | `/metadata/poll?seconds={s}&uuid={u}` | — | `200` / `401` / timeout-as-Ok / other | yes |
| `POST` | `/metadata/commit?uuid={u}` | form-encoded `deleted=...&chunk_ids=...&path=...` | `200 CommitResultStatus` / `401` / other | yes |

Every outgoing request carries:
- `Authorization: Bearer {token}`
- `User-Agent: cooklang-sync-client/{CARGO_PKG_VERSION}` (set as default header on the `reqwest::Client`)
- `x-client-version: {CARGO_PKG_VERSION}` (same)

The `Remote` struct generates a per-instance `uuid` (v4) that is appended to `commit` and `poll` URLs. Tests should **not** assert on its exact value — only on shape — because the `uuid` is non-deterministic per `Remote::new`.

### `CommitResultStatus` wire shape

The enum is `#[derive(Deserialize, Serialize)]` without `#[serde(...)]` adjustments, so it uses serde's default enum encoding: externally tagged.

```json
{ "Success": 17 }
{ "NeedChunks": "abc,def" }
```

Tests serialize/deserialize via this shape.

### `syncer` entry points covered

- `check_upload_once(pool, chunker, remote, namespace_id) -> Result<bool>` — scans `registry::updated_locally`, calls `remote.commit`, on `NeedChunks` enqueues chunk bodies into batches then calls `remote.upload_batch`. Returns `true` only when every row committed successfully on the first pass (no `NeedChunks` branch taken).
- `check_download_once(pool, chunker, remote, storage_path, namespace_id) -> Result<bool>` — uses `registry::latest_jid(...).unwrap_or(0)` as the watermark, calls `remote.list`, collects missing chunks (after warming local via `chunker.hashify` when the file already exists), calls `remote.download_batch` streaming, then per row either appends a tombstone + deletes the local file (for deletes) or writes the file via `chunker.save` and appends a registry row. Returns `true` when the remote list was non-empty.

The full `syncer::run` / `upload_loop` / `download_loop` are **not** targeted directly here because `upload_loop` sleeps 5 s before first iteration and `download_loop` spins on `remote.poll()` forever; that belongs to Plan 6 end-to-end. The two `check_*_once` functions are the meaningful unit-of-work boundary and are also what external callers invoke through `lib::run_download_once` / `lib::run_upload_once`.

### Deferred items absorbed from PR #16 review

Three small follow-ups are folded into this plan because they exercise code paths naturally reached from syncer/registry tests:

1. **`check_index_once` on an empty storage dir** — pin it returns `Ok(false)` and writes nothing.
2. **`registry::updated_locally` surfaces an unsynced deletion** — a path whose latest row is a tombstone with `jid = None` must appear in `updated_locally`. This keeps the upload path honest: tombstones that never made it to the server are retried.
3. **`registry::latest_jid` with `Some(0)`** — a row with `jid = Some(0)` must return `Ok(0)`, not `NotFound`. The current code path unwraps `jid` to `0` both for `Some(0)` and — conceptually — any `NULL` that slipped past the `is_not_null` filter; the test pins the intended `Some(0)` case.

---

## File map

**Created:**
- `client/tests/remote_tests.rs` — `wiremock`-driven coverage for every `Remote` method
- `client/tests/syncer_tests.rs` — upload + download path coverage through `wiremock`
- `client/tests/common/` — (extended, see Task 1)

**Modified:**
- `client/tests/common/mod.rs` — add `client_base()` helper that pairs a `Chunker` with a `storage_dir` tempdir and a pool
- `client/tests/registry_tests.rs` — append the two deferred assertions (items 2 and 3 above)
- `client/tests/indexer_tests.rs` — append the one deferred assertion (item 1 above)

No production code changes.

---

## Task 1: Extend `common` helpers for syncer tests

**Files:**
- Modify: `client/tests/common/mod.rs`

- [ ] **Step 1: Add the `client_base` helper**

Append the following to `client/tests/common/mod.rs`, below the existing helpers. Keep `#![allow(dead_code)]` at the top of the file (already present from Plan 1).

```rust
use cooklang_sync_client::chunker::{Chunker, InMemoryCache};

/// Bundle of the three things every syncer integration test needs:
/// a fresh SQLite pool, a tempdir root, and a `Chunker` rooted at that dir.
///
/// The tempdir is returned separately (not moved into the `Chunker`) so the
/// test can write fixture files into it before invoking the syncer.
pub struct ClientBase {
    pub pool: cooklang_sync_client::connection::ConnectionPool,
    pub dir: TempDir,
    pub chunker: Chunker,
}

pub fn client_base() -> ClientBase {
    let (pool, dir) = fresh_client_pool();
    let cache = InMemoryCache::new(100, 10_000_000);
    let chunker = Chunker::new(cache, dir.path().to_path_buf());
    ClientBase { pool, dir, chunker }
}
```

- [ ] **Step 2: Verify it still compiles in isolation**

Run: `cargo build -p cooklang-sync-client --tests`
Expected: build succeeds (no tests need to use `client_base` yet — the compile is all we're pinning here).

- [ ] **Step 3: Commit**

```bash
git add client/tests/common/mod.rs
git commit -m "test(common): add client_base helper for syncer tests"
```

---

## Task 2: `remote_tests.rs` scaffolding + `commit` happy path + `NeedChunks`

**Files:**
- Create: `client/tests/remote_tests.rs`

- [ ] **Step 1: Create the file with imports and a shared `new_remote` helper**

```rust
//! Integration tests for `cooklang_sync_client::remote::Remote`.
//!
//! Every test spins up a fresh `wiremock::MockServer` and points a `Remote`
//! at its URL. Tests assert on URL shape, headers, and body — *not* on the
//! per-instance `uuid` that `Remote` mints at construction.

use cooklang_sync_client::remote::{CommitResultStatus, Remote, ResponseFileRecord};
use wiremock::matchers::{body_string_contains, header, header_exists, method, path, query_param, query_param_contains};
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
    let result = remote.commit("recipes/a.cook", false, "abc,def").await.expect("commit");
    assert!(matches!(result, CommitResultStatus::Success(42)));
}
```

- [ ] **Step 2: Run the test, expect it to pass**

Run: `cargo test -p cooklang-sync-client --test remote_tests commit_returns_success_on_2xx_with_success_payload`
Expected: PASS.

- [ ] **Step 3: Add the `NeedChunks` branch**

Append below the previous test:

```rust
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
```

Run: `cargo test -p cooklang-sync-client --test remote_tests commit_returns_need_chunks_on_2xx_with_need_chunks_payload`
Expected: PASS.

- [ ] **Step 4: Commit**

```bash
git add client/tests/remote_tests.rs
git commit -m "test(remote): pin commit happy path and NeedChunks wire shape"
```

---

## Task 3: `commit` error paths (401 + 5xx)

**Files:**
- Modify: `client/tests/remote_tests.rs`

- [ ] **Step 1: Add 401 and 5xx tests**

Append:

```rust
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
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p cooklang-sync-client --test remote_tests commit_maps_`
Expected: both PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/remote_tests.rs
git commit -m "test(remote): pin commit 401/5xx error mapping"
```

---

## Task 4: `list` — happy path + empty body + error

**Files:**
- Modify: `client/tests/remote_tests.rs`

- [ ] **Step 1: Add three `list` tests**

Append:

```rust
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
```

- [ ] **Step 2: Run them**

Run: `cargo test -p cooklang-sync-client --test remote_tests list_`
Expected: all 3 PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/remote_tests.rs
git commit -m "test(remote): pin list happy/empty/unauthorized"
```

---

## Task 5: `poll` — OK + 401 + timeout-as-OK

**Files:**
- Modify: `client/tests/remote_tests.rs`

- [ ] **Step 1: Add the three `poll` tests**

Append:

```rust
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
```

- [ ] **Step 2: Run them**

Run: `cargo test -p cooklang-sync-client --test remote_tests poll_ -- --test-threads=1`
Expected: all 3 PASS. The timeout test takes ~65 s by design; do not reduce `REQUEST_TIMEOUT_SECS`.

- [ ] **Step 3: Mark the timeout test `#[ignore]` if CI flaky**

If the timeout test is flaky in CI, mark it `#[ignore = "slow — waits REQUEST_TIMEOUT_SECS+5"]` and run via `cargo test -- --ignored` in a separate CI job. Default: **keep it unignored** — the behavior it pins (treat timeout as Ok) is load-bearing for the download loop and silent regressions would be invisible.

- [ ] **Step 4: Commit**

```bash
git add client/tests/remote_tests.rs
git commit -m "test(remote): pin poll ok/unauthorized/timeout-as-ok"
```

---

## Task 6: `upload` single + `upload_batch` multipart boundary

**Files:**
- Modify: `client/tests/remote_tests.rs`

- [ ] **Step 1: Add `upload` and `upload_batch` tests**

Append:

```rust
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
```

- [ ] **Step 2: Run them**

Run: `cargo test -p cooklang-sync-client --test remote_tests upload`
Expected: all 3 PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/remote_tests.rs
git commit -m "test(remote): pin upload + upload_batch multipart shape"
```

---

## Task 7: `download` single chunk + error

**Files:**
- Modify: `client/tests/remote_tests.rs`

- [ ] **Step 1: Add `download` tests**

Append:

```rust
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
```

- [ ] **Step 2: Run them**

Run: `cargo test -p cooklang-sync-client --test remote_tests download_`
Expected: both PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/remote_tests.rs
git commit -m "test(remote): pin download ok + unauthorized"
```

---

## Task 8: `download_batch` multipart parsing

**Files:**
- Modify: `client/tests/remote_tests.rs`

- [ ] **Step 1: Add the streaming multipart test**

Append:

```rust
use futures::StreamExt;

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
```

- [ ] **Step 2: Run them**

Run: `cargo test -p cooklang-sync-client --test remote_tests download_batch`
Expected: both PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/remote_tests.rs
git commit -m "test(remote): pin download_batch multipart streaming + missing content-type"
```

---

## Task 9: `syncer_tests.rs` scaffolding + upload happy path

**Files:**
- Create: `client/tests/syncer_tests.rs`

The upload path walks: `registry::updated_locally` → `chunker.hashify` → `remote.commit` → if `Success(jid)` call `registry::update_jid`, else enqueue chunk bytes and at end call `remote.upload_batch`. The happy test makes the server always return `Success` so **no** `upload_batch` request ever fires and the function returns `Ok(true)`.

- [ ] **Step 1: Write the file scaffold and the first test**

```rust
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
use cooklang_sync_client::syncer::{check_download_once, check_upload_once};
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
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p cooklang-sync-client --test syncer_tests check_upload_once_commits_success_and_marks_jid`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/syncer_tests.rs
git commit -m "test(syncer): pin check_upload_once happy path"
```

---

## Task 10: `check_upload_once` — NeedChunks path issues `upload_batch`

**Files:**
- Modify: `client/tests/syncer_tests.rs`

- [ ] **Step 1: Add the test**

Append:

```rust
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
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p cooklang-sync-client --test syncer_tests check_upload_once_triggers_upload_batch_when_server_asks_for_chunks`
Expected: PASS. The `expect(1)` on both mocks verifies exactly one commit and one upload_batch were sent.

- [ ] **Step 3: Commit**

```bash
git add client/tests/syncer_tests.rs
git commit -m "test(syncer): pin check_upload_once NeedChunks -> upload_batch"
```

---

## Task 11: `check_upload_once` — deleted row commits without reading chunks

**Files:**
- Modify: `client/tests/syncer_tests.rs`

The upload path short-circuits hashify for `deleted=true` rows — `chunk_ids = vec![""]` and commit is called with `""`. Tombstones don't need the file on disk anymore.

- [ ] **Step 1: Add the test**

Append:

```rust
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
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p cooklang-sync-client --test syncer_tests check_upload_once_commits_tombstone_without_hashifying`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/syncer_tests.rs
git commit -m "test(syncer): pin tombstone upload short-circuits hashify"
```

---

## Task 12: `check_download_once` — applies remote list to empty local state

**Files:**
- Modify: `client/tests/syncer_tests.rs`

- [ ] **Step 1: Add the test**

Append:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_download_once_writes_file_and_inserts_registry_row() {
    use cooklang_sync_client::chunker::Chunker;

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
    let scratch_cache = cooklang_sync_client::chunker::InMemoryCache::new(10, 1_000_000);
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

    let mut base = common::client_base();
    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(&mut base.chunker));
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
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p cooklang-sync-client --test syncer_tests check_download_once_writes_file_and_inserts_registry_row`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/syncer_tests.rs
git commit -m "test(syncer): pin check_download_once applies remote list to empty state"
```

---

## Task 13: `check_download_once` — tombstone deletes local file + appends tombstone row

**Files:**
- Modify: `client/tests/syncer_tests.rs`

- [ ] **Step 1: Add the test**

Append:

```rust
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

    let mut base = common::client_base();
    tokio::fs::write(base.dir.path().join("gone.cook"), b"bye\n").await.unwrap();
    {
        let conn = &mut get_connection(&base.pool).expect("checkout");
        registry::create(conn, &vec![sample_create("gone.cook", 4)]).expect("create");
    }

    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(&mut base.chunker));
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
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p cooklang-sync-client --test syncer_tests check_download_once_removes_local_file_and_appends_tombstone`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/syncer_tests.rs
git commit -m "test(syncer): pin check_download_once tombstone path"
```

---

## Task 14: `check_download_once` — empty remote list returns false, writes nothing

**Files:**
- Modify: `client/tests/syncer_tests.rs`

- [ ] **Step 1: Add the test**

Append:

```rust
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

    let mut base = common::client_base();
    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(&mut base.chunker));
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
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p cooklang-sync-client --test syncer_tests check_download_once_empty_remote_list_is_noop`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/syncer_tests.rs
git commit -m "test(syncer): pin check_download_once empty-list noop"
```

---

## Task 15: `check_download_once` — 401 bubbles through as Unauthorized

**Files:**
- Modify: `client/tests/syncer_tests.rs`

The download loop handles this specially: `SyncError::Unauthorized` is propagated as-is so the outer caller can stop and surface a login prompt. Every other error is wrapped as `SyncError::Unknown`. Pinning this keeps the distinction load-bearing.

- [ ] **Step 1: Add the test**

Append:

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn check_download_once_propagates_unauthorized_from_list() {
    use cooklang_sync_client::errors::SyncError;

    let server = MockServer::start().await;
    Mock::given(method("GET"))
        .and(path("/metadata/list"))
        .respond_with(ResponseTemplate::new(401))
        .mount(&server)
        .await;

    let mut base = common::client_base();
    let remote = Remote::new(&server.uri(), TOKEN);
    let chunker_arc = Arc::new(Mutex::new(&mut base.chunker));
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
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p cooklang-sync-client --test syncer_tests check_download_once_propagates_unauthorized_from_list`
Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add client/tests/syncer_tests.rs
git commit -m "test(syncer): pin Unauthorized propagation from remote.list"
```

---

## Task 16: Deferred review follow-ups (registry + indexer)

**Files:**
- Modify: `client/tests/registry_tests.rs`
- Modify: `client/tests/indexer_tests.rs`

These absorb the three items the PR #16 reviewer flagged as nice-to-have.

- [ ] **Step 1: Add to `registry_tests.rs`: unsynced tombstone surfaced by `updated_locally`**

Append to `client/tests/registry_tests.rs`:

```rust
#[test]
fn updated_locally_surfaces_unsynced_tombstone() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    // Create then delete (tombstone is appended; jid still None on both rows).
    registry::create(conn, &vec![sample_create("gone.cook", 10, 1)]).expect("create");
    let existing: Vec<FileRecord> = registry::non_deleted(conn, 1).expect("non_deleted");
    let sample = existing.first().expect("row present");
    registry::delete(conn, &vec![sample_delete(sample)]).expect("delete");

    // Latest row per path is the tombstone, which still has jid=None, so
    // updated_locally must surface it — this is what lets the upload path
    // retry tombstones that never reached the server.
    let unsynced = registry::updated_locally(conn, 1).expect("updated_locally");
    assert_eq!(unsynced.len(), 1, "tombstone with jid=None must appear in updated_locally");
    assert_eq!(unsynced[0].path, "gone.cook");
    assert!(unsynced[0].deleted, "latest row is the tombstone");
    assert!(unsynced[0].jid.is_none());
}
```

- [ ] **Step 2: Add to `registry_tests.rs`: `latest_jid` returns `0` for `jid = Some(0)`**

Append:

```rust
#[test]
fn latest_jid_returns_zero_for_explicit_jid_zero() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    // Insert a single row with jid=Some(0).
    let mut form = sample_create("a.cook", 5, 1);
    form.jid = Some(0);
    registry::create(conn, &vec![form]).expect("create");

    let latest = registry::latest_jid(conn, 1).expect("latest_jid");
    assert_eq!(latest, 0, "Some(0) must unwrap to 0, not NotFound");
}
```

- [ ] **Step 3: Run the new registry tests**

Run: `cargo test -p cooklang-sync-client --test registry_tests updated_locally_surfaces_unsynced_tombstone latest_jid_returns_zero_for_explicit_jid_zero`
Expected: both PASS.

- [ ] **Step 4: Add to `indexer_tests.rs`: empty dir is a no-op**

Append to `client/tests/indexer_tests.rs`:

```rust
#[test]
fn check_index_once_on_empty_dir_is_noop() {
    let (pool, _dir) = common::fresh_client_pool();
    let storage = tempfile::TempDir::new().expect("tempdir");

    let changed = cooklang_sync_client::indexer::check_index_once(&pool, storage.path(), 1)
        .expect("check_index_once");
    assert!(!changed, "empty dir must return Ok(false)");

    let conn = &mut cooklang_sync_client::connection::get_connection(&pool).expect("checkout");
    let rows = cooklang_sync_client::registry::non_deleted(conn, 1).expect("non_deleted");
    assert!(rows.is_empty(), "empty dir must not produce any registry rows");
}
```

- [ ] **Step 5: Run the new indexer test**

Run: `cargo test -p cooklang-sync-client --test indexer_tests check_index_once_on_empty_dir_is_noop`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add client/tests/registry_tests.rs client/tests/indexer_tests.rs
git commit -m "test(client): absorb Plan 2 review follow-ups (tombstone, jid=0, empty dir)"
```

---

## Task 17: Full suite green

**Files:** (none modified)

- [ ] **Step 1: Run the full client test suite**

Run: `cargo test -p cooklang-sync-client -- --test-threads=4`
Expected: all tests PASS. The `poll_treats_client_timeout_as_ok` test runs for ~65 s by design.

- [ ] **Step 2: Run clippy with `-D warnings`**

Run: `cargo clippy -p cooklang-sync-client --tests -- -D warnings`
Expected: 0 warnings.

- [ ] **Step 3: Verify nothing changed in production code**

Run: `git diff --stat main..HEAD -- client/src/`
Expected: empty (no changes under `client/src/`). This plan adds tests only.

If unexpected production changes appear, investigate before continuing — the plan forbids touching production code.

---

## Out of scope for Plan 3

- The full `syncer::run` loop (covered end-to-end in Plan 6).
- `upload_loop` / `download_loop` scheduling and listener transitions (covered end-to-end in Plan 6, where the whole machinery runs against a live server).
- Server-side (covered by Plans 4–5).
