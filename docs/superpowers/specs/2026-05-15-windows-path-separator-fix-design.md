# Windows path-separator fix for indexer

**Issue:** [cooklang/cooklang-sync#18](https://github.com/cooklang/cooklang-sync/issues/18) — First sync on Windows uploads tombstones for every downloaded file.

## Problem

On Windows, `cooklang-sync-client` uploads a `deleted: true` tombstone for every file the download loop has just written to disk, then immediately re-uploads the same content. Each downloaded file produces alternating create/delete entries per indexer cycle until the loop stabilises. Any user-created file at a server-known path gets tombstoned within seconds.

### Root cause

The indexer and the downloader build map keys from `Path` values using two different conversions:

- `client/src/indexer.rs:189` (`build_file_record`): `path.strip_prefix(base)?.to_string_lossy().into_owned()` — on Windows this is `plats\pates-carbo.cook` (backslash).
- The downloader (`syncer.rs::check_download_once`) inserts `FileRecord` rows using forward-slash paths from server responses — `plats/pates-carbo.cook`.

`compare_records` keys its `HashMap<String, _>` on these raw strings, so the two never collide:

1. Downloader writes `plats/pates-carbo.cook` and inserts a registry row with the forward-slash path.
2. Indexer's `WalkDir` finds the same file on disk and builds a `CreateForm` keyed `plats\pates-carbo.cook`.
3. `compare_records` sees a DB row with no FS match (→ `DeleteForm`) and an FS entry with no DB match (→ `CreateForm`).
4. `remote.commit` normalises both via `path.to_slash()` (`client/src/remote.rs:259`), so on the wire both refer to the same path. The server records a tombstone followed by a re-create.

On macOS and Linux the native path separator is already `/`, so the mismatch never surfaces. Windows is the first platform where this bug bites in practice — cook-md/editor's first significant Windows release is what triggered the downstream bug report.

## Fix

Normalise the indexer's path key to forward slashes at the same boundary `remote.commit` already does. `path-slash = "0.2.1"` is already a dependency (`client/Cargo.toml:43`).

In `client/src/indexer.rs`:

```rust
use path_slash::PathExt as _;

// in build_file_record, replacing line 189:
let path = path.strip_prefix(base)?.to_slash_lossy().into_owned();
```

### Scope verification — is this the only site?

I grepped `client/src` for `to_string_lossy` and `to_str()`:

| Location | Use | Affected? |
|---|---|---|
| `indexer.rs:189` | Builds a path key for the registry/comparison HashMap | **Yes — fix here** |
| `chunker.rs:235` | `file_name().to_string_lossy()` compared against literals like `.shopping-list` | No — single filename component, no separators |
| `remote.rs:307` | HTTP header `to_str()` | No — not a path |

`build_file_record` is the only path-key construction site that needs to change. Upload (`remote.rs:259`) and download (`syncer.rs::check_download_once`) already produce forward-slash keys.

## Regression tests

### Unit test — `build_file_record` returns forward-slash paths

In `client/src/indexer.rs` `#[cfg(test)] mod tests`:

- Create a temp dir, create a nested file inside it (e.g. `<tmp>/plats/pates-carbo.cook`).
- Build the input path using `Path::new(...).join(...)` so the components use native separators on each platform.
- Call `build_file_record(&path, &base, namespace_id)`.
- Assert the returned `CreateForm.path`:
  - Contains no `\` character.
  - Equals `plats/pates-carbo.cook` exactly.

Uses a real temp-dir file because `build_file_record` calls `path.metadata()`. Refactoring to inject metadata is out of scope for this fix.

### Integration test — no spurious tombstone after download

In `client/tests/` (sibling to the existing `chunk_property_tests.rs`), add a test that:

- Sets up an in-memory or temp-file SQLite connection pool.
- Inserts a `FileRecord` with a forward-slash path matching what `check_download_once` would have inserted from a server response.
- Writes the corresponding file to disk under the storage path.
- Runs `check_index_once` directly (it's `pub`, no network involved).
- Asserts `registry::non_deleted(...)` still contains the file and no soft-deleted row was added.

This test passes on macOS/Linux today (the bug doesn't reproduce there) but would catch any future regression that breaks the indexer/downloader path-key agreement. On Windows it would have failed before this fix.

## CI: multi-platform test matrix

The repo currently has no workflow that runs `cargo test` on push or PR (only `release.yml`, `claude-code-review.yml`, `claude.yml`). That gap is the structural reason this bug slipped through — the integration test in the previous section would have failed on Windows years ago if Windows had ever been in CI.

Add `.github/workflows/test.yml`:

- Triggers: `push` (any branch) and `pull_request` (against `main`).
- Job `test` with matrix `os: [ubuntu-latest, macos-latest, windows-latest]`, `runs-on: ${{ matrix.os }}`.
- Steps: checkout → `dtolnay/rust-toolchain@stable` → cache cargo registry/index/target keyed on `runner.os` and `Cargo.lock` → `cargo test --workspace --all-features`.
- `fail-fast: false` so a failure on one OS doesn't mask failures on the others.

Scope notes:
- Use the workspace's stable Rust — no separate beta/nightly leg unless we discover compatibility needs.
- No coverage / lint jobs in this spec; that's a separate decision.
- The `claude-code-review.yml` workflow is unchanged.

This protects against the entire class of "path-separator or other platform-dependent bug" regressions, not just the one this spec fixes.

## Out of scope

The issue calls out two defense-in-depth follow-ups that would also harden the destructive paths in `check_download_once`:

1. Guard against applying a remote tombstone or overwrite for a path absent from the local registry.
2. On first run with empty registry but non-empty storage, run `check_index_once` synchronously before the download loop.

These address the broader foot-gun ("remote tombstone replay on an unindexed local") rather than the Windows-specific manifestation. They are deferred to a separate spec.

## Affected consumers

- `cooklang-sync-client 0.4.9` — used by `cook-sync` (sync-agent). Not affected in practice (macOS/Linux only).
- `cooklang-sync-client 0.4.11` — used by `cooklang-native` in `cook-md/editor`. Affected on Windows.

Both consumers should bump once the fix lands.

## Severity

Data loss with no warning. Every Windows user who activates sync against a non-empty namespace is affected, plus any file they create afterwards at a server-known path.
