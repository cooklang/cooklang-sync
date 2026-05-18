# Empty Folder Cleanup on Download

GitHub issue: [#21 — folder isn't deleted from device](https://github.com/cooklang/cooklang-sync/issues/21).

## Problem

When the user deletes a file on one device, the sync server propagates a delete
event. On every other device, the syncer removes the file but leaves the
parent directory in place. The result: empty folders accumulate on receiving
devices and surface in the UI as ghost folders the user cannot easily clean up
themselves (mobile file managers often hide empty-folder removal behind
multiple taps).

The bug lives in `Chunker::delete` (`client/src/chunker.rs:147`) which already
carries a `// TODO delete folders up too if empty` marker. The function is
invoked by the download loop in `Syncer::check_download_once`
(`client/src/syncer.rs:309-315`) when a remote delete record arrives.

## Goals

- After a synced delete, remove any parent directories that become empty as a
  result, up to (but never including) the storage root.
- Keep the change scoped to the function flagged by the existing TODO.
- Do not regress: a sibling file in the same directory must keep its parent.

## Non-goals

- Cleaning up empty folders that became empty by some other path (e.g. user
  manually deleted files outside the sync flow). The indexer is responsible
  for tracking the user's view of the directory tree; this fix is targeted at
  the sync-driven case described in the issue.
- Treating OS-generated files (`.DS_Store`, `Thumbs.db`, `.localized`) as
  "doesn't count". We use strict-empty semantics: a directory containing any
  entry is left alone. On the actual sync targets (iOS, Android), those
  files are not auto-created, so the strict rule is the right default. If
  desktop users report leftover folders caused by `.DS_Store`, revisit.
- Bulk cleanup on upload, indexer, or startup. Out of scope.

## Design

### Behavior

Modify `Chunker::delete(&mut self, path: &str)`:

1. Remove the file as today (`fs::remove_file`). Propagate the existing error
   as `SyncError::from_io_error`.
2. Walk parent directories upward starting from `full_path.parent()`:
   - If the candidate directory equals `self.base_path`, stop. The storage
     root is never removed.
   - Otherwise, attempt `fs::remove_dir(candidate)`. `remove_dir` only
     succeeds on empty directories, which is exactly our strict-empty rule.
   - On success: continue with `candidate.parent()`.
   - On any error: stop walking. Do not propagate. The dominant cause is
     `ENOTEMPTY` (the directory has remaining siblings) which is the normal
     terminating condition, not a failure. Other errors (permissions, a
     concurrent process re-creating the directory, etc.) are not worth
     failing a download cycle over; the parent will be retried on the next
     delete that lands in that subtree.

### Sketch

```rust
pub async fn delete(&mut self, path: &str) -> Result<()> {
    trace!("deleting {:?}", path);
    let full_path = self.full_path(path);

    fs::remove_file(&full_path)
        .await
        .map_err(|e| SyncError::from_io_error(path, e))?;

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

The TODO comment on the prior body is removed in the same edit.

### Why this approach

Three options were considered:

1. **Walk up inside `Chunker::delete`.** (chosen) Minimal, self-contained,
   matches the existing TODO. Each delete pays one `remove_dir` per ancestor
   level, which is one cheap syscall — ENOTEMPTY is fast.
2. **Post-pass in `check_download_once`.** Track directories touched during
   the deletes loop, then sweep them once after the loop. Slightly less
   syscalls when many files in the same folder are deleted in a single
   batch, but adds plumbing (set of touched paths, second loop) for a
   negligible saving.
3. **Full tree sweep at end of each download cycle.** Highest I/O cost, most
   thorough. Overkill for a targeted fix.

Option 1 wins on simplicity-per-risk and stays inside the unit the bug
report names.

## Error handling

| Failure mode                                          | Behavior                                       |
|-------------------------------------------------------|------------------------------------------------|
| `remove_file` fails                                   | Propagate as today (`SyncError::from_io_error`)|
| `remove_dir` fails (ENOTEMPTY, permission, missing)   | Stop walking; return `Ok(())`                  |
| Parent chain reaches `base_path`                      | Stop; never remove the storage root            |
| `parent()` returns `None`                             | Stop (loop terminates)                         |

The function still returns `Ok(())` whenever the file itself was removed,
regardless of cleanup outcome. This preserves the contract callers in
`syncer.rs` rely on — a delete event is "handled" once the file is gone.

## Tests

Add four unit tests to the existing `chunker.rs` test module. Each uses a
`TempDir` as the storage base, mirroring the style of
`delete_removes_file_from_storage_dir`.

1. `delete_removes_empty_parent_directories` — write `a/b/c.cook` under a
   fresh storage dir, call `delete("a/b/c.cook")`, assert both `a/b/` and
   `a/` are gone.
2. `delete_stops_at_first_non_empty_ancestor` — write `a/b/c.cook` and
   `a/sibling.cook`, call `delete("a/b/c.cook")`, assert `a/b/` is gone
   but `a/` and `a/sibling.cook` survive.
3. `delete_does_not_remove_storage_root` — write a single file at the
   storage root, call `delete` on it, assert the storage root itself
   still exists.
4. `delete_leaves_sibling_files_in_same_directory` — write `a/x.cook` and
   `a/y.cook`, call `delete("a/x.cook")`, assert `a/` and `a/y.cook` are
   still present.

No integration test is added; the change is purely local filesystem
behavior and the existing chunker tests cover the wiring.

## Out of scope follow-ups

- Indexer-side detection of folders that became empty through means other
  than the sync flow. If user reports come in, the right place is a
  separate indexer pass, not this function.
- Treating known OS-generated hidden files as "doesn't count" for
  emptiness checks. Revisit if reported.
