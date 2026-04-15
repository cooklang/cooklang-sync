# Library Test Coverage — Design

**Date:** 2026-04-15
**Scope:** `cooklang-sync-client` and `cooklang-sync-server` crates
**Goal:** Full coverage pass — every module has meaningful tests (unit for logic, integration for orchestration), plus a small end-to-end client↔server harness. No hard coverage percentage gate; coverage is a byproduct of module-by-module meaningfulness.

---

## Decisions (locked)

| Topic | Choice |
|---|---|
| Scope | Full coverage pass (unit + integration across every module with gaps) |
| Integration strategy | In-process Rocket client (`rocket::local::asynchronous::Client`) for server; `wiremock` for client-side `Remote` unit tests; a small number of true client↔server e2e tests against a real spawned server on an ephemeral port |
| Database in tests | Shared tempdir + migration helper — one helper builds a fresh on-disk SQLite, runs diesel migrations, returns a pool. Cleanup via `TempDir` drop. |
| Filesystem in tests | Real filesystem via `tempfile::TempDir`. Mock only `Remote` (via `wiremock`) and `SyncStatusListener` (test double). No filesystem trait refactor. |
| Coverage target | Module-by-module meaningfulness. No `cargo tarpaulin` gate in CI. |
| FFI/mobile surface | Out of scope — tested transitively via the Rust functions they wrap. |
| Mutation/fuzz/bench | Out of scope beyond existing `proptest` suite. |

---

## 1. Test organization & shared helpers

### Layout

```
client/
  src/*.rs           # inline #[cfg(test)] unit tests stay/extend here
  tests/
    common/mod.rs               # shared helpers
    chunk_property_tests.rs     # existing, kept unchanged
    registry_tests.rs
    indexer_tests.rs
    syncer_tests.rs
    remote_tests.rs
    lib_tests.rs
    e2e_tests.rs                # full client ↔ real server

server/
  src/*.rs           # inline #[cfg(test)] unit tests stay/extend here
  tests/
    common/mod.rs
    auth_tests.rs
    metadata_routes.rs
    chunks_routes.rs
    chunk_id_tests.rs
    notification_tests.rs
    integration.rs              # route-level multi-step flows
```

### Shared helpers

**`client/tests/common/mod.rs`:**

- `fresh_client_pool() -> (DbPool, TempDir)` — creates a `TempDir`, builds a SQLite path inside it, runs embedded migrations, returns a `diesel::r2d2` pool and the `TempDir` guard so the caller keeps the directory alive for the test's lifetime.
- `tempdir_with_files(files: &[(&str, &[u8])]) -> TempDir` — builds a storage dir pre-populated with files at relative paths.
- `wiremock_remote() -> (MockServer, Remote)` — starts a `wiremock::MockServer`, constructs a `Remote` pointing at its `uri()`, returns both.
- `recording_listener() -> (Arc<SyncContext>, Arc<Mutex<Vec<SyncStatus>>>)` — returns a `SyncContext` whose listener pushes every `on_status_changed` and `on_complete` call into a shared `Vec` for assertions.
- `sample_jwt(uid: i32) -> String` — hard-coded test secret (`"secret"`) signing a `{uid, exp}` claim far in the future.

**`server/tests/common/mod.rs`:**

- `fresh_server_pool() -> (DbPool, TempDir)` — same idea as client helper for server's metadata DB.
- `rocket_client() -> (rocket::local::asynchronous::Client, TempDir)` — builds a `create_server()` instance with test DB pool + tempdir-backed chunk storage path, returns the local client and the dir guard.
- `mint_jwt(uid: i32) -> String` — signs with the configured test secret.
- `authed_get(client, path, uid)` / `authed_post(client, path, uid, body)` — convenience wrappers that inject `Authorization: Bearer <token>`.

---

## 2. Client test coverage

### `chunker.rs` (extend existing)

Already well-covered. Add:

- Multi-chunk text file (>1 line boundary, verify chunk count)
- `hashify` on non-existent path returns error
- `delete` removes chunk from disk and evicts from cache
- `save` errors when one of the referenced chunks is missing

### `connection.rs`

- `get_connection_pool(path)` creates pool, runs migrations, returns a working connection
- Fails gracefully on un-writable path

### `errors.rs`

2–3 tests: `From` conversions for the wrapped variants round-trip (e.g., `io::Error → SyncError`, diesel, reqwest). Just enough to pin the variant surface.

### `context.rs`

- `SyncContext::new` stores token and listener
- `token()` / `listener()` accessors return expected values
- Cloning listener `Arc` produces same underlying callable
- `CancellationToken` triggers downstream cancellation

### `file_watcher.rs`

- `async_watcher()` returns a live debouncer + channel
- Writing a file into a watched dir eventually surfaces on the channel within a bounded timeout

### `registry.rs` (deep coverage)

Against a fresh client pool (migrations applied):

- `create` inserts a row, assigns a monotonic `jid`
- `update` changes size/mtime/hashes, preserves path
- `delete` marks `deleted=true` with a new `jid`
- `latest_jid` is monotonic across mixed ops
- `find_by_path` returns the most recent record for that path
- `records_since(jid)` returns only rows with `jid > since`, in jid order
- `next_unsynced` / `mark_synced` flow: unsynced rows surface, marking removes them
- Empty-DB edge cases: `latest_jid` on empty, `records_since(0)` on empty

### `indexer.rs`

Already has `truncate_to_seconds`. Add:

- `check_index_once` on a `TempDir`:
  - New file → `CreateForm` recorded in registry
  - Modified file → `UpdateForm` recorded
  - Deleted file → `DeleteForm` recorded
  - Unchanged file → no-op
  - Directories skipped
  - Hidden files handled per current behavior (assert whatever the code does — pin it)
- `run` loop: drive a real `TempDir`, push file events through the channel, assert rows land in the registry and the `local_registry_updated_tx` is pinged

### `remote.rs` (against `wiremock`)

- `commit` happy path: returns `ok`, new jid
- `commit` with needed-chunks response: client surfaces the missing-hash set
- `commit` 401 → `SyncError::Unauthorized` (or whatever current variant); 500 → generic error
- `list(since_jid)` returns ordered records; handles empty response
- `poll` returns when server returns 200; returns cleanly on timeout
- `upload_chunks` forms correct multipart body; success → ok; 400 → error surfaces
- `download_chunks` decodes multipart response into `Vec<(hash, bytes)>`
- `check_chunks` returns the missing hash set
- All requests carry `Authorization: Bearer <token>` header

### `syncer.rs` (highest-value surface)

Mock `Remote` (via `wiremock` or a hand-rolled trait impl — see Open Questions §6), real registry + real FS tempdir:

- **Upload path:** unsynced local record → `commit` reports needed chunks → `upload_chunks` → second `commit` → row `mark_synced`. Asserts `jid` assigned, chunks uploaded exactly once.
- **Download path:** `list` returns newer records → `check_chunks` identifies missing → `download_chunks` pulls them → `chunker.save` reconstructs file on disk → local registry updated to matching `jid`.
- **Conflict:** local pending + incoming remote record for same path. Pin current behavior (document in test what wins, whether conflict resolution happens, etc.).
- **`download_only=true`** skips the upload half entirely — assert no `commit`/`upload_chunks` calls observed by wiremock.
- **Status listener** receives `Syncing` → `Idle` transitions at the right moments; `Error` on failure.
- **Error propagation:** wiremock returns 500 → syncer surfaces `SyncError`, listener gets `on_complete(false, Some(msg))`.

### `lib.rs`

- `extract_uid_from_jwt`:
  - Happy path: returns `uid` from a well-formed token
  - Malformed token (wrong number of parts) currently `panic!`s — pin this, or if the design calls for returning a `Result` note it as a follow-up
  - Missing `uid` claim: same — pin current behavior
- `run_async` smoke test: wiremock returns empty `list`, no local files; spawn run, cancel via `CancellationToken` after a brief moment, assert it exits cleanly and listener sees `Syncing`→`Idle` (or completion callback)
- `run_upload_once` / `run_download_once`: short integration against wiremock

---

## 3. Server test coverage

### `auth/token.rs`

- `sign` + `verify` round-trip returns the original `uid`
- Verification rejects wrong-secret, expired token, malformed token
- `uid` extraction correct

### `auth/request.rs`

Via `rocket_client()`, mounted on a simple test route that echoes `user.uid`:

- Valid token → route receives correct `User { uid }`
- Missing `Authorization` header → 401
- Malformed bearer (no "Bearer " prefix, empty token) → 401
- Expired / bad-signature token → 401

### `auth/user.rs`, `auth/mod.rs`

Covered transitively; no dedicated tests (they are trivial type re-exports).

### `chunk_id.rs`

Pure functions:

- `chunk_path(uid, hash)` produces the expected on-disk layout (e.g., `uid/aa/bb/…`). Exact layout derived from reading the current implementation.
- Different `(uid, hash)` pairs never collide to the same path
- Round-trip parse (if a reverse helper exists)
- Edge hashes: shortest legal length, all-zero, mixed case

### `metadata/db.rs`

Pool / migration helpers run cleanly against a fresh tempdir SQLite.

### `metadata/models.rs` (extend)

Already has construction tests. Add a diesel insert + select round-trip using `fresh_server_pool()` to verify the `Insertable`/`Queryable` derives match the schema.

### `metadata/middleware.rs`

Via `rocket_client()`, verify a representative route returns the shape middleware produces (e.g., uniform error body on a 4xx, CORS headers present if applicable).

### `metadata/notification.rs`

- `notify(uid)` wakes a pending `poll(uid)`
- `poll` for a different `uid` does not wake
- `poll` with a short timeout returns cleanly when nothing fires
- Concurrent pollers on the same `uid` all wake on a single `notify`

### `metadata/request.rs` / `response.rs`

2–3 direct serde round-trip tests with edge values: empty vectors, unicode paths, very long paths. Otherwise covered transitively by route tests.

### `metadata/mod.rs` (routes)

Using `rocket::local::asynchronous::Client`:

- `POST /metadata/commit` happy path (all chunks present) → 200 with `ok` + new `jid`
- `POST /metadata/commit` with missing chunks → 200 with `needed_chunks` set, no `jid` advance (verify DB state)
- `POST /metadata/commit` with invalid/missing auth → 401
- `GET /metadata/list?since=…` returns only rows with `jid > since`, ordered, scoped to the authenticated `uid` — verify another `uid`'s rows are **not** visible
- `GET /metadata/poll` wakes on concurrent `commit`: spawn a poller, then trigger a commit in another task; assert the poller returns a non-empty response within the timeout
- Path validation: paths containing `..`, empty paths, and overly long paths behave per current code — pin whatever it does today

### `chunks/mod.rs` (routes)

- `POST /chunks/upload` multipart: valid chunks stored on disk under the uid namespace, returns ok; file contents on disk match posted bytes
- Upload with hash mismatch (posted bytes hash ≠ claimed hash) → 400 and no file written
- `POST /chunks/check` with a mix of existing + missing hashes → returns the missing subset
- `GET /chunks/download?hashes=…` returns multipart with the requested chunks; 404 if any hash is missing for that uid
- All three require auth (401 without)
- **uid isolation:** uid=A uploads a chunk; uid=B cannot download or check-see that chunk

### `chunks/request.rs` / `response.rs`

Multipart parsing edge cases — empty body, malformed boundary — covered transitively by the route tests above.

### `create_server()` in `lib.rs`

Smoke test: builds, all expected routes mounted (hit each path once and assert not-404).

---

## 4. End-to-end tests (`client/tests/e2e_tests.rs`)

Each test spawns `create_server()` on `127.0.0.1:0` (ephemeral port) with tempdir DB + chunk storage, reads the bound port from the Rocket instance, and drives the real client against it using the public `run_upload_once` / `run_download_once` APIs.

1. **Upload → download on a second client.** Client A puts 3 files in its storage, runs `run_upload_once`; client B (fresh tempdir + fresh client DB, same uid) runs `run_download_once`; assert identical file contents.
2. **Modify → sync.** After (1), modify a file on A; sync A; sync B; assert B sees the new content and the old version is replaced.
3. **Delete propagates.** Delete a file on A; sync A; sync B; assert B's file is removed.
4. **Two uids don't see each other.** uid=1 syncs a file; uid=2 calls list via a fresh client — gets nothing.
5. **Binary round-trip.** A 2MB random-bytes file syncs A → B byte-identically.

Each test tears down the spawned server in a `Drop`/explicit shutdown path.

---

## 5. Dependencies, execution, CI

### Cargo changes

- `client/Cargo.toml` dev-deps: already has `wiremock`, `tempfile`, `mockall`, `proptest`, `tokio-test`. **No additions.**
- `server/Cargo.toml` dev-deps: add `tempfile`. Everything else is already present (`rocket` with `json`, `mockall`, `tokio-test`, `proptest`).
- Both crates need `diesel_migrations` (client already has it; server already has it) to embed migrations into the shared test helpers.

### Running

- `cargo test --workspace` runs everything.
- E2E tests live in `client/tests/e2e_tests.rs` — no `#[ignore]` by default, but named with an `e2e_` prefix so `cargo test --workspace -- --skip e2e_` skips them when iterating.
- No new CI jobs, no coverage gate. Existing CI runs `cargo test --workspace`.

### Out of scope (YAGNI)

- `cargo tarpaulin` or any coverage percentage gate
- Android / iOS FFI harness tests (covered transitively via the Rust functions they wrap)
- Mutation testing, additional fuzz beyond existing `proptest`
- Benchmarks / perf regression tests
- Load / stress tests on the server

---

## 6. Open questions resolved during design

- **Mocking `Remote` in syncer tests.** `Remote` is a concrete struct; we use `wiremock` against a `MockServer` so the real `reqwest` path runs. This avoids a trait refactor.
- **Pinning vs. fixing existing behavior.** Where current behavior is ambiguous (hidden files, path validation, conflict resolution, `extract_uid_from_jwt` on malformed input), tests **pin current behavior** with a comment noting the behavior is being documented, not validated as correct. Fixes are out of scope for this pass.

---

## 7. Deliverables

- ~11 new test files across both crates (see layout in §1)
- 2 shared-helper modules (`client/tests/common/mod.rs`, `server/tests/common/mod.rs`)
- Extensions to existing inline `#[cfg(test)]` blocks in `chunker.rs`, `indexer.rs`, `models.rs`
- One dev-dep addition (`tempfile` in `server/Cargo.toml`)
- No production code changes. If any are required to make a seam testable, they are called out in the implementation plan as small, local changes.
