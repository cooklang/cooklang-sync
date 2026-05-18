use futures::{
    channel::mpsc::{Receiver, Sender},
    SinkExt, StreamExt,
};
use std::collections::HashMap;
use std::path::Path;
use std::sync::Arc;
use path_slash::PathExt as _;
use walkdir::WalkDir;

use notify_debouncer_mini::DebounceEventResult;
use time::OffsetDateTime;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use log::debug;

use crate::chunker;
use crate::connection::{get_connection, ConnectionPool};
use crate::errors::SyncError;
use crate::models::*;
use crate::registry;
use crate::{SyncStatus, SyncStatusListener};

type DBFiles = HashMap<String, FileRecord>;
type DiskFiles = HashMap<String, CreateForm>;

const CHECK_INTERVAL_WAIT_SEC: Duration = Duration::from_secs(61);

/// Indexer main loop. It doesn't manipulate files, but only
/// compares what we have in filesystem with what we have in DB.
/// If it finds a difference it will update DB records.
/// When any change made it will send a message in channel
/// that Syncer is listening.
///
/// It runs both on interval and on any event coming from FS watcher.
pub async fn run(
    token: CancellationToken,
    listener: Option<Arc<dyn SyncStatusListener>>,
    pool: &ConnectionPool,
    storage_path: &Path,
    namespace_id: i32,
    mut local_file_update_rx: Receiver<DebounceEventResult>,
    mut updated_tx: Sender<IndexerUpdateEvent>,
) -> Result<(), SyncError> {
    loop {
        // Check for cancellation at loop start
        if token.is_cancelled() {
            debug!("Indexer received shutdown signal");
            break;
        }

        // Notify that we're starting to index
        if let Some(ref cb) = listener {
            cb.on_status_changed(SyncStatus::Indexing);
        }

        if check_index_once(pool, storage_path, namespace_id)? {
            updated_tx.send(IndexerUpdateEvent::Updated).await?;
        }

        // Return to idle after indexing
        if let Some(ref cb) = listener {
            cb.on_status_changed(SyncStatus::Idle);
        }

        tokio::select! {
            _ = token.cancelled() => {
                debug!("Indexer shutting down");
                break;
            }
            _ = tokio::time::sleep(CHECK_INTERVAL_WAIT_SEC) => {},
            Some(_) = local_file_update_rx.next() => {},
        };
    }

    Ok(())
}

pub fn check_index_once(
    pool: &ConnectionPool,
    storage_path: &Path,
    namespace_id: i32,
) -> Result<bool, SyncError> {
    debug!("interval scan");

    let from_db = get_file_records_from_registry(pool, namespace_id)?;
    let from_fs = get_file_records_from_disk(storage_path, namespace_id)?;

    let (to_remove, to_add) = compare_records(from_db, from_fs, namespace_id);

    if !to_remove.is_empty() || !to_add.is_empty() {
        let conn = &mut get_connection(pool)?;

        if !to_remove.is_empty() {
            registry::delete(conn, &to_remove)?;
        }

        if !to_add.is_empty() {
            registry::create(conn, &to_add)?;
        }

        Ok(true)
    } else {
        Ok(false)
    }
}

fn filter_eligible(p: &Path) -> bool {
    // TODO properly follow symlinks, they can be broken as well
    if p.is_symlink() {
        return false;
    }
    chunker::is_text(p) || chunker::is_binary(p)
}

fn is_dot_dir(e: &walkdir::DirEntry) -> bool {
    // Prune any directory whose name starts with '.', but never prune
    // the root itself (depth == 0). The root exemption lets users
    // configure a hidden storage path like ~/.cooklang without
    // accidentally skipping everything inside it.
    e.depth() > 0
        && e.file_name().to_str().is_some_and(|s| s.starts_with('.'))
}

fn get_file_records_from_disk(base_path: &Path, namespace_id: i32) -> Result<DiskFiles, SyncError> {
    let mut cache = HashMap::new();

    let iter = WalkDir::new(base_path)
        .into_iter()
        .filter_entry(|e| !(e.file_type().is_dir() && is_dot_dir(e)))
        .filter_map(|e| e.ok())
        .map(|p| p.into_path())
        .filter(|p| filter_eligible(p));

    for p in iter {
        let record = build_file_record(&p, base_path, namespace_id)?;

        cache.insert(record.path.clone(), record);
    }

    Ok(cache)
}

fn get_file_records_from_registry(
    pool: &ConnectionPool,
    namespace_id: i32,
) -> Result<DBFiles, SyncError> {
    let mut cache = HashMap::new();

    let conn = &mut get_connection(pool)?;

    for record in registry::non_deleted(conn, namespace_id)? {
        cache.insert(record.path.clone(), record);
    }

    Ok(cache)
}

fn compare_records(
    from_db: DBFiles,
    from_fs: DiskFiles,
    namespace_id: i32,
) -> (Vec<DeleteForm>, Vec<CreateForm>) {
    let mut to_remove: Vec<DeleteForm> = Vec::new();
    let mut to_add: Vec<CreateForm> = Vec::new();

    for (p, db_file) in &from_db {
        match from_fs.get(p) {
            // When file from DB is also present on a disk
            // we need to check if it was changed and if it was
            // remove and add file again.
            Some(disk_file) => {
                if db_file != disk_file {
                    to_add.push(disk_file.clone());
                }
            }
            // When file from DB is not present on a disk
            // we should mark it as deleted in DB
            None => {
                to_remove.push(build_delete_form(db_file, namespace_id));
            }
        }
    }

    for (p, disk_file) in &from_fs {
        if !from_db.contains_key(p) {
            to_add.push(disk_file.clone());
        }
    }

    (to_remove, to_add)
}

fn build_file_record(path: &Path, base: &Path, namespace_id: i32) -> Result<CreateForm, SyncError> {
    let metadata = path
        .metadata()
        .map_err(|e| SyncError::from_io_error(path, e))?;
    // we assume that it was already checked and only one of these can be now
    let path = path.strip_prefix(base)?.to_slash_lossy().into_owned();
    let size: i64 = metadata.len().try_into()?;
    let time = metadata
        .modified()
        .map_err(|e| SyncError::from_io_error(path.clone(), e))?;
    let modified_at = truncate_to_seconds(OffsetDateTime::from(time));

    let f = CreateForm {
        jid: None,
        path,
        deleted: false,
        size,
        modified_at,
        namespace_id,
    };

    Ok(f)
}

/// Truncate an `OffsetDateTime` to whole-second granularity.
///
/// `PartialEq<CreateForm> for FileRecord` compares `modified_at` to detect
/// local changes. If disk mtime carries sub-second precision (APFS nanoseconds,
/// ext4 nanoseconds) but the stored-and-read-back value differs below the
/// second — whether from SQLite round-trip loss or from an external process
/// bumping mtime without changing content — the indexer keeps "detecting" a
/// change every cycle and uploading the same content forever. Normalising to
/// whole seconds on both sides makes the equality stable.
///
/// Real content edits advance mtime by at least a whole second in practice,
/// so this does not lose change detection.
pub(crate) fn truncate_to_seconds(t: OffsetDateTime) -> OffsetDateTime {
    t.replace_nanosecond(0).unwrap_or(t)
}

fn build_delete_form(record: &FileRecord, namespace_id: i32) -> DeleteForm {
    DeleteForm {
        path: record.path.to_string(),
        jid: None,
        deleted: true,
        size: record.size,
        modified_at: record.modified_at,
        namespace_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    use std::fs::{self, File};

    fn dt(nanos: u32) -> OffsetDateTime {
        OffsetDateTime::UNIX_EPOCH
            .replace_nanosecond(nanos)
            .unwrap()
    }

    #[test]
    fn truncate_to_seconds_zeroes_subsecond_component() {
        let truncated = truncate_to_seconds(dt(123_456_789));
        assert_eq!(truncated.nanosecond(), 0);
        assert_eq!(truncated, OffsetDateTime::UNIX_EPOCH);
    }

    #[test]
    fn truncate_to_seconds_is_idempotent_on_whole_seconds() {
        let t = OffsetDateTime::UNIX_EPOCH;
        assert_eq!(truncate_to_seconds(t), t);
    }

    #[test]
    fn truncate_to_seconds_makes_roundtrip_equality_stable() {
        // Simulates: nanosecond-precision mtime read from disk vs a value
        // round-tripped through storage that may lose precision below the
        // second. Without truncation these differ; with truncation they match,
        // so the indexer's equality check is stable.
        let disk = dt(123_456_789);
        let stored_roundtrip = dt(123_000_000);
        assert_ne!(disk, stored_roundtrip);
        assert_eq!(truncate_to_seconds(disk), truncate_to_seconds(stored_roundtrip));
    }

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

        assert_eq!(records.len(), 1, "expected exactly one record; got {:?}", records.keys().collect::<Vec<_>>());
        assert!(records.contains_key("recipes/dinner.cook"), "normal recipe must be indexed; got {:?}", records.keys().collect::<Vec<_>>());
        assert!(!records.contains_key(".git/config.yaml"), "dot-dir file must be excluded; got {:?}", records.keys().collect::<Vec<_>>());
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

        assert_eq!(records.len(), 1, "expected exactly one record; got {:?}", records.keys().collect::<Vec<_>>());
        assert!(records.contains_key(".shopping-list"), "whitelisted root dotfile must be indexed; got {:?}", records.keys().collect::<Vec<_>>());
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

        assert_eq!(records.len(), 1, "expected exactly one record; got {:?}", records.keys().collect::<Vec<_>>());
        assert!(records.contains_key("r.cook"), "file inside hidden storage root must be indexed; got {:?}", records.keys().collect::<Vec<_>>());
    }
}
