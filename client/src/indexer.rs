use futures::{
    channel::mpsc::{Receiver, Sender},
    SinkExt, StreamExt,
};
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

use notify_debouncer_mini::DebounceEventResult;
use time::OffsetDateTime;
use tokio::time::Duration;

use log::debug;

use crate::chunker;
use crate::connection::{get_connection, ConnectionPool};
use crate::errors::SyncError;
use crate::models::*;
use crate::registry;

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
    pool: &ConnectionPool,
    storage_path: &Path,
    namespace_id: i32,
    mut local_file_update_rx: Receiver<DebounceEventResult>,
    mut updated_tx: Sender<IndexerUpdateEvent>,
) -> Result<(), SyncError> {
    loop {
        if check_index_once(pool, storage_path, namespace_id)? {
            updated_tx.send(IndexerUpdateEvent::Updated).await?;
        }

        tokio::select! {
            _ = tokio::time::sleep(CHECK_INTERVAL_WAIT_SEC) => {},
            Some(_) = local_file_update_rx.next() => {},
        };
    }
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

fn get_file_records_from_disk(base_path: &Path, namespace_id: i32) -> Result<DiskFiles, SyncError> {
    let mut cache = HashMap::new();

    let iter = WalkDir::new(base_path)
        .into_iter()
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
    let metadata = path.metadata().map_err(|e| SyncError::from_io_error(path, e))?;
    // we assume that it was already checked and only one of these can be now
    let path = path.strip_prefix(base)?.to_string_lossy().into_owned();
    let size: i64 = metadata.len().try_into()?;
    let time = metadata.modified().map_err(|e| SyncError::from_io_error(path.clone(), e))?;
    let modified_at = OffsetDateTime::from(time);

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
