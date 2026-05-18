# Empty Folder Cleanup on Download Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** When the syncer downloads a delete event, also remove any parent directories that became empty as a result — fixing GitHub issue #21 where folders linger on receiving devices after their last file is deleted.

**Architecture:** Single-function change to `Chunker::delete` in `client/src/chunker.rs`. After `fs::remove_file`, walk up `parent()` chain calling `fs::remove_dir` — which natively fails on non-empty directories, giving us strict-empty semantics. Stop at `self.base_path` (never touch the storage root) or on any error.

**Tech Stack:** Rust, tokio (async fs), tempfile (test helper).

**Spec:** `docs/superpowers/specs/2026-05-18-empty-folder-cleanup-design.md`

---

## File Structure

- Modify: `client/src/chunker.rs:147-157` — replace the body of `Chunker::delete` (keeping its signature and behavior on `remove_file` failure).
- Modify: `client/src/chunker.rs` test module (currently ends at line 648) — add four new `#[tokio::test]` functions, mirroring the style of `delete_removes_file_from_storage_dir` (lines 577-598).

No new files. No caller changes — `syncer.rs:313-315` continues to call `chunker.delete(&d.path)` unchanged.

---

## Task 1: Test — empty parent directories get removed

**Files:**
- Modify (test): `client/src/chunker.rs` (append to `mod tests`)

- [ ] **Step 1: Write the failing test**

Append inside the `mod tests { ... }` block in `client/src/chunker.rs` (just before the closing `}` of the module, which is currently at the end of the file):

```rust
#[tokio::test]
async fn delete_removes_empty_parent_directories() {
    // Issue #21: when the syncer deletes the only file inside a
    // nested directory, those now-empty parents must also be removed.
    // Otherwise users see ghost folders accumulate on receiving devices.
    let temp = tempfile::TempDir::new().unwrap();
    let cache = InMemoryCache::new(100, 10_000);
    let mut chunker = Chunker::new(cache, temp.path().to_path_buf());

    let nested_dir = temp.path().join("a").join("b");
    tokio::fs::create_dir_all(&nested_dir).await.unwrap();
    tokio::fs::write(nested_dir.join("c.cook"), b"eggs\n")
        .await
        .unwrap();

    chunker
        .delete("a/b/c.cook")
        .await
        .expect("delete should succeed");

    assert!(
        !temp.path().join("a/b/c.cook").exists(),
        "file should be removed"
    );
    assert!(
        !temp.path().join("a/b").exists(),
        "empty intermediate directory should be removed"
    );
    assert!(
        !temp.path().join("a").exists(),
        "empty grandparent directory should be removed"
    );
    assert!(
        temp.path().exists(),
        "storage root must never be removed"
    );
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -p cooklang-sync-client --lib chunker::tests::delete_removes_empty_parent_directories`

Expected: FAIL — the assertion `!temp.path().join("a/b").exists()` will fail because the current `Chunker::delete` only removes the file, leaving `a/b/` behind.

- [ ] **Step 3: Implement the cleanup walk**

In `client/src/chunker.rs`, replace the entire current body of `pub async fn delete` (lines 147-157):

```rust
pub async fn delete(&mut self, path: &str) -> Result<()> {
    trace!("deleting {:?}", path);
    let full_path = self.full_path(path);

    fs::remove_file(&full_path)
        .await
        .map_err(|e| SyncError::from_io_error(path, e))?;

    // Walk parents upward and remove empty directories. `remove_dir`
    // only succeeds on empty directories, which gives us strict-empty
    // semantics without manually counting entries. Stop at the storage
    // root (never remove it) and on any error (most commonly ENOTEMPTY
    // when a sibling file is present — that's the normal terminating
    // condition, not a failure to propagate).
    let mut parent = full_path.parent();
    while let Some(dir) = parent {
        if dir == self.base_path {
            break;
        }
        if fs::remove_dir(dir).await.is_err() {
            break;
        }
        parent = dir.parent();
    }

    Ok(())
}
```

Note: the existing `// TODO delete folders up too if empty` comment from line 151 is removed by this replacement.

- [ ] **Step 4: Run test to verify it passes**

Run: `cargo test -p cooklang-sync-client --lib chunker::tests::delete_removes_empty_parent_directories`

Expected: PASS.

- [ ] **Step 5: Run existing chunker tests to verify no regression**

Run: `cargo test -p cooklang-sync-client --lib chunker::tests`

Expected: all chunker tests pass, including the existing `delete_removes_file_from_storage_dir` and `test_chunker_delete`.

- [ ] **Step 6: Commit**

```bash
git add client/src/chunker.rs
git commit -m "$(cat <<'EOF'
fix(chunker): remove empty parent dirs on delete

Closes #21. When the syncer downloads a delete event, walk up from
the removed file and remove empty parent directories using
fs::remove_dir (which natively rejects non-empty dirs, giving us
strict-empty semantics). Stop at the storage root and on any error
so a sibling file or permissions issue never fails the download.
EOF
)"
```

---

## Task 2: Test — stop at first non-empty ancestor

**Files:**
- Modify (test): `client/src/chunker.rs` (append to `mod tests`)

- [ ] **Step 1: Write the test**

Append inside the `mod tests { ... }` block in `client/src/chunker.rs`:

```rust
#[tokio::test]
async fn delete_stops_at_first_non_empty_ancestor() {
    // If `a/` still has a sibling file after we delete `a/b/c.cook`,
    // we must remove `a/b/` (now empty) but leave `a/` alone.
    // remove_dir naturally enforces this via ENOTEMPTY; this test
    // pins that behavior so a future refactor can't accidentally
    // implement recursive deletion.
    let temp = tempfile::TempDir::new().unwrap();
    let cache = InMemoryCache::new(100, 10_000);
    let mut chunker = Chunker::new(cache, temp.path().to_path_buf());

    let nested_dir = temp.path().join("a").join("b");
    tokio::fs::create_dir_all(&nested_dir).await.unwrap();
    tokio::fs::write(nested_dir.join("c.cook"), b"eggs\n")
        .await
        .unwrap();
    tokio::fs::write(temp.path().join("a").join("sibling.cook"), b"flour\n")
        .await
        .unwrap();

    chunker
        .delete("a/b/c.cook")
        .await
        .expect("delete should succeed");

    assert!(
        !temp.path().join("a/b/c.cook").exists(),
        "target file should be removed"
    );
    assert!(
        !temp.path().join("a/b").exists(),
        "empty intermediate directory should be removed"
    );
    assert!(
        temp.path().join("a").exists(),
        "non-empty ancestor must be preserved"
    );
    assert!(
        temp.path().join("a/sibling.cook").exists(),
        "sibling file must be preserved"
    );
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p cooklang-sync-client --lib chunker::tests::delete_stops_at_first_non_empty_ancestor`

Expected: PASS (Task 1 already implemented the behavior).

- [ ] **Step 3: Commit**

```bash
git add client/src/chunker.rs
git commit -m "test(chunker): pin stop-at-non-empty-ancestor behavior"
```

---

## Task 3: Test — storage root is never removed

**Files:**
- Modify (test): `client/src/chunker.rs` (append to `mod tests`)

- [ ] **Step 1: Write the test**

Append inside the `mod tests { ... }` block in `client/src/chunker.rs`:

```rust
#[tokio::test]
async fn delete_does_not_remove_storage_root() {
    // Even when the very last file in the storage root is deleted,
    // the root itself must survive — otherwise the next download
    // cycle has nowhere to write to, and the indexer would crash
    // walking a missing directory.
    let temp = tempfile::TempDir::new().unwrap();
    let cache = InMemoryCache::new(100, 10_000);
    let mut chunker = Chunker::new(cache, temp.path().to_path_buf());

    tokio::fs::write(temp.path().join("only.cook"), b"sugar\n")
        .await
        .unwrap();

    chunker
        .delete("only.cook")
        .await
        .expect("delete should succeed");

    assert!(
        !temp.path().join("only.cook").exists(),
        "file should be removed"
    );
    assert!(
        temp.path().exists(),
        "storage root must never be removed"
    );
    assert!(
        temp.path().is_dir(),
        "storage root must still be a directory"
    );
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p cooklang-sync-client --lib chunker::tests::delete_does_not_remove_storage_root`

Expected: PASS.

- [ ] **Step 3: Commit**

```bash
git add client/src/chunker.rs
git commit -m "test(chunker): pin storage root preservation on delete"
```

---

## Task 4: Test — sibling files in same directory are preserved

**Files:**
- Modify (test): `client/src/chunker.rs` (append to `mod tests`)

- [ ] **Step 1: Write the test**

Append inside the `mod tests { ... }` block in `client/src/chunker.rs`:

```rust
#[tokio::test]
async fn delete_leaves_sibling_files_in_same_directory() {
    // Deleting `a/x.cook` when `a/y.cook` also exists must leave
    // both `a/` and `a/y.cook` intact. This is the "obvious" case
    // but worth pinning — a naive implementation that always
    // removes the immediate parent would break it.
    let temp = tempfile::TempDir::new().unwrap();
    let cache = InMemoryCache::new(100, 10_000);
    let mut chunker = Chunker::new(cache, temp.path().to_path_buf());

    let dir = temp.path().join("a");
    tokio::fs::create_dir_all(&dir).await.unwrap();
    tokio::fs::write(dir.join("x.cook"), b"salt\n").await.unwrap();
    tokio::fs::write(dir.join("y.cook"), b"pepper\n").await.unwrap();

    chunker
        .delete("a/x.cook")
        .await
        .expect("delete should succeed");

    assert!(
        !temp.path().join("a/x.cook").exists(),
        "target file should be removed"
    );
    assert!(
        temp.path().join("a").exists(),
        "directory with remaining files must be preserved"
    );
    assert!(
        temp.path().join("a/y.cook").exists(),
        "sibling file must be preserved"
    );
}
```

- [ ] **Step 2: Run test to verify it passes**

Run: `cargo test -p cooklang-sync-client --lib chunker::tests::delete_leaves_sibling_files_in_same_directory`

Expected: PASS.

- [ ] **Step 3: Run the entire client test suite as a final regression check**

Run: `cargo test -p cooklang-sync-client`

Expected: all tests pass. Pay attention to indexer tests, especially the dot-directory tests added in #20 — those should be unaffected since they don't exercise `Chunker::delete`.

- [ ] **Step 4: Commit**

```bash
git add client/src/chunker.rs
git commit -m "test(chunker): pin sibling preservation on delete"
```

---

## Notes for the implementer

- The four tests are intentionally split across four tasks (one commit each) for clean review. Tasks 2–4 will pass immediately after Task 1's implementation lands; that's expected — they exist to pin behavior, not to drive new code.
- `fs` in `chunker.rs` is `tokio::fs` (already imported at line 5). `fs::remove_dir` returns `io::Result<()>`; we discard the error variant on purpose (see the comment in the implementation).
- `self.base_path` is a `PathBuf` constructed from whatever the caller passed to `Chunker::new`. `full_path()` builds the file path by cloning `base_path` and `push`-ing the relative `path`, so `parent()` walking from `full_path` will pass through exactly equal `PathBuf` values as it climbs — `dir == self.base_path` is a sound stop condition.
- Don't change `syncer.rs`. The call site at `syncer.rs:313-315` is correct as-is.
- Don't try to be clever with `tokio::fs::read_dir` to count entries before calling `remove_dir`. `remove_dir`'s built-in ENOTEMPTY check is atomic with the actual removal; a separate count introduces a race window.
