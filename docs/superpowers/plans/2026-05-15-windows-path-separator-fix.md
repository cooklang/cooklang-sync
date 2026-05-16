# Windows Path-Separator Fix Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop `cooklang-sync-client` from uploading spurious tombstones on Windows by normalizing indexer path keys to forward slashes, and prevent regressions of this bug class by adding multi-platform test CI.

**Architecture:** One-line fix in `client/src/indexer.rs::build_file_record` to use `path_slash::PathExt::to_slash_lossy()` instead of `to_string_lossy()`. Two regression tests (one unit test in the same file, one integration test under `client/tests/`). A new GitHub Actions workflow that runs `cargo test --workspace` on Ubuntu, macOS, and Windows so platform-dependent bugs surface in CI.

**Tech Stack:** Rust 2021 (workspace `cooklang-sync-client` and `cooklang-sync-server`), `diesel` + `libsqlite3-sys` (bundled SQLite, used for in-test DB), `tempfile`, `path-slash` 0.2.1 (already a runtime dep), GitHub Actions.

---

## File Structure

- **Modify** `client/src/indexer.rs` — replace `to_string_lossy()` with `to_slash_lossy()` at line 189; add a `path_slash` import; extend `#[cfg(test)] mod tests` with a unit test for `build_file_record`.
- **Modify** `client/tests/indexer_tests.rs` — append an integration test that simulates "downloader-wrote-a-file" state and runs `check_index_once`, asserting no spurious `DeleteForm` is recorded. The repo already has this file with `mod common;` and a `write` helper; the new test reuses both.
- **Create** `.github/workflows/test.yml` — multi-platform CI matrix (Ubuntu, macOS, Windows) running `cargo test --workspace --all-features` on push and pull request.

Each is self-contained; tasks below cover them in order.

---

## Task 1: Add failing unit test for `build_file_record` forward-slash normalization

**Files:**
- Modify: `client/src/indexer.rs` (extend `#[cfg(test)] mod tests` near the bottom of the file)

This test asserts that `build_file_record` returns a `CreateForm.path` using `/` regardless of how the input path was constructed. It uses a real temp-dir file because `build_file_record` calls `path.metadata()`.

- [ ] **Step 1: Add `tempfile` import inside the test module**

`tempfile` is already in `[dev-dependencies]` (`client/Cargo.toml:53`). At the top of `client/src/indexer.rs`'s existing `mod tests` block (around line 237), add:

```rust
    use tempfile::TempDir;
    use std::fs::{self, File};
```

Place these `use` statements alongside `use super::*;`. Leave the existing tests untouched.

- [ ] **Step 2: Add the failing unit test at the bottom of `mod tests`**

Append the following test to the end of `mod tests` in `client/src/indexer.rs` (before its closing `}`):

```rust
    #[test]
    fn build_file_record_normalises_path_separators_to_forward_slash() {
        // The indexer's HashMap is keyed on the returned path string; the
        // downloader inserts registry rows using forward-slash paths from the
        // server. If these disagree, every downloaded file looks "missing on
        // disk" to the indexer and triggers a spurious tombstone upload.
        // See https://github.com/cooklang/cooklang-sync/issues/18.

        let tmp = TempDir::new().expect("create tempdir");
        let base = tmp.path();

        // Construct the nested path the way WalkDir would produce it on the
        // host: a Path built from native components. On Windows this contains
        // backslashes; on Unix it contains forward slashes. Either way, the
        // returned CreateForm.path must use forward slashes.
        let nested_dir = base.join("plats");
        fs::create_dir_all(&nested_dir).expect("create nested dir");
        let file_path = nested_dir.join("pates-carbo.cook");
        File::create(&file_path).expect("create file");

        let record = build_file_record(&file_path, base, 1).expect("build_file_record");

        assert!(
            !record.path.contains('\\'),
            "path must not contain backslash, got {:?}",
            record.path
        );
        assert_eq!(record.path, "plats/pates-carbo.cook");
    }
```

- [ ] **Step 3: Run the new test and verify it fails on Windows behaviour**

On macOS or Linux this test passes today because the native separator is `/`. The point of writing it first is documentary: it locks in the contract and will fail on Windows under current code. Run it locally to confirm it compiles and passes on the host platform:

```bash
cd client && cargo test --lib indexer::tests::build_file_record_normalises_path_separators_to_forward_slash -- --nocapture
```

Expected on macOS/Linux: `test result: ok. 1 passed`.

Note: this test would FAIL on Windows under the current `to_string_lossy()` because `nested_dir.join("pates-carbo.cook")` on Windows yields `plats\pates-carbo.cook`. We confirm that failure mode via CI in Task 5 — we don't need a Windows machine locally.

- [ ] **Step 4: Commit**

```bash
git add client/src/indexer.rs
git commit -m "test: assert build_file_record returns forward-slash paths"
```

---

## Task 2: Apply the path-slash normalization fix

**Files:**
- Modify: `client/src/indexer.rs` (imports at the top, line 189 in `build_file_record`)

- [ ] **Step 1: Add the `path_slash` import**

At the top of `client/src/indexer.rs`, after the existing `use std::path::Path;` (line 6), add:

```rust
use path_slash::PathExt as _;
```

Mirror the alias style used in `client/src/remote.rs:2` (`use path_slash::PathExt as _;`). The `_` binding silences any unused-import warning while still importing the trait.

- [ ] **Step 2: Replace the path conversion call**

In `client/src/indexer.rs`, line 189 (inside `build_file_record`), change:

```rust
    let path = path.strip_prefix(base)?.to_string_lossy().into_owned();
```

to:

```rust
    let path = path.strip_prefix(base)?.to_slash_lossy().into_owned();
```

`to_slash_lossy()` returns a `Cow<'_, str>`; `into_owned()` works the same as before. The behaviour difference is that backslash separators on Windows are converted to `/`.

- [ ] **Step 3: Run the unit test from Task 1**

```bash
cd client && cargo test --lib indexer::tests::build_file_record_normalises_path_separators_to_forward_slash
```

Expected: PASS on macOS/Linux (it already passed). On Windows (which we'll verify via CI), this is the change that makes it pass.

- [ ] **Step 4: Run the full indexer test module to confirm no regressions**

```bash
cd client && cargo test --lib indexer::tests
```

Expected: all four tests pass (the three pre-existing `truncate_to_seconds_*` tests plus the new one).

- [ ] **Step 5: Run the full client test suite**

```bash
cd client && cargo test
```

Expected: no failures. (`chunk_property_tests.rs` will run; existing property tests should pass.)

- [ ] **Step 6: Commit**

```bash
git add client/src/indexer.rs
git commit -m "fix: normalise indexer path keys to forward slashes (#18)"
```

---

## Task 3: Add integration test for spurious-tombstone scenario

**Files:**
- Modify: `client/tests/indexer_tests.rs` (append a new `#[test]` function)

This test simulates the post-download state that breaks on Windows: a `FileRecord` with a forward-slash path already exists in the registry (as `check_download_once` would have inserted), and the corresponding file is on disk. Running `check_index_once` must NOT mark the record deleted.

The test reuses the existing helpers already in `indexer_tests.rs` on main: `common::fresh_client_pool()`, `storage_dir()`, `write()`, and the `NS` constant.

- [ ] **Step 1: Add the needed imports to `client/tests/indexer_tests.rs`**

Inspect the top of `client/tests/indexer_tests.rs`. It currently imports:

```rust
use cooklang_sync_client::connection::get_connection;
use cooklang_sync_client::indexer::check_index_once;
use cooklang_sync_client::models::FileRecord;
use cooklang_sync_client::registry;
use cooklang_sync_client::schema::file_records;
```

Add `CreateForm` to the `models` import so it becomes:

```rust
use cooklang_sync_client::models::{CreateForm, FileRecord};
```

Also add `time::OffsetDateTime` to the imports (the test needs it to set `modified_at`):

```rust
use time::OffsetDateTime;
```

Place it alongside the other `use` lines near the top of the file.

- [ ] **Step 2: Append the new test at the end of `client/tests/indexer_tests.rs`**

Add this test as the last item in the file (just before the file's final newline; do not add it inside any `cfg`-gated block — it must run on every platform):

```rust
#[test]
fn check_index_once_does_not_tombstone_just_downloaded_file() {
    // Regression test for https://github.com/cooklang/cooklang-sync/issues/18.
    //
    // The downloader inserts FileRecord rows using forward-slash paths from
    // server responses. The indexer builds its on-disk view by walking the
    // filesystem and converting Path values to strings. If the indexer keys
    // paths differently from the downloader, `compare_records` treats the
    // same file as both "missing on disk" (-> DeleteForm) and "new on disk"
    // (-> CreateForm), which uploads a tombstone to the server. On Windows
    // this destroys recipes on first sync.
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = storage_dir();

    // Simulate what the downloader does: write the file to disk with a
    // forward-slash relative path and insert a non-deleted FileRecord
    // with the same forward-slash path (this is the server's canonical form).
    let rel = "plats/pates-carbo.cook";
    let content = b"-- a recipe --";
    let full = write(&storage, rel, content);

    let modified_at = {
        let meta = fs::metadata(&full).expect("stat file");
        let m = OffsetDateTime::from(meta.modified().expect("modified mtime"));
        m.replace_nanosecond(0).unwrap_or(m) // match build_file_record's truncate_to_seconds
    };

    {
        let conn = &mut get_connection(&pool).unwrap();
        let form = CreateForm {
            jid: Some(42), // downloaded records carry a server jid
            path: rel.to_string(),
            deleted: false,
            size: content.len() as i64,
            modified_at,
            namespace_id: NS,
        };
        registry::create(conn, &vec![form]).expect("seed registry");
    }

    // Run the indexer's filesystem-vs-registry comparison.
    check_index_once(&pool, storage.path(), NS).expect("check_index_once");

    // The downloaded file must still be the only active row, and it must
    // not have been soft-deleted by a spurious DeleteForm.
    let conn = &mut get_connection(&pool).unwrap();
    let live = registry::non_deleted(conn, NS).unwrap();
    assert_eq!(live.len(), 1, "expected exactly one active row, got {:?}", live);
    assert_eq!(live[0].path, rel);
    assert!(!live[0].deleted, "downloaded file must not be soft-deleted by the indexer");
}
```

Note: `write` (the file-writing helper) returns `PathBuf` already in this file (`indexer_tests.rs:27-34`). `fs` is already imported at the top. `storage_dir` and `NS` are already defined locally. No new helpers needed.

- [ ] **Step 3: Run the new test**

```bash
cd client && cargo test --test indexer_tests check_index_once_does_not_tombstone_just_downloaded_file
```

Expected on macOS/Linux: PASS — Task 2 already landed the fix, and on Unix-likes the bug never reproduced. The test's value is twofold: (a) on Windows in CI it would FAIL without the fix and PASSES with it; (b) it locks the indexer/downloader path-key contract for future changes.

If you suspect the test is a no-op (e.g., trivially passing because `non_deleted` filtering masks the bug), confirm by checking the row count too: the test asserts `live.len() == 1`, so an additional `deleted: true` row would still leave `live.len() == 1` but the integration test from `indexer_tests.rs` line 119 already covers the deletion path. Together they pin both directions.

- [ ] **Step 4: Run the full `indexer_tests` integration suite to confirm no regressions**

```bash
cd client && cargo test --test indexer_tests
```

Expected: all tests pass (the existing tests plus the new one).

- [ ] **Step 5: Commit**

```bash
git add client/tests/indexer_tests.rs
git commit -m "test: regression for #18 spurious tombstone after download"
```

---

## Task 4: Verify no other path-key construction sites exist

**Files:** none modified — this is a verification step recorded explicitly so the executor doesn't skip it.

The spec claims `indexer.rs:189` is the only path-key construction site. Confirm before declaring the fix complete.

- [ ] **Step 1: Grep for other `to_string_lossy` callers and `to_str()` calls in the client crate**

```bash
cd client && grep -rn "to_string_lossy\|\.to_str()" src/
```

Expected output (exactly these three matches):

```
src/chunker.rs:235:        let file_name_str = file_name.to_string_lossy();
src/indexer.rs:189:    let path = path.strip_prefix(base)?.to_slash_lossy().into_owned();
src/remote.rs:307:                        .and_then(|v| v.to_str().ok())
```

(After Task 2, `indexer.rs:189` is the `to_slash_lossy` line. The line number may shift by a line or two if the `use path_slash::PathExt as _;` import was added higher up — that's fine; what matters is that the match is in `build_file_record`.)

- [ ] **Step 2: Confirm the two remaining matches are non-path-key uses**

- `src/chunker.rs:235` — operates on `file_name()` (a single component, no separators) and compares to literals like `.shopping-list`. Not a registry key. No change needed.
- `src/remote.rs:307` — `to_str()` is called on a `HeaderValue`, not a `Path`. Not a path at all. No change needed.

If you find any additional `to_string_lossy()` or `.to_str()` call on a `Path` whose result is used as a HashMap key or registry key, STOP and add a task to normalise it. As of this writing, there are none.

- [ ] **Step 3: No commit needed for this verification step.**

---

## Task 5: Add multi-platform test CI workflow

**Files:**
- Create: `.github/workflows/test.yml`

The repo currently has no workflow that runs `cargo test` on push or PR — that gap is the structural reason this bug went unnoticed for years. Add one that runs on Ubuntu, macOS, and Windows so Windows-specific bugs surface immediately.

- [ ] **Step 1: Create the workflow file**

Create `.github/workflows/test.yml` with the following exact content:

```yaml
name: Test

on:
  push:
    branches:
      - '**'
  pull_request:
    branches:
      - main

env:
  CARGO_TERM_COLOR: always
  RUST_BACKTRACE: 1

jobs:
  test:
    name: cargo test (${{ matrix.os }})
    strategy:
      fail-fast: false
      matrix:
        os: [ubuntu-latest, macos-latest, windows-latest]
    runs-on: ${{ matrix.os }}
    steps:
      - name: Checkout sources
        uses: actions/checkout@v4

      - name: Install Rust toolchain (stable)
        uses: dtolnay/rust-toolchain@stable

      - name: Cache cargo registry and target
        uses: actions/cache@v4
        with:
          path: |
            ~/.cargo/registry/index/
            ~/.cargo/registry/cache/
            ~/.cargo/git/db/
            target/
          key: ${{ runner.os }}-cargo-test-${{ hashFiles('**/Cargo.lock') }}
          restore-keys: ${{ runner.os }}-cargo-test-

      - name: cargo test --workspace
        run: cargo test --workspace
```

Notes on the choices:
- `branches: ['**']` for `push` runs on every branch the workflow file is present on, matching standard "test on every push" expectations.
- `fail-fast: false` lets the matrix complete so we see failures on every OS independently, not just whichever one fails first.
- The command is `cargo test --workspace` (default features) — **deliberately not `--all-features`**. The server crate exposes `database_postgres` as a non-default feature that links `libpq`, which is not available out of the box on the Windows runner and is unrelated to this fix. Default features give us `database_sqlite` with `libsqlite3-sys/bundled` for the server and the client's normal build, which is exactly what the new tests need.
- No coverage / lint / format steps — out of scope for this spec.

- [ ] **Step 2: Validate the workflow YAML locally (optional but quick)**

If you have `actionlint` installed:

```bash
actionlint .github/workflows/test.yml
```

Expected: no output (no errors). If `actionlint` isn't installed, skip — GitHub will validate on push.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/test.yml
git commit -m "ci: run cargo test on ubuntu, macos, windows"
```

- [ ] **Step 4: Push the branch and confirm the workflow runs**

After pushing the branch:

```bash
gh run list --workflow=test.yml --limit 5
```

Expected: a run for the just-pushed commit appears, with three jobs (`cargo test (ubuntu-latest)`, `cargo test (macos-latest)`, `cargo test (windows-latest)`).

Wait for completion and check status:

```bash
gh run watch
```

Expected outcome:
- **Ubuntu** — PASS (existing behavior, no separator mismatch).
- **macOS** — PASS (same).
- **Windows** — PASS *because Task 2's fix is already in place*. The unit test from Task 1 and the integration test from Task 3 both rely on the forward-slash normalization. Without the fix, the Windows job would fail on both. Confirming the Windows job passes is the cross-platform proof that #18 is fixed.

If the Windows job fails on either test, the fix is incomplete — return to Task 2/Task 4 and look for an additional path-key site.

---

## Task 6: Final verification and PR preparation

**Files:** none modified.

- [ ] **Step 1: Re-run the full local test suite**

```bash
cargo test --workspace --all-features
```

Expected: all tests pass on the local host (macOS/Linux).

- [ ] **Step 2: Confirm the CI matrix is green**

```bash
gh run list --workflow=test.yml --limit 1
```

Expected: most recent run for this branch shows all three jobs as `completed success`.

- [ ] **Step 3: Bump the client crate version (optional, ask user)**

The spec notes that both downstream consumers (`cook-sync`, `cooklang-native`) should bump after the fix lands. If the user wants this PR to also publish a new patch version, bump `client/Cargo.toml` from `0.4.12` to `0.4.13`:

```bash
sed -i.bak 's/^version = "0.4.12"$/version = "0.4.13"/' client/Cargo.toml && rm client/Cargo.toml.bak
```

Then update `Cargo.lock`:

```bash
cargo update -p cooklang-sync-client
```

And commit:

```bash
git add client/Cargo.toml Cargo.lock
git commit -m "chore: bump cooklang-sync-client to 0.4.13"
```

If the user prefers to release on a separate cadence, skip this step.

- [ ] **Step 4: Open the PR**

Use the standard PR flow with title `fix: normalise indexer path keys on Windows (#18)` and a body that:
- Links to issue #18.
- Summarises the fix (one-line `to_slash_lossy` change), the regression tests, and the new test CI matrix.
- Notes the deferred defense-in-depth follow-ups from the spec's "Out of scope" section.
