# Test Coverage Plan 2 — Client `registry` + `indexer` Integration Tests

> **For agentic workers:** REQUIRED SUB-SKILL: Use `superpowers:subagent-driven-development`. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Integration coverage for `client/src/registry.rs` (all public query/mutation fns) and `client/src/indexer.rs` (`check_index_once` + `run` loop), driven by a real SQLite tempdir pool and a real tempdir filesystem.

**Architecture:** New integration test files under `client/tests/` — `registry_tests.rs` and `indexer_tests.rs`. Both reuse `common::fresh_client_pool()` from Plan 1. Tests operate on the real diesel schema with embedded migrations applied; no trait mocks are introduced.

**Tech Stack:** `diesel` (SQLite + r2d2), `tempfile::TempDir`, `time::OffsetDateTime`, `futures::channel::mpsc`, `tokio` (multi-threaded test runtime for `run` loop), `tokio_util::CancellationToken`.

---

## Preamble: API reality vs. spec §2

The design spec §2 lists some aspirational API names (`update`, `find_by_path`, `records_since`, `next_unsynced`, `mark_synced`). The actual public surface in `client/src/registry.rs` is:

| Function | Behavior |
|---|---|
| `create(conn, &Vec<CreateForm>) -> Result<usize>` | Bulk insert; `jid` typically `None`; monotonic `id` assigned by SQLite. |
| `update_jid(conn, &FileRecord, jid: i32) -> Result<usize>` | Updates `jid` on a single row by `id`. No other columns change. |
| `delete(conn, &Vec<DeleteForm>) -> Result<usize>` | **Appends** a new row with `deleted=true` (log-append, not UPDATE). |
| `non_deleted(conn, ns) -> Result<Vec<FileRecord>>` | Latest row per path (max id) filtered to `deleted=false`, scoped by namespace. |
| `updated_locally(conn, ns) -> Result<Vec<FileRecord>>` | Latest row per path with `jid IS NULL`, scoped by namespace. |
| `latest_jid(conn, ns) -> Result<i32>` | `max(jid)` across all non-null rows, scoped by namespace. Returns `0` when the row it finds has `jid = Some(0)` or a `NotFound` `diesel::result::Error` when no row has a jid. |

Tests pin this **real** surface. Each test file uses `mod common;` to get `fresh_client_pool()`.

Note also: the `indexer` does not have an `UpdateForm` path — a modified file is recorded by appending a fresh `CreateForm`, which becomes the new "latest" via the `max(id)` subquery in `non_deleted`. Tests reflect this.

---

## Task 1: `registry_tests.rs` scaffolding + `create` happy path

**Files:**
- Create: `client/tests/registry_tests.rs`

Short helpers live inline in this file (shared only with Task 2–6, no need to promote to `common`).

- [ ] **Step 1: Write the test file scaffold + the first failing test**

```rust
//! Integration tests for `cooklang_sync_client::registry`.

mod common;

use cooklang_sync_client::connection::get_connection;
use cooklang_sync_client::models::{CreateForm, DeleteForm, FileRecord};
use cooklang_sync_client::registry;
use cooklang_sync_client::schema::file_records;
use diesel::prelude::*;
use time::OffsetDateTime;

/// Build a `CreateForm` with a deterministic `modified_at` (whole seconds) so
/// equality comparisons via `PartialEq<CreateForm> for FileRecord` are stable.
fn sample_create(path: &str, size: i64, ns: i32) -> CreateForm {
    CreateForm {
        jid: None,
        path: path.to_string(),
        deleted: false,
        size,
        modified_at: OffsetDateTime::from_unix_timestamp(1_700_000_000).unwrap(),
        namespace_id: ns,
    }
}

fn sample_delete(from: &FileRecord) -> DeleteForm {
    DeleteForm {
        path: from.path.clone(),
        jid: None,
        size: from.size,
        modified_at: from.modified_at,
        deleted: true,
        namespace_id: from.namespace_id,
    }
}

#[test]
fn create_inserts_rows_and_returns_count() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    let forms = vec![
        sample_create("a.cook", 10, 1),
        sample_create("b.cook", 20, 1),
    ];
    let n = registry::create(conn, &forms).expect("create should insert");
    assert_eq!(n, 2);

    let all: Vec<FileRecord> = file_records::table
        .select(FileRecord::as_select())
        .order(file_records::id.asc())
        .load(conn)
        .expect("load all");
    assert_eq!(all.len(), 2);
    assert_eq!(all[0].path, "a.cook");
    assert_eq!(all[0].size, 10);
    assert!(all[0].jid.is_none(), "new rows must have no jid yet");
    assert!(all[0].id < all[1].id, "id must be monotonic");
}
```

- [ ] **Step 2: Run it**

```
cargo test -p cooklang-sync-client --test registry_tests create_inserts_rows_and_returns_count
```

Expected: PASS.

- [ ] **Step 3: Commit**

```
git add client/tests/registry_tests.rs
git commit -m "test(client): scaffold registry integration tests + create happy path"
```

---

## Task 2: `registry::update_jid` round-trip

**Files:**
- Modify: `client/tests/registry_tests.rs`

- [ ] **Step 1: Append the test**

```rust
#[test]
fn update_jid_sets_jid_and_preserves_other_columns() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    registry::create(conn, &vec![sample_create("a.cook", 42, 1)]).unwrap();

    let row: FileRecord = file_records::table
        .select(FileRecord::as_select())
        .first(conn)
        .expect("row");
    assert!(row.jid.is_none());
    let original_path = row.path.clone();
    let original_size = row.size;
    let original_mtime = row.modified_at;

    let n = registry::update_jid(conn, &row, 7).expect("update_jid");
    assert_eq!(n, 1);

    let after: FileRecord = file_records::table
        .select(FileRecord::as_select())
        .first(conn)
        .expect("reload");
    assert_eq!(after.jid, Some(7));
    assert_eq!(after.path, original_path);
    assert_eq!(after.size, original_size);
    assert_eq!(after.modified_at, original_mtime);
    assert_eq!(after.deleted, false);
}
```

- [ ] **Step 2: Run it**

```
cargo test -p cooklang-sync-client --test registry_tests update_jid
```

Expected: PASS.

- [ ] **Step 3: Commit**

```
git add client/tests/registry_tests.rs
git commit -m "test(client): cover registry::update_jid round-trip"
```

---

## Task 3: `registry::delete` appends a tombstone

Pins the log-append semantics: `delete` does **not** UPDATE an existing row; it INSERTs a new row with `deleted=true`. This is load-bearing for the `non_deleted` query (latest id wins).

**Files:**
- Modify: `client/tests/registry_tests.rs`

- [ ] **Step 1: Append the test**

```rust
#[test]
fn delete_appends_tombstone_row_rather_than_updating() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    registry::create(conn, &vec![sample_create("a.cook", 10, 1)]).unwrap();
    let live: FileRecord = file_records::table
        .select(FileRecord::as_select())
        .first(conn)
        .unwrap();
    assert_eq!(live.deleted, false);

    let n = registry::delete(conn, &vec![sample_delete(&live)]).expect("delete");
    assert_eq!(n, 1);

    // Two rows for the same path: original (live) + appended tombstone.
    let rows: Vec<FileRecord> = file_records::table
        .filter(file_records::path.eq("a.cook"))
        .select(FileRecord::as_select())
        .order(file_records::id.asc())
        .load(conn)
        .unwrap();
    assert_eq!(rows.len(), 2, "delete is append-only; original row is preserved");
    assert_eq!(rows[0].deleted, false);
    assert_eq!(rows[1].deleted, true);
    assert!(rows[1].id > rows[0].id, "tombstone id must be newer");
}
```

- [ ] **Step 2: Run it**

```
cargo test -p cooklang-sync-client --test registry_tests delete_appends
```

Expected: PASS.

- [ ] **Step 3: Commit**

```
git add client/tests/registry_tests.rs
git commit -m "test(client): pin registry::delete append-only tombstone semantics"
```

---

## Task 4: `registry::non_deleted` — latest-per-path + deleted filter + namespace isolation

**Files:**
- Modify: `client/tests/registry_tests.rs`

- [ ] **Step 1: Append the test**

```rust
#[test]
fn non_deleted_returns_latest_live_row_per_path_scoped_by_namespace() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    // Namespace 1: "a.cook" created, then re-created (modified), then namespace 2
    // has its own unrelated "a.cook".
    registry::create(
        conn,
        &vec![
            sample_create("a.cook", 10, 1), // id 1 (ns 1, old)
            sample_create("b.cook", 20, 1), // id 2 (ns 1)
        ],
    )
    .unwrap();

    // Modified-file path: append a new CreateForm with a larger size.
    let mut modified = sample_create("a.cook", 11, 1);
    modified.modified_at = OffsetDateTime::from_unix_timestamp(1_700_000_500).unwrap();
    registry::create(conn, &vec![modified]).unwrap(); // id 3 (ns 1, newer)

    // A deleted file in ns 1.
    registry::create(conn, &vec![sample_create("c.cook", 30, 1)]).unwrap(); // id 4
    let c: FileRecord = file_records::table
        .filter(file_records::path.eq("c.cook"))
        .select(FileRecord::as_select())
        .first(conn)
        .unwrap();
    registry::delete(conn, &vec![sample_delete(&c)]).unwrap(); // id 5 (tombstone)

    // Namespace 2 rows must not leak into namespace 1.
    registry::create(conn, &vec![sample_create("a.cook", 999, 2)]).unwrap(); // id 6

    let live = registry::non_deleted(conn, 1).expect("non_deleted ns 1");
    let paths: Vec<(&str, i64)> = live.iter().map(|r| (r.path.as_str(), r.size)).collect();
    // Expect: a.cook (size 11, latest live row) + b.cook. c.cook is hidden (tombstone is latest).
    assert_eq!(paths, vec![("a.cook", 11), ("b.cook", 20)]);

    let live_ns2 = registry::non_deleted(conn, 2).unwrap();
    assert_eq!(live_ns2.len(), 1);
    assert_eq!(live_ns2[0].path, "a.cook");
    assert_eq!(live_ns2[0].size, 999);
}

#[test]
fn non_deleted_empty_db_returns_empty_vec() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");
    let live = registry::non_deleted(conn, 1).unwrap();
    assert!(live.is_empty());
}
```

- [ ] **Step 2: Run it**

```
cargo test -p cooklang-sync-client --test registry_tests non_deleted
```

Expected: PASS.

- [ ] **Step 3: Commit**

```
git add client/tests/registry_tests.rs
git commit -m "test(client): cover registry::non_deleted latest-per-path + namespace scoping"
```

---

## Task 5: `registry::updated_locally` — null-jid latest-per-path + namespace scoping

**Files:**
- Modify: `client/tests/registry_tests.rs`

- [ ] **Step 1: Append the test**

```rust
#[test]
fn updated_locally_returns_latest_null_jid_rows_per_path_scoped_by_namespace() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).expect("checkout");

    // ns 1: create "a.cook" (id 1), sync it (jid=5), re-modify (id 3, null jid).
    registry::create(conn, &vec![sample_create("a.cook", 10, 1)]).unwrap();
    let a1: FileRecord = file_records::table
        .filter(file_records::path.eq("a.cook"))
        .select(FileRecord::as_select())
        .first(conn)
        .unwrap();
    registry::update_jid(conn, &a1, 5).unwrap();

    let mut a2 = sample_create("a.cook", 11, 1);
    a2.modified_at = OffsetDateTime::from_unix_timestamp(1_700_000_500).unwrap();
    registry::create(conn, &vec![a2]).unwrap();

    // ns 1: "b.cook" created and synced — should NOT appear.
    registry::create(conn, &vec![sample_create("b.cook", 20, 1)]).unwrap();
    let b: FileRecord = file_records::table
        .filter(file_records::path.eq("b.cook"))
        .select(FileRecord::as_select())
        .first(conn)
        .unwrap();
    registry::update_jid(conn, &b, 6).unwrap();

    // ns 2: unrelated unsynced row — must not leak into ns 1.
    registry::create(conn, &vec![sample_create("x.cook", 30, 2)]).unwrap();

    let pending = registry::updated_locally(conn, 1).unwrap();
    let paths: Vec<(&str, i64)> = pending.iter().map(|r| (r.path.as_str(), r.size)).collect();
    assert_eq!(paths, vec![("a.cook", 11)]);

    let pending_ns2 = registry::updated_locally(conn, 2).unwrap();
    assert_eq!(pending_ns2.len(), 1);
    assert_eq!(pending_ns2[0].path, "x.cook");
}

#[test]
fn updated_locally_empty_db_returns_empty_vec() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).unwrap();
    assert!(registry::updated_locally(conn, 1).unwrap().is_empty());
}
```

- [ ] **Step 2: Run it**

```
cargo test -p cooklang-sync-client --test registry_tests updated_locally
```

Expected: PASS.

- [ ] **Step 3: Commit**

```
git add client/tests/registry_tests.rs
git commit -m "test(client): cover registry::updated_locally pending-local filter"
```

---

## Task 6: `registry::latest_jid` — empty, mixed, and namespace-scoped

**Files:**
- Modify: `client/tests/registry_tests.rs`

Pin: with zero rows carrying a jid, `latest_jid` surfaces a `diesel::NotFound` error (the underlying `.first()` returns NotFound). Document this behavior — callers must handle NotFound to mean "no synced rows yet".

- [ ] **Step 1: Append the tests**

```rust
#[test]
fn latest_jid_returns_not_found_on_empty_db() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).unwrap();
    let err = registry::latest_jid(conn, 1).expect_err("no jid rows = NotFound");
    assert!(
        matches!(err, diesel::result::Error::NotFound),
        "empty DB should produce NotFound, got: {err:?}"
    );
}

#[test]
fn latest_jid_returns_highest_jid_in_namespace_and_ignores_null_jid_rows() {
    let (pool, _dir) = common::fresh_client_pool();
    let conn = &mut get_connection(&pool).unwrap();

    // ns 1: three rows, jids 3, 7, and null.
    registry::create(
        conn,
        &vec![
            sample_create("a.cook", 10, 1),
            sample_create("b.cook", 20, 1),
            sample_create("c.cook", 30, 1),
        ],
    )
    .unwrap();
    let mut rows: Vec<FileRecord> = file_records::table
        .filter(file_records::namespace_id.eq(1))
        .select(FileRecord::as_select())
        .order(file_records::id.asc())
        .load(conn)
        .unwrap();
    rows.sort_by_key(|r| r.id);
    registry::update_jid(conn, &rows[0], 3).unwrap();
    registry::update_jid(conn, &rows[1], 7).unwrap();
    // rows[2] stays jid=None.

    // ns 2: jid 100 — must not bleed into ns 1's latest_jid.
    registry::create(conn, &vec![sample_create("x.cook", 1, 2)]).unwrap();
    let x: FileRecord = file_records::table
        .filter(file_records::namespace_id.eq(2))
        .select(FileRecord::as_select())
        .first(conn)
        .unwrap();
    registry::update_jid(conn, &x, 100).unwrap();

    assert_eq!(registry::latest_jid(conn, 1).unwrap(), 7);
    assert_eq!(registry::latest_jid(conn, 2).unwrap(), 100);
}
```

- [ ] **Step 2: Run them**

```
cargo test -p cooklang-sync-client --test registry_tests latest_jid
```

Expected: PASS.

- [ ] **Step 3: Commit**

```
git add client/tests/registry_tests.rs
git commit -m "test(client): cover registry::latest_jid empty + namespace scoping"
```

---

## Task 7: `indexer::check_index_once` — FS-driven registry convergence

This is the single largest test file in Plan 2. It exercises the filesystem → registry diff through the real public entrypoint.

**Files:**
- Create: `client/tests/indexer_tests.rs`

- [ ] **Step 1: Write the scaffold + "new file" test**

```rust
//! Integration tests for `cooklang_sync_client::indexer::check_index_once`.
//!
//! Each test creates a fresh `TempDir`, drops files into it, runs
//! `check_index_once`, and asserts the resulting state of `file_records`.

mod common;

use cooklang_sync_client::connection::get_connection;
use cooklang_sync_client::indexer::check_index_once;
use cooklang_sync_client::models::FileRecord;
use cooklang_sync_client::registry;
use cooklang_sync_client::schema::file_records;
use diesel::prelude::*;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use tempfile::TempDir;

const NS: i32 = 1;

/// Build an empty storage tempdir the indexer can scan.
fn storage_dir() -> TempDir {
    TempDir::new().expect("tempdir")
}

fn write(storage: &TempDir, rel: &str, bytes: &[u8]) -> PathBuf {
    let full = storage.path().join(rel);
    if let Some(parent) = full.parent() {
        fs::create_dir_all(parent).unwrap();
    }
    fs::write(&full, bytes).unwrap();
    full
}

#[test]
fn check_index_once_records_new_file() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();
    write(&storage, "recipes/soup.cook", b"title: Soup\n");

    let changed = check_index_once(&pool, storage.path(), NS).expect("scan");
    assert!(changed, "new file must cause an update");

    let conn = &mut get_connection(&pool).unwrap();
    let live = registry::non_deleted(conn, NS).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].path, "recipes/soup.cook");
    assert_eq!(live[0].deleted, false);
    assert!(live[0].jid.is_none(), "indexer records are always unsynced");
    assert!(live[0].size > 0);
}
```

- [ ] **Step 2: Run it**

```
cargo test -p cooklang-sync-client --test indexer_tests check_index_once_records_new_file
```

Expected: PASS.

- [ ] **Step 3: Append "unchanged file is a no-op"**

```rust
#[test]
fn check_index_once_is_noop_when_nothing_changed() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();
    write(&storage, "a.cook", b"hello");

    assert!(check_index_once(&pool, storage.path(), NS).unwrap());
    assert!(!check_index_once(&pool, storage.path(), NS).unwrap(),
        "second scan with no FS changes must return false");

    let conn = &mut get_connection(&pool).unwrap();
    let rows: i64 = file_records::table
        .count()
        .get_result(conn)
        .unwrap();
    assert_eq!(rows, 1, "no duplicate row appended on no-op scan");
}
```

- [ ] **Step 4: Append "modified file appends a new CreateForm row"**

Pin: the indexer does NOT emit an `UpdateForm`; it appends a fresh `CreateForm` whose larger id makes it the new "latest" in `non_deleted`'s `max(id)` subquery. Without this pin the test would silently allow a future refactor to in-place UPDATE.

```rust
#[test]
fn check_index_once_appends_a_new_row_when_file_is_modified() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();
    let path = write(&storage, "a.cook", b"v1");
    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    // Rewrite content with a different size and advance mtime by >=1 second
    // so truncate_to_seconds still produces a distinguishable value.
    std::thread::sleep(std::time::Duration::from_millis(1100));
    fs::write(&path, b"v2-longer").unwrap();

    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    let conn = &mut get_connection(&pool).unwrap();
    let rows: Vec<FileRecord> = file_records::table
        .filter(file_records::path.eq("a.cook"))
        .select(FileRecord::as_select())
        .order(file_records::id.asc())
        .load(conn)
        .unwrap();
    assert_eq!(rows.len(), 2, "modified file => new CreateForm appended, not in-place update");
    assert!(rows[0].size != rows[1].size || rows[0].modified_at != rows[1].modified_at);

    // non_deleted yields the newer row.
    let live = registry::non_deleted(conn, NS).unwrap();
    assert_eq!(live.len(), 1);
    assert_eq!(live[0].id, rows[1].id);
}
```

- [ ] **Step 5: Append "deleted file records a tombstone"**

```rust
#[test]
fn check_index_once_records_delete_when_file_is_removed() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();
    let path = write(&storage, "gone.cook", b"bye");
    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    fs::remove_file(&path).unwrap();
    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    let conn = &mut get_connection(&pool).unwrap();
    let live = registry::non_deleted(conn, NS).unwrap();
    assert!(live.is_empty(), "removed file must be absent from non_deleted");

    let rows: Vec<FileRecord> = file_records::table
        .filter(file_records::path.eq("gone.cook"))
        .select(FileRecord::as_select())
        .order(file_records::id.asc())
        .load(conn)
        .unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].deleted, false);
    assert_eq!(rows[1].deleted, true);
}
```

- [ ] **Step 6: Append "recursion + ineligible-extension filter"**

```rust
#[test]
fn check_index_once_skips_ineligible_files_and_recurses_into_subdirs() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();

    // eligible (chunker::is_text / is_binary):
    write(&storage, "top.cook", b"a");
    write(&storage, "nested/dir/inner.md", b"b");
    write(&storage, "photo.jpg", &[0xff, 0xd8, 0xff]);
    // ineligible extensions:
    write(&storage, "notes.txt", b"c");
    write(&storage, "script.rs", b"d");

    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    let conn = &mut get_connection(&pool).unwrap();
    let mut paths: Vec<String> = registry::non_deleted(conn, NS)
        .unwrap()
        .into_iter()
        .map(|r| r.path)
        .collect();
    paths.sort();
    assert_eq!(paths, vec![
        "nested/dir/inner.md".to_string(),
        "photo.jpg".to_string(),
        "top.cook".to_string(),
    ]);
}
```

- [ ] **Step 7: Append "dotfile allowlist" behavior pin**

```rust
#[test]
fn check_index_once_indexes_dotfiles_on_the_is_text_allowlist() {
    // Pins current chunker::is_text behavior: `.shopping-list`,
    // `.shopping-checked`, `.bookmarks` are explicitly allowed even though
    // they have no extension and start with a dot.
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();

    write(&storage, ".shopping-list", b"milk");
    write(&storage, ".hidden-random", b"not included");

    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    let conn = &mut get_connection(&pool).unwrap();
    let mut paths: Vec<String> = registry::non_deleted(conn, NS)
        .unwrap()
        .into_iter()
        .map(|r| r.path)
        .collect();
    paths.sort();
    assert_eq!(paths, vec![".shopping-list".to_string()]);
}
```

- [ ] **Step 8: Append "symlinks are skipped"**

```rust
#[test]
fn check_index_once_skips_symlinks() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();

    let target = write(&storage, "real.cook", b"r");
    let link = storage.path().join("link.cook");
    symlink(&target, &link).expect("symlink");

    assert!(check_index_once(&pool, storage.path(), NS).unwrap());

    let conn = &mut get_connection(&pool).unwrap();
    let mut paths: Vec<String> = registry::non_deleted(conn, NS)
        .unwrap()
        .into_iter()
        .map(|r| r.path)
        .collect();
    paths.sort();
    assert_eq!(paths, vec!["real.cook".to_string()],
        "symlink entry must be skipped by filter_eligible");
}
```

- [ ] **Step 9: Run the whole file**

```
cargo test -p cooklang-sync-client --test indexer_tests
```

Expected: all tests PASS.

- [ ] **Step 10: Commit**

```
git add client/tests/indexer_tests.rs
git commit -m "test(client): cover indexer::check_index_once filesystem-driven behavior"
```

---

## Task 8: `indexer::run` — event-driven loop integration

Exercises the public `run` async entrypoint with a real tokio runtime, real filesystem, real pool, and real mpsc channels. Verifies:

- The initial scan emits `IndexerUpdateEvent::Updated` into the `updated_tx` channel.
- Pushing a `DebounceEventResult` into `local_file_update_rx` after a filesystem change re-runs the scan and emits another update event.
- The `CancellationToken` cleanly exits the loop.

**Files:**
- Create: `client/tests/indexer_run_tests.rs`

- [ ] **Step 1: Write the test file**

```rust
//! Integration tests for `cooklang_sync_client::indexer::run` (the async loop).

mod common;

use cooklang_sync_client::connection::get_connection;
use cooklang_sync_client::indexer::run;
use cooklang_sync_client::models::{FileRecord, IndexerUpdateEvent};
use cooklang_sync_client::schema::file_records;
use diesel::prelude::*;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use std::fs;
use std::time::Duration;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const NS: i32 = 1;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_emits_update_event_on_initial_scan_and_on_subsequent_fs_event() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = tempfile::TempDir::new().unwrap();

    // Seed one file BEFORE the loop starts so the initial scan finds work.
    fs::write(storage.path().join("a.cook"), b"v1").unwrap();

    let (fs_tx, fs_rx) = mpsc::channel::<notify_debouncer_mini::DebounceEventResult>(8);
    let (updated_tx, mut updated_rx) = mpsc::channel::<IndexerUpdateEvent>(8);

    let token = CancellationToken::new();
    let token_for_loop = token.clone();

    let pool_cloned = pool.clone();
    let storage_path = storage.path().to_path_buf();
    let join = tokio::spawn(async move {
        run(
            token_for_loop,
            None, // no listener
            &pool_cloned,
            &storage_path,
            NS,
            fs_rx,
            updated_tx,
        )
        .await
    });

    // 1. Initial scan must emit an update (the seeded file is new).
    let first = timeout(Duration::from_secs(5), updated_rx.next())
        .await
        .expect("initial Updated event within 5s")
        .expect("channel not closed");
    assert!(matches!(first, IndexerUpdateEvent::Updated));

    // 2. Create a new file and push a synthetic FS event so the loop re-scans.
    //    Sleep >1s so modified_at truncated-to-seconds differs from the seed.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    fs::write(storage.path().join("b.cook"), b"new").unwrap();
    fs_tx
        .clone()
        .send(Ok(Vec::new()))
        .await
        .expect("push synthetic debounce event");

    let second = timeout(Duration::from_secs(5), updated_rx.next())
        .await
        .expect("second Updated event within 5s")
        .expect("channel not closed");
    assert!(matches!(second, IndexerUpdateEvent::Updated));

    // 3. Cancel and verify clean exit.
    token.cancel();
    let res = timeout(Duration::from_secs(5), join)
        .await
        .expect("loop must exit within 5s of cancel")
        .expect("task joined");
    res.expect("run returns Ok on cancel");

    // 4. Both files should be in the registry.
    let conn = &mut get_connection(&pool).unwrap();
    let mut paths: Vec<String> = file_records::table
        .filter(file_records::deleted.eq(false))
        .select(FileRecord::as_select())
        .load(conn)
        .unwrap()
        .into_iter()
        .map(|r| r.path)
        .collect();
    paths.sort();
    assert_eq!(paths, vec!["a.cook".to_string(), "b.cook".to_string()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_exits_cleanly_when_cancelled_immediately() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = tempfile::TempDir::new().unwrap();
    let (_fs_tx, fs_rx) = mpsc::channel::<notify_debouncer_mini::DebounceEventResult>(1);
    let (updated_tx, _updated_rx) = mpsc::channel::<IndexerUpdateEvent>(1);

    let token = CancellationToken::new();
    token.cancel(); // pre-cancelled

    let result = timeout(
        Duration::from_secs(5),
        run(
            token,
            None,
            &pool,
            storage.path(),
            NS,
            fs_rx,
            updated_tx,
        ),
    )
    .await
    .expect("run must exit within 5s when token is already cancelled")
    .expect("run returns Ok");

    // Nothing to assert beyond "it exited" — no seed files, no events.
    let _ = result;
}
```

- [ ] **Step 2: Run it**

```
cargo test -p cooklang-sync-client --test indexer_run_tests
```

Expected: both tests PASS within a few seconds.

- [ ] **Step 3: Commit**

```
git add client/tests/indexer_run_tests.rs
git commit -m "test(client): cover indexer::run event-driven loop + cancellation"
```

---

## Task 9: Branch-level sanity

- [ ] **Step 1: Run the full client test suite**

```
cargo test -p cooklang-sync-client
```

Expected: all tests pass (including Plan 1 suite, carried forward from `main` once rebased). No new warnings on our new files.

- [ ] **Step 2: Clippy**

```
cargo clippy -p cooklang-sync-client --tests --no-deps -- -D warnings
```

Expected: no warnings on the new test files.

- [ ] **Step 3: No commit (verification only)**

If either command fails, fix the offending file(s), re-run, and amend/commit the fix before moving on.

---

## Notes for reviewers

- **Why append-only delete** is explicitly pinned: the `non_deleted` query's `max(id)` logic depends on tombstones being newer rows rather than UPDATEs. If someone refactors `delete` into an UPDATE, `non_deleted` silently breaks for previously-live paths. Tests Task 3 + Task 7 Step 5 guard against that.
- **No `UpdateForm` on the indexer path**: `check_index_once` only uses `create` and `delete`. Task 7 Step 4 locks this in.
- **`latest_jid` on empty = NotFound, not `Ok(0)`**: Task 6 pins this so callers are forced to handle the two "no jid yet" cases deliberately.
- **Namespace scoping** is tested explicitly in every query test; cross-namespace bleed is a load-bearing correctness property on a multi-tenant client DB.
- **mtime truncation**: tests that mutate a file wait >=1.1s before rewriting so the whole-seconds `modified_at` is distinguishable. Otherwise equality via `PartialEq<CreateForm> for FileRecord` could collapse the "modified" case into a no-op.
