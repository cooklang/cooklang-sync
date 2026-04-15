# Library Test Coverage — Plan 1: Foundation + Client Unit Tests

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Land the shared test-helper infrastructure and cover the small, leaf-level client modules with unit tests (`connection.rs`, `errors.rs`, `context.rs`, `file_watcher.rs`, JWT extraction in `lib.rs`, extensions to `chunker.rs` and `models.rs`).

**Architecture:** One shared `client/tests/common/mod.rs` helper module, reused across every integration test file. Each client module gets a focused test file under `client/tests/` (for integration-style tests that need DB/FS) or additional `#[cfg(test)]` blocks inline (for pure-logic tests). Real filesystem via `tempfile::TempDir`, real SQLite via the shared pool helper.

**Tech Stack:** Rust, `diesel` (SQLite + r2d2), `diesel_migrations`, `tokio`, `tokio-test`, `tempfile`, `mockall` (available but mostly unused in this plan), `wiremock` (introduced here for one file_watcher HTTP-adjacent smoke test — actually deferred to Plan 3).

**Companion spec:** `docs/superpowers/specs/2026-04-15-library-test-coverage-design.md`

**Sibling plans (not in scope here):**
- Plan 2: client registry + indexer integration
- Plan 3: client remote (wiremock) + syncer
- Plan 4: server unit tests (auth, chunk_id, metadata models/db/notification, request/response serde)
- Plan 5: server route tests (metadata, chunks, middleware, create_server smoke)
- Plan 6: end-to-end tests

---

## File map

**Created:**
- `client/tests/common/mod.rs` — shared test helpers
- `client/tests/connection_tests.rs`
- `client/tests/context_tests.rs`
- `client/tests/file_watcher_tests.rs`
- `client/tests/lib_jwt_tests.rs`

**Modified:**
- `client/src/errors.rs` — append `#[cfg(test)] mod tests` block
- `client/src/chunker.rs` — append 4 new tests to existing `#[cfg(test)] mod tests` block
- `client/src/lib.rs` — (no change; `extract_uid_from_jwt` tested from the external integration file)

No production code changes. Every test uses the existing public API.

---

## Task 1: Shared test helpers

**Files:**
- Create: `client/tests/common/mod.rs`

- [ ] **Step 1: Create the helpers file**

Create `client/tests/common/mod.rs` with the following content:

```rust
//! Shared helpers for integration tests.
//!
//! This module is included by each integration test via `mod common;`.
//! Every helper returns values paired with the `TempDir` that backs them
//! so callers can keep the directory alive for the test's lifetime.

#![allow(dead_code)] // Not every test file uses every helper.

use cooklang_sync_client::connection::{get_connection_pool, ConnectionPool};
use std::path::PathBuf;
use tempfile::TempDir;
use tokio::fs;

/// Build a fresh on-disk SQLite pool with all migrations applied.
/// Returns the pool and the backing `TempDir` (keep it alive).
pub fn fresh_client_pool() -> (ConnectionPool, TempDir) {
    let dir = TempDir::new().expect("create tempdir");
    let db_path = dir.path().join("client.sqlite3");
    let db_path_str = db_path.to_str().expect("tempdir path is utf-8").to_string();
    let pool = get_connection_pool(&db_path_str).expect("build connection pool");
    (pool, dir)
}

/// Build a tempdir pre-populated with files at relative paths.
/// Example: `tempdir_with_files(&[("recipes/a.cook", b"Eggs\n")]).await`
pub async fn tempdir_with_files(files: &[(&str, &[u8])]) -> TempDir {
    let dir = TempDir::new().expect("create tempdir");
    for (rel, bytes) in files {
        let path: PathBuf = dir.path().join(rel);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).await.expect("mkdir -p");
        }
        fs::write(&path, bytes).await.expect("write file");
    }
    dir
}

/// Mint a JWT signed with the test secret `"secret"` containing the given uid.
/// Expiration is set far in the future (year 2099).
/// Matches the client's `extract_uid_from_jwt` expectations (HS256, `uid` claim).
pub fn sample_jwt(uid: i32) -> String {
    // Header: {"alg":"HS256","typ":"JWT"} -> URL-safe no-pad base64
    let header = base64_url_nopad(br#"{"alg":"HS256","typ":"JWT"}"#);
    // Payload: {"uid":<uid>,"exp":4102444800}  (2100-01-01)
    let payload_json = format!(r#"{{"uid":{},"exp":4102444800}}"#, uid);
    let payload = base64_url_nopad(payload_json.as_bytes());
    // `extract_uid_from_jwt` does NOT verify the signature, so any placeholder works.
    let signature = base64_url_nopad(b"test-signature-unverified");
    format!("{}.{}.{}", header, payload, signature)
}

fn base64_url_nopad(bytes: &[u8]) -> String {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    URL_SAFE_NO_PAD.encode(bytes)
}
```

- [ ] **Step 2: Confirm `base64` is available as a dev-dep pathway**

Run: `cargo tree -p cooklang-sync-client -e normal | grep -E '^[a-z]' | grep base64 | head -5`
Expected: shows `base64 v0.22.x` as a normal (non-dev) dep of `cooklang-sync-client`. Since it's a regular dep, the test module can use it directly via `base64::...` without any additional `[dev-dependencies]` entry.

- [ ] **Step 3: Verify helpers compile under test harness**

Create a throw-away `client/tests/_common_smoke.rs` containing:

```rust
mod common;

#[test]
fn helpers_compile_and_pool_works() {
    let (pool, _dir) = common::fresh_client_pool();
    let _ = pool.get().expect("check out a connection");
}

#[test]
fn sample_jwt_has_three_parts() {
    let token = common::sample_jwt(42);
    assert_eq!(token.split('.').count(), 3);
}

#[tokio::test]
async fn tempdir_with_files_writes_files() {
    let dir = common::tempdir_with_files(&[("a/b.txt", b"hi")]).await;
    let content = tokio::fs::read(dir.path().join("a/b.txt"))
        .await
        .expect("read written file");
    assert_eq!(content, b"hi");
}
```

Run: `cargo test -p cooklang-sync-client --test _common_smoke -- --nocapture`
Expected: all 3 pass.

- [ ] **Step 4: Remove the smoke test file**

The smoke test was scaffolding. Delete `client/tests/_common_smoke.rs` — the real tests in later tasks will exercise the helpers.

Run: `rm client/tests/_common_smoke.rs`

- [ ] **Step 5: Commit**

```bash
git add client/tests/common/mod.rs
git commit -m "test(client): add shared test helpers for pool/tempdir/jwt"
```

---

## Task 2: `connection.rs` tests

**Files:**
- Create: `client/tests/connection_tests.rs`

- [ ] **Step 1: Write the failing tests**

Create `client/tests/connection_tests.rs`:

```rust
//! Integration tests for `cooklang_sync_client::connection`.

mod common;

use cooklang_sync_client::connection::{get_connection, get_connection_pool};
use diesel::prelude::*;
use tempfile::TempDir;

#[test]
fn get_connection_pool_creates_db_and_runs_migrations() {
    let dir = TempDir::new().unwrap();
    let db_path = dir.path().join("fresh.sqlite3");
    assert!(!db_path.exists(), "precondition: DB file does not exist yet");

    let pool = get_connection_pool(db_path.to_str().unwrap())
        .expect("pool creation with migrations should succeed");

    // After pool creation the DB file exists.
    assert!(db_path.exists(), "SQLite file should be created");

    // Migrations should have created the `file_records` table.
    // We probe it with a count query that must succeed against the real schema.
    let conn = &mut get_connection(&pool).expect("checkout connection");
    let count: i64 = diesel::sql_query("SELECT COUNT(*) AS c FROM file_records")
        .load::<RowCount>(conn)
        .expect("migration must have created file_records")
        .first()
        .map(|r| r.c)
        .unwrap_or(-1);
    assert_eq!(count, 0, "fresh DB should have zero rows in file_records");
}

#[test]
fn get_connection_pool_returns_error_for_unwritable_path() {
    // On macOS, /dev/null/subpath is not a writable directory.
    let bogus = "/dev/null/does_not_exist/db.sqlite3";
    let err = get_connection_pool(bogus)
        .err()
        .expect("pool creation should fail on unwritable parent");
    let msg = format!("{err}");
    assert!(
        msg.to_lowercase().contains("connection"),
        "expected ConnectionInitError message, got: {msg}"
    );
}

#[test]
fn get_connection_checks_out_multiple_times() {
    let (pool, _dir) = common::fresh_client_pool();
    let a = get_connection(&pool).expect("first checkout");
    drop(a);
    let _b = get_connection(&pool).expect("second checkout after drop");
}

#[derive(QueryableByName)]
struct RowCount {
    #[diesel(sql_type = diesel::sql_types::BigInt)]
    c: i64,
}
```

- [ ] **Step 2: Run to verify — some or all should pass on first run**

Run: `cargo test -p cooklang-sync-client --test connection_tests -- --nocapture`
Expected: all 3 pass. If `get_connection_pool_returns_error_for_unwritable_path` behaves differently on the CI host, adjust the bogus path to `"/nonexistent_dir_12345/db.sqlite3"` and document in a comment.

- [ ] **Step 3: Commit**

```bash
git add client/tests/connection_tests.rs
git commit -m "test(client): cover connection pool creation and checkout"
```

---

## Task 3: `errors.rs` tests (inline)

**Files:**
- Modify: `client/src/errors.rs` (append test module)

- [ ] **Step 1: Append the test module**

Append to the **end** of `client/src/errors.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn io_error_from_conversion_preserves_message() {
        let io = std::io::Error::new(std::io::ErrorKind::NotFound, "boom");
        let err: SyncError = io.into();
        let msg = format!("{err}");
        assert!(msg.contains("boom"), "wrapped IO error message preserved: {msg}");
        assert!(matches!(err, SyncError::IoErrorGeneric(_)));
    }

    #[test]
    fn from_io_error_helper_attaches_path() {
        let io = std::io::Error::new(std::io::ErrorKind::PermissionDenied, "nope");
        let err = SyncError::from_io_error("/some/path", io);
        let msg = format!("{err}");
        assert!(msg.contains("/some/path"), "path should be in message: {msg}");
        assert!(msg.contains("nope"), "source cause should be in message: {msg}");
        assert!(matches!(err, SyncError::IoError { .. }));
    }

    #[test]
    fn unauthorized_has_stable_display() {
        let err = SyncError::Unauthorized;
        assert_eq!(format!("{err}"), "Unauthorized token");
    }

    #[test]
    fn unknown_variant_includes_context() {
        let err = SyncError::Unknown("xyz".into());
        assert!(format!("{err}").contains("xyz"));
    }
}
```

- [ ] **Step 2: Run the new tests**

Run: `cargo test -p cooklang-sync-client errors::tests -- --nocapture`
Expected: 4 tests pass.

- [ ] **Step 3: Commit**

```bash
git add client/src/errors.rs
git commit -m "test(client): add unit tests for SyncError conversions and display"
```

---

## Task 4: `context.rs` tests

**Files:**
- Create: `client/tests/context_tests.rs`

- [ ] **Step 1: Write the tests**

Create `client/tests/context_tests.rs`:

```rust
//! Integration tests for `SyncContext` lifecycle and status listener.

use cooklang_sync_client::{SyncContext, SyncStatus, SyncStatusListener};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingListener {
    statuses: Mutex<Vec<String>>, // Stored as strings because SyncStatus is not PartialEq.
    completions: Mutex<Vec<(bool, Option<String>)>>,
}

impl RecordingListener {
    fn statuses(&self) -> Vec<String> {
        self.statuses.lock().unwrap().clone()
    }
    fn completions(&self) -> Vec<(bool, Option<String>)> {
        self.completions.lock().unwrap().clone()
    }
}

impl SyncStatusListener for RecordingListener {
    fn on_status_changed(&self, status: SyncStatus) {
        self.statuses
            .lock()
            .unwrap()
            .push(format!("{:?}", status));
    }
    fn on_complete(&self, success: bool, message: Option<String>) {
        self.completions.lock().unwrap().push((success, message));
    }
}

#[test]
fn new_context_starts_with_no_listener() {
    let ctx = SyncContext::new();
    assert!(ctx.listener().is_none());
}

#[test]
fn set_listener_then_listener_returns_same_arc() {
    let ctx = SyncContext::new();
    let listener: Arc<RecordingListener> = Arc::new(RecordingListener::default());
    ctx.set_listener(listener.clone() as Arc<dyn SyncStatusListener>);

    let got = ctx.listener().expect("listener should be set");
    // Trigger a status; the recording listener we handed in should see it.
    got.on_status_changed(SyncStatus::Indexing);
    assert_eq!(listener.statuses(), vec!["Indexing".to_string()]);
}

#[test]
fn notify_status_forwards_to_listener() {
    let ctx = SyncContext::new();
    let listener: Arc<RecordingListener> = Arc::new(RecordingListener::default());
    ctx.set_listener(listener.clone() as Arc<dyn SyncStatusListener>);

    ctx.notify_status(SyncStatus::Syncing);
    ctx.notify_status(SyncStatus::Uploading);
    ctx.notify_status(SyncStatus::Idle);

    assert_eq!(
        listener.statuses(),
        vec!["Syncing".to_string(), "Uploading".to_string(), "Idle".to_string()]
    );
}

#[test]
fn notify_status_without_listener_is_silent() {
    let ctx = SyncContext::new();
    // Should not panic.
    ctx.notify_status(SyncStatus::Idle);
}

#[test]
fn cancel_propagates_to_child_token() {
    let ctx = SyncContext::new();
    let child = ctx.token();
    assert!(!child.is_cancelled(), "precondition: child not cancelled");
    ctx.cancel();
    assert!(child.is_cancelled(), "child should be cancelled after parent.cancel()");
}

#[test]
fn child_tokens_are_independent_of_each_other() {
    let ctx = SyncContext::new();
    let a = ctx.token();
    let b = ctx.token();
    a.cancel();
    assert!(a.is_cancelled());
    assert!(!b.is_cancelled(), "sibling token should not be affected");
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p cooklang-sync-client --test context_tests -- --nocapture`
Expected: 6 tests pass.

- [ ] **Step 3: Commit**

```bash
git add client/tests/context_tests.rs
git commit -m "test(client): cover SyncContext listener and cancellation"
```

---

## Task 5: `file_watcher.rs` tests

**Files:**
- Create: `client/tests/file_watcher_tests.rs`

Note: the debouncer flushes every 2 seconds. Tests use a generous timeout.

- [ ] **Step 1: Write the tests**

Create `client/tests/file_watcher_tests.rs`:

```rust
//! Integration tests for `async_watcher`.
//!
//! The debouncer is configured with a 2-second flush window in production,
//! so these tests wait up to 6 seconds for events. Slow, but correct.

mod common;

use cooklang_sync_client::file_watcher::async_watcher;
use futures::StreamExt;
use notify::RecursiveMode;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_watcher_reports_file_creation() {
    let dir = TempDir::new().unwrap();
    let (mut debouncer, mut rx) = async_watcher().expect("build debouncer");

    debouncer
        .watcher()
        .watch(dir.path(), RecursiveMode::Recursive)
        .expect("watch tempdir");

    // Create a file after a short delay so the watcher is definitely armed.
    let path = dir.path().join("hello.cook");
    tokio::spawn({
        let path = path.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            tokio::fs::write(&path, b"content").await.expect("write");
        }
    });

    // Debouncer fires after DEBOUNCE_SEC (2s); allow up to 6s.
    let result = timeout(Duration::from_secs(6), rx.next())
        .await
        .expect("watcher event should arrive before timeout");

    let events = result
        .expect("channel should deliver at least one event")
        .expect("watcher result should not be Err");
    assert!(
        events.iter().any(|e| e.path == path),
        "event list should include the created file; got: {:?}",
        events
    );
}
```

- [ ] **Step 2: Run the test**

Run: `cargo test -p cooklang-sync-client --test file_watcher_tests -- --nocapture`
Expected: 1 test passes. Runtime ~2-3 seconds.

- [ ] **Step 3: Commit**

```bash
git add client/tests/file_watcher_tests.rs
git commit -m "test(client): add async_watcher file-event smoke test"
```

---

## Task 6: `lib.rs` JWT extraction tests

**Files:**
- Create: `client/tests/lib_jwt_tests.rs`

Note on behavior: the current `extract_uid_from_jwt` uses `.expect(...)` on malformed input, so malformed tokens **panic**. The tests **pin** this behavior. A follow-up could change the signature to `Result`, but that's out of scope.

- [ ] **Step 1: Write the tests**

Create `client/tests/lib_jwt_tests.rs`:

```rust
//! Tests for `extract_uid_from_jwt`.
//!
//! `extract_uid_from_jwt` does **not** verify signatures — it only decodes the
//! middle (payload) segment and extracts the `uid` claim. These tests document
//! current behavior, including panic paths, so future refactors notice breakage.

mod common;

use cooklang_sync_client::extract_uid_from_jwt;

#[test]
fn extract_uid_returns_correct_uid_for_valid_token() {
    let token = common::sample_jwt(42);
    assert_eq!(extract_uid_from_jwt(&token), 42);
}

#[test]
fn extract_uid_handles_zero_uid() {
    let token = common::sample_jwt(0);
    assert_eq!(extract_uid_from_jwt(&token), 0);
}

#[test]
fn extract_uid_handles_negative_uid() {
    let token = common::sample_jwt(-1);
    assert_eq!(extract_uid_from_jwt(&token), -1);
}

#[test]
#[should_panic]
fn extract_uid_panics_on_token_with_missing_segments() {
    // Only one segment — no '.' separators.
    let _ = extract_uid_from_jwt("notatoken");
}

#[test]
#[should_panic]
fn extract_uid_panics_on_malformed_base64_payload() {
    // Three segments but payload is not valid base64.
    let _ = extract_uid_from_jwt("aaa.!!!not-b64!!!.bbb");
}

#[test]
#[should_panic]
fn extract_uid_panics_on_payload_without_uid_field() {
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;
    use base64::Engine;
    let header = URL_SAFE_NO_PAD.encode(br#"{"alg":"HS256","typ":"JWT"}"#);
    let payload = URL_SAFE_NO_PAD.encode(br#"{"not_uid":1}"#);
    let sig = URL_SAFE_NO_PAD.encode(b"x");
    let token = format!("{header}.{payload}.{sig}");
    let _ = extract_uid_from_jwt(&token);
}
```

- [ ] **Step 2: Run the tests**

Run: `cargo test -p cooklang-sync-client --test lib_jwt_tests -- --nocapture`
Expected: 6 tests pass.

- [ ] **Step 3: Commit**

```bash
git add client/tests/lib_jwt_tests.rs
git commit -m "test(client): pin extract_uid_from_jwt success and panic paths"
```

---

## Task 7: Extend `chunker.rs` tests

**Files:**
- Modify: `client/src/chunker.rs` (append 4 tests inside the existing `#[cfg(test)] mod tests` block)

Before writing, read the end of the existing `mod tests` block so your inserts go **before** the closing `}`.

- [ ] **Step 1: Read the target insertion point**

Run: `cargo test -p cooklang-sync-client --lib chunker::tests -- --list 2>/dev/null | tail -40`
Verify you see the existing test names. Note the exact line number of the closing `}` of `mod tests` by opening the file and scrolling to the end.

- [ ] **Step 2: Append the new tests inside `mod tests`**

Just before the final `}` of the `mod tests` block in `client/src/chunker.rs`, insert:

```rust
    #[tokio::test]
    async fn hashify_errors_on_missing_file() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = InMemoryCache::new(100, 10_000);
        let mut chunker = Chunker::new(cache, temp.path().to_path_buf());

        let err = chunker
            .hashify("does_not_exist.cook")
            .await
            .expect_err("hashify on missing file should error");
        // We only assert it is an error; the specific variant is implementation-dependent.
        let _ = format!("{err:?}");
    }

    #[tokio::test]
    async fn save_errors_when_referenced_chunk_is_missing() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = InMemoryCache::new(100, 10_000);
        let mut chunker = Chunker::new(cache, temp.path().to_path_buf());

        // Reference a hash that was never stored.
        let phantom = "deadbeefdeadbeefdeadbeefdeadbeef";
        let err = chunker
            .save("out.cook", vec![phantom])
            .await
            .expect_err("save should fail when chunk is not available");
        let _ = format!("{err:?}");
    }

    #[tokio::test]
    async fn delete_removes_chunk_from_disk_and_cache() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = InMemoryCache::new(100, 10_000);
        let mut chunker = Chunker::new(cache, temp.path().to_path_buf());

        let data = b"alpha".to_vec();
        let hash = chunker.hash(&data, 16);
        chunker.save_chunk(&hash, data.clone()).unwrap();

        // Precondition: chunk is retrievable.
        let got = chunker.read_chunk(&hash).expect("chunk present before delete");
        assert_eq!(got, data);

        chunker.delete_chunk(&hash).expect("delete_chunk succeeds");

        // Postcondition: read must now fail (or return empty — pin whichever).
        let post = chunker.read_chunk(&hash);
        assert!(
            post.is_err() || post.as_ref().map(|v| v.is_empty()).unwrap_or(false),
            "after delete, read_chunk should error or return empty; got: {post:?}"
        );
    }

    #[tokio::test]
    async fn hashify_text_file_with_multiple_lines_produces_multiple_chunks() {
        let temp = tempfile::TempDir::new().unwrap();
        let cache = InMemoryCache::new(1000, 100_000);
        let mut chunker = Chunker::new(cache, temp.path().to_path_buf());

        let content = "line-a\nline-b\nline-c\n";
        tokio::fs::write(temp.path().join("multi.cook"), content.as_bytes())
            .await
            .unwrap();

        let hashes = chunker.hashify("multi.cook").await.unwrap();
        assert_eq!(
            hashes.len(),
            3,
            "three newline-terminated lines should yield three text chunks; got {hashes:?}"
        );
    }
```

**Important:** if `delete_chunk` is not the current method name, check the `Chunker` impl and adjust. Also confirm `save_chunk`/`read_chunk` method signatures match those already used by the existing property tests (they do — see `client/tests/chunk_property_tests.rs`).

- [ ] **Step 3: Run the extended suite**

Run: `cargo test -p cooklang-sync-client --lib chunker::tests -- --nocapture`
Expected: all tests (existing + 4 new) pass.

- [ ] **Step 4: Commit**

```bash
git add client/src/chunker.rs
git commit -m "test(client): add chunker error paths and multi-chunk case"
```

---

## Task 8: Verify the full plan-1 suite is green

- [ ] **Step 1: Run the whole client test suite**

Run: `cargo test -p cooklang-sync-client -- --nocapture`
Expected: every test from this plan, plus all pre-existing tests, pass. Note the time taken (should be under ~15 seconds on a modern laptop, dominated by the 2s file-watcher debounce).

- [ ] **Step 2: Confirm no warnings from the new test files**

Run: `cargo test -p cooklang-sync-client --tests 2>&1 | grep -E 'warning:' | head -20`
Expected: no new warnings (or only warnings from pre-existing non-test code).

- [ ] **Step 3: Final commit if any touch-ups were needed**

If steps 1–2 produced fixes, commit them:

```bash
git add -A
git commit -m "test(client): fix-ups from full plan-1 suite run"
```

Otherwise skip.

---

## Self-review notes

- **Helpers keep `TempDir` alive via return tuple** — so callers cannot drop the directory prematurely.
- **`extract_uid_from_jwt` panic tests use `#[should_panic]`** — pinning documented behavior, not endorsing it.
- **`file_watcher` test uses `flavor = "multi_thread"`** — the debouncer's internal executor needs a worker thread distinct from the test's.
- **No production code changes** in this plan. If a future subagent discovers they need a new public accessor (e.g., to inspect `Chunker` internals), that goes in the subsequent plan, not here.

---

## Handoff

After this plan is complete, execution continues with:

- **Plan 2** (to be written): client registry integration tests, indexer extended tests.
- **Plan 3** (to be written): client remote + syncer with `wiremock`.
- **Plan 4** (to be written): server unit tests.
- **Plan 5** (to be written): server route tests.
- **Plan 6** (to be written): end-to-end client↔server tests.
