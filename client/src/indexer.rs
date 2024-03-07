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
    mut local_file_update_rx: Receiver<DebounceEventResult>,
    mut updated_tx: Sender<IndexerUpdateEvent>,
) -> Result<(), SyncError> {
    loop {
        debug!("interval scan");

        // TODO should be smarter and don't stop the loop,
        // unless repeating errors
        let from_db = get_file_records_from_registry(pool)?;
        let from_fs = get_file_records_from_disk(storage_path)?;

        let (to_remove, to_add) = compare_records(from_db, from_fs);

        if !to_remove.is_empty() || !to_add.is_empty() {
            let conn = &mut get_connection(pool)?;

            if !to_remove.is_empty() {
                registry::delete(conn, &to_remove)?;
            }

            if !to_add.is_empty() {
                registry::create(conn, &to_add)?;
            }

            updated_tx.send(IndexerUpdateEvent::Updated).await?;
        }

        tokio::select! {
            _ = tokio::time::sleep(CHECK_INTERVAL_WAIT_SEC) => {},
            Some(_) = local_file_update_rx.next() => {},
        };
    }
}

fn filter_eligible(p: &Path) -> bool {
    // TODO properly follow symlinks, they can be broken as well
    if p.is_symlink() {
        return false;
    }

    if let Some(ext) = p.extension() {
        // TODO allow generic
        ext == "cook" || ext == "conf" || ext == "yaml" || ext == "yml"
    } else {
        false
    }
}

fn get_file_records_from_disk(base_path: &Path) -> Result<DiskFiles, SyncError> {
    let mut cache = HashMap::new();

    let iter = WalkDir::new(base_path)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|p| p.into_path())
        .filter(|p| filter_eligible(p));

    for p in iter {
        let record = build_file_record(&p, base_path)?;

        cache.insert(record.path.clone(), record);
    }

    Ok(cache)
}

fn get_file_records_from_registry(pool: &ConnectionPool) -> Result<DBFiles, SyncError> {
    let mut cache = HashMap::new();

    let conn = &mut get_connection(pool)?;

    for record in registry::non_deleted(conn)? {
        cache.insert(record.path.clone(), record);
    }

    Ok(cache)
}

fn compare_records(from_db: DBFiles, from_fs: DiskFiles) -> (Vec<DeleteForm>, Vec<CreateForm>) {
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
                to_remove.push(build_delete_form(db_file));
            }
        }
    }

    for (p, disk_file) in &from_fs {
        if from_db.get(p).is_none() {
            to_add.push(disk_file.clone());
        }
    }

    (to_remove, to_add)
}

fn build_file_record(path: &Path, base: &Path) -> Result<CreateForm, SyncError> {
    let metadata = path.metadata()?;
    let path = path.strip_prefix(base)?.to_string_lossy().into_owned();
    let size: i64 = metadata.len().try_into()?;
    let time = metadata.modified()?;
    let modified_at = OffsetDateTime::from(time);

    let f = CreateForm {
        jid: None,
        path,
        deleted: false,
        size,
        format: "t".to_string(),
        modified_at,
    };

    Ok(f)
}

fn build_delete_form(record: &FileRecord) -> DeleteForm {
    DeleteForm {
        path: record.path.to_string(),
        jid: None,
        deleted: true,
        size: record.size,
        format: record.format.to_string(),
        modified_at: record.modified_at,
    }
}
