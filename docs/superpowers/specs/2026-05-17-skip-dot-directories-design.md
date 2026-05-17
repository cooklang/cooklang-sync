# Skip dot-directories on indexer scan

GitHub issue: [#20 — don't sync directories starting with '.'](https://github.com/cooklang/cooklang-sync/issues/20)

## Goal

The local indexer should ignore any file located inside a directory whose name starts with `.` (e.g., `.git/`, `.vscode/`, `notes/.tmp/`).

Files starting with `.` directly under the storage root remain eligible, preserving the existing whitelist for `.shopping-list`, `.shopping-checked`, and `.bookmarks` handled by `chunker::is_text`.

## Scope

Upload side only — `client/src/indexer.rs`. No changes to the download path, server, or chunker.

Pre-existing server data inside dot-directories will still be downloaded into the local filesystem, but the indexer will then ignore those files (they won't be re-uploaded as tombstones — `filter_eligible` already gates that). Symmetric download-side filtering is explicitly out of scope.

## Implementation

In `get_file_records_from_disk` (currently `client/src/indexer.rs:117`), introduce a `WalkDir::filter_entry` step that prunes any **directory** entry whose file name starts with `.`. The check runs only on directory entries — files with a leading dot at any depth still flow through `filter_entry` and are evaluated as before by `filter_eligible` (which delegates to `chunker::is_text` / `is_binary`).

```rust
let iter = WalkDir::new(base_path)
    .into_iter()
    .filter_entry(|e| {
        // Prune dot-directories anywhere in the tree.
        // Files (including ones with leading dot) still flow through.
        !(e.file_type().is_dir() && is_dot_dir(e))
    })
    .filter_map(|e| e.ok())
    .map(|p| p.into_path())
    .filter(|p| filter_eligible(p));
```

Helper:

```rust
fn is_dot_dir(e: &walkdir::DirEntry) -> bool {
    e.depth() > 0
        && e.file_name().to_str().is_some_and(|s| s.starts_with('.'))
}
```

The `depth() > 0` guard means that if the user configures their storage root to be itself hidden (e.g., `~/.cooklang`), we don't accidentally skip everything inside it. `WalkDir` always visits the root with `depth() == 0`.

## Tests

New tests added to the `#[cfg(test)]` module in `client/src/indexer.rs`, alongside the existing `build_file_record_normalises_path_separators_to_forward_slash`:

1. **`get_file_records_from_disk_skips_files_inside_dot_directory`** — create `recipes/dinner.cook` and `.git/HEAD`, assert only the cook file appears in the returned map.
2. **`get_file_records_from_disk_skips_nested_dot_directory`** — create `recipes/.cache/x.cook`, assert it's not picked up.
3. **`get_file_records_from_disk_keeps_dotfile_in_root`** — create `.shopping-list` at the root, assert it's picked up (regression guard for the existing dotfile whitelist).
4. **`get_file_records_from_disk_keeps_files_when_storage_root_is_hidden`** — inside a `TempDir`, create a `.cooklang/r.cook` subtree, then call `get_file_records_from_disk` with `base_path` pointing at `.cooklang`. Assert `r.cook` is picked up (guards the `depth() > 0` invariant: the root itself, even if hidden, must not be pruned).

## Out of scope

- Download-side filtering. Pre-existing dot-dir content on the server remains downloadable; we just stop adding more.
- Configurable ignore patterns (e.g. `.syncignore`). YAGNI — not in the issue.
- Special handling for symlinks pointing into dot-dirs. The existing symlink filter in `filter_eligible` is unchanged.

## Decisions log

These were resolved during brainstorming (2026-05-17):

- **Filter semantics.** Skip any path whose ancestor directory starts with `.`; files with leading dot at the root still sync. Chosen over also rejecting dotfiles at root, which would break `.shopping-list` and friends.
- **Direction.** Upload-side only. Chosen over symmetric upload+download filtering to keep the change minimal and avoid invalidating data already on the server.
- **Mechanism.** `WalkDir::filter_entry` (subtree pruning) rather than per-entry `filter_eligible` check. Chosen because a `.git/` directory inside a recipe folder can be tens of thousands of files; pruning saves real IO on every index pass.
