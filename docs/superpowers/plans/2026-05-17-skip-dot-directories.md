# Skip Dot-Directories on Indexer Scan Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Stop the client indexer from picking up any file located inside a directory whose name starts with `.` (e.g., `.git/`, `.vscode/`, `notes/.tmp/`), while preserving the existing dotfile whitelist for `.shopping-list`, `.shopping-checked`, `.bookmarks` at the storage root.

**Architecture:** A single-file change in `client/src/indexer.rs`. Add a small `is_dot_dir` helper and insert `WalkDir::filter_entry(...)` into the iterator chain in `get_file_records_from_disk` so subtrees rooted at dot-directories are pruned during the walk (not just filtered post-walk). The storage root itself is exempted via a `depth() > 0` guard so users whose storage path is hidden (e.g. `~/.cooklang`) still get scanned. Four unit tests in the same file's `#[cfg(test)] mod tests` cover the four behavioural shapes.

**Tech Stack:** Rust 2021, `walkdir` 2.5 (already a dependency), `tempfile` (already a dev-dependency).

---

## File Structure

- **Modify** `client/src/indexer.rs` — add a free `is_dot_dir(&walkdir::DirEntry) -> bool` helper near the existing `filter_eligible`; change `get_file_records_from_disk` to insert `.filter_entry(...)` into the `WalkDir` chain; append four tests to `#[cfg(test)] mod tests`.

No other files require changes. The download path, server, chunker, and configuration code are all untouched. See `docs/superpowers/specs/2026-05-17-skip-dot-directories-design.md` for scope rationale.

---

## Task 1: Add failing tests for dot-directory filtering

**Files:**
- Modify: `client/src/indexer.rs` (extend `#[cfg(test)] mod tests` at the bottom of the file)

These four tests pin the four behaviours from the spec: (a) prune a top-level dot-dir, (b) prune a nested dot-dir, (c) keep a whitelisted dotfile at the root, (d) never prune the root itself even when the root path is hidden.

The existing `mod tests` block already imports `use super::*;`, `use tempfile::TempDir;`, and `use std::fs::{self, File};` (see `client/src/indexer.rs:237-240`). The new tests reuse those imports — no new imports needed for Task 1.

- [ ] **Step 1: Append the four new tests at the bottom of `mod tests`**

Add the following four tests at the end of `mod tests` in `client/src/indexer.rs`, immediately before its closing `}`. Do not reorder existing tests.

```rust
    #[test]
    fn get_file_records_from_disk_skips_files_inside_dot_directory() {
        // Issue #20: files inside a top-level dot-directory must not be
        // indexed. The dot-dir convention covers VCS metadata (.git),
        // editor state (.vscode), OS caches (.Trash) etc — none of which
        // belong in a recipe sync.
        let tmp = TempDir::new().expect("create tempdir");
        let base = tmp.path();

        // A normal recipe — should be indexed.
        fs::create_dir_all(base.join("recipes")).expect("mkdir recipes");
        File::create(base.join("recipes/dinner.cook")).expect("create cook");

        // A hidden VCS metadata directory containing a file that would
        // otherwise pass `filter_eligible` (note the `.yaml` extension —
        // without it, the test would pass trivially because filter_eligible
        // already rejects extension-less files like `.git/HEAD`).
        fs::create_dir_all(base.join(".git")).expect("mkdir .git");
        File::create(base.join(".git/config.yaml")).expect("create config");

        let records = get_file_records_from_disk(base, 1).expect("walk");
        let keys: Vec<&String> = records.keys().collect();

        assert_eq!(
            keys,
            vec![&"recipes/dinner.cook".to_string()],
            "only the recipe should be indexed; got {:?}",
            keys
        );
    }

    #[test]
    fn get_file_records_from_disk_skips_nested_dot_directory() {
        // The pruning must apply at any depth, not just under the root.
        // A `.cache/` inside an otherwise normal subfolder should still
        // be skipped.
        let tmp = TempDir::new().expect("create tempdir");
        let base = tmp.path();

        fs::create_dir_all(base.join("recipes/.cache")).expect("mkdir nested");
        File::create(base.join("recipes/.cache/x.cook")).expect("create nested file");

        let records = get_file_records_from_disk(base, 1).expect("walk");
        assert!(
            records.is_empty(),
            "nested dot-dir contents must be skipped; got {:?}",
            records.keys().collect::<Vec<_>>()
        );
    }

    #[test]
    fn get_file_records_from_disk_keeps_dotfile_in_root() {
        // Files (not directories) with a leading dot must still flow
        // through. The chunker's is_text whitelist already allows
        // `.shopping-list`, `.shopping-checked`, `.bookmarks` — we must
        // not break that contract.
        let tmp = TempDir::new().expect("create tempdir");
        let base = tmp.path();

        File::create(base.join(".shopping-list")).expect("create dotfile");

        let records = get_file_records_from_disk(base, 1).expect("walk");
        let keys: Vec<&String> = records.keys().collect();
        assert_eq!(
            keys,
            vec![&".shopping-list".to_string()],
            "whitelisted root dotfile must be indexed; got {:?}",
            keys
        );
    }

    #[test]
    fn get_file_records_from_disk_keeps_files_when_storage_root_is_hidden() {
        // If the user configures their storage_dir to be a hidden path
        // (e.g. ~/.cooklang), we must not prune the root itself —
        // otherwise nothing would ever sync. The depth() > 0 guard in
        // is_dot_dir enforces this; this test pins it.
        let tmp = TempDir::new().expect("create tempdir");
        let hidden_root = tmp.path().join(".cooklang");
        fs::create_dir_all(&hidden_root).expect("mkdir hidden root");
        File::create(hidden_root.join("r.cook")).expect("create cook in hidden root");

        let records = get_file_records_from_disk(&hidden_root, 1).expect("walk");
        let keys: Vec<&String> = records.keys().collect();
        assert_eq!(
            keys,
            vec![&"r.cook".to_string()],
            "files inside a hidden storage root must still be indexed; got {:?}",
            keys
        );
    }
```

- [ ] **Step 2: Run the new tests and verify the expected red/green split**

```bash
cd client && cargo test --lib indexer::tests::get_file_records_from_disk_ -- --nocapture
```

Expected (4 cases run):
- `_skips_files_inside_dot_directory` **FAILS** — current code indexes `.git/config.yaml` because `filter_eligible` accepts `.yaml`.
- `_skips_nested_dot_directory` **FAILS** — current code indexes `recipes/.cache/x.cook`.
- `_keeps_dotfile_in_root` **PASSES** today (existing behaviour we must preserve — acts as regression guard).
- `_keeps_files_when_storage_root_is_hidden` **PASSES** today (root is walked normally — acts as regression guard for the `depth() > 0` invariant introduced in Task 2).

Two failing tests is the "red" Task 2 will turn green. The two passing tests guard behaviour we must not break.

- [ ] **Step 3: Commit the failing tests**

```bash
git add client/src/indexer.rs
git commit -m "test(indexer): pin dot-directory skip behaviour (#20)"
```

Committing the failing tests separately makes the bisect story clean: the next commit is the fix, and `git bisect` will land on it cleanly.

---

## Task 2: Implement the dot-directory pruning

**Files:**
- Modify: `client/src/indexer.rs` (add `is_dot_dir` helper; extend `get_file_records_from_disk`)

- [ ] **Step 1: Add the `is_dot_dir` helper near `filter_eligible`**

In `client/src/indexer.rs`, just below the existing `filter_eligible` function (currently `client/src/indexer.rs:109-115`), add:

```rust
fn is_dot_dir(e: &walkdir::DirEntry) -> bool {
    // Prune any directory whose name starts with '.', but never prune
    // the root itself (depth == 0). The root exemption lets users
    // configure a hidden storage path like ~/.cooklang without
    // accidentally skipping everything inside it.
    e.depth() > 0
        && e.file_name().to_str().is_some_and(|s| s.starts_with('.'))
}
```

The function takes `&walkdir::DirEntry` (the path-qualified type — no extra `use` needed; `walkdir` is already imported). `is_some_and` is in stable Rust since 1.70, which is well below the edition-2021 toolchain this project uses.

- [ ] **Step 2: Insert `filter_entry` into the iterator chain**

In `client/src/indexer.rs`, locate `get_file_records_from_disk` (currently `client/src/indexer.rs:117-133`). Find the iterator construction:

```rust
    let iter = WalkDir::new(base_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|p| p.into_path())
        .filter(|p| filter_eligible(p));
```

Replace it with:

```rust
    let iter = WalkDir::new(base_path)
        .into_iter()
        .filter_entry(|e| !(e.file_type().is_dir() && is_dot_dir(e)))
        .filter_map(|e| e.ok())
        .map(|p| p.into_path())
        .filter(|p| filter_eligible(p));
```

`filter_entry` is the operation that actually prunes subtrees — `walkdir` won't recurse into any directory for which the predicate returns `false`. The predicate keeps everything that isn't a dot-directory; files with a leading dot still flow through and are evaluated downstream by `filter_eligible`.

- [ ] **Step 3: Run the four new tests and verify they pass**

```bash
cd client && cargo test --lib indexer::tests::get_file_records_from_disk_ -- --nocapture
```

Expected: all 4 pass.

- [ ] **Step 4: Run the full `indexer::tests` module to confirm no regressions**

```bash
cd client && cargo test --lib indexer::tests
```

Expected: every test in the module passes (the three `truncate_to_seconds_*` tests, `build_file_record_normalises_path_separators_to_forward_slash`, and the four new dot-dir tests).

- [ ] **Step 5: Run the full client test suite**

```bash
cd client && cargo test
```

Expected: no failures across unit and integration tests.

- [ ] **Step 6: Commit**

```bash
git add client/src/indexer.rs
git commit -m "fix(indexer): skip directories starting with '.' (#20)"
```

---

## Task 3: Verify no other indexer entry points bypass the filter

**Files:** none modified — this is a verification step recorded explicitly so the executor doesn't skip it.

The spec's "upload-side only" scope assumes `get_file_records_from_disk` is the **only** function that turns disk state into registry rows. Confirm this is still true.

- [ ] **Step 1: Grep for other `WalkDir` usages in the client crate**

```bash
cd client && grep -rn "WalkDir" src/
```

Expected output (exactly one match):

```
src/indexer.rs:120:    let iter = WalkDir::new(base_path)
```

If any other `WalkDir::new` appears, evaluate whether it also feeds registry rows; if so, add a follow-up task to apply the same `filter_entry`. As of this writing, the only walker is the indexer's.

- [ ] **Step 2: Confirm `filter_eligible` is still the only post-walk filter**

```bash
cd client && grep -n "filter_eligible\|filter_entry" src/indexer.rs
```

Expected: two matches inside `get_file_records_from_disk` (one `.filter_entry`, one `.filter(|p| filter_eligible(p))`), plus the function definition of `filter_eligible`. No other callers.

- [ ] **Step 3: No commit needed for this verification step.**

---

## Task 4: PR preparation

**Files:** none modified.

- [ ] **Step 1: Re-run the workspace test suite**

```bash
cargo test --workspace
```

Expected: all tests pass.

- [ ] **Step 2: Push the branch and open the PR**

Push, then open a PR with title `fix(indexer): skip directories starting with '.' (#20)` and a body that:

- Links to issue #20.
- Summarises the change (one `filter_entry` predicate in `get_file_records_from_disk`, four unit tests).
- Notes the scope decisions captured in the spec: upload-side only; root-path exemption via `depth() > 0`; existing dotfile whitelist at root preserved.
- Mentions the explicit out-of-scope items (download-side filtering, `.syncignore` configurability) so reviewers don't ask.
