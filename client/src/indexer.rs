use futures::{
    channel::mpsc::{Receiver, Sender},
    join, SinkExt, StreamExt,
};
use std::collections::HashMap;
use std::path::Path;
use walkdir::WalkDir;

use notify::Event;
use time::OffsetDateTime;
use tokio::time::Duration;

use log::{debug};

use crate::registry;
use crate::errors::SyncError;
use crate::connection::{ConnectionPool, get_connection};
use crate::models::*;

type DBFiles = HashMap<String, FileRecord>;
type DiskFiles = HashMap<String, CreateForm>;

const CHECK_INTERVAL_WAIT_SEC: Duration = Duration::from_secs(61);

pub async fn run(
    pool: &ConnectionPool,
    storage_path: &Path,
    mut local_file_update_rx: Receiver<Result<Event, notify::Error>>,
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
                registry::delete(conn, &to_remove.iter().map(|r| r.id).collect())?;
            }

            if !to_add.is_empty() {
                registry::create(conn, &to_add)?;
            }

            updated_tx.send(IndexerUpdateEvent::Updated).await;
        }

        join!(tokio::time::sleep(CHECK_INTERVAL_WAIT_SEC), local_file_update_rx.next());
    }

}


fn filter_eligible(p: &Path) -> bool {
    // TODO properly follow symlinks, they can be broken as well
    if p.is_symlink() {
        return false;
    }

    if let Some(ext) = p.extension() {
        // TODO allow generic
        ext == "cook"
    } else {
        false
    }
}

fn get_file_records_from_disk(base_path: &Path) -> Result<DiskFiles,SyncError> {
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

fn get_file_records_from_registry(pool: &ConnectionPool) -> Result<DBFiles,SyncError> {
    let mut cache = HashMap::new();

    let conn = &mut get_connection(pool)?;

    for record in registry::non_deleted(conn)? {
        cache.insert(record.path.clone(), record);
    }

    Ok(cache)
}

fn compare_records(
    from_db: DBFiles,
    from_fs: DiskFiles,
) -> (Vec<FileRecord>, Vec<CreateForm>) {
    let mut to_remove: Vec<FileRecord> = Vec::new();
    let mut to_add: Vec<CreateForm> = Vec::new();

    for (p, db_file) in &from_db {
        match from_fs.get(p) {
            Some(disk_file) => {
                if db_file != disk_file {
                    to_remove.push(db_file.clone());
                    to_add.push(disk_file.clone());
                }
            }
            None => {
                to_remove.push(db_file.clone());
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


fn build_file_record(path: &Path, base: &Path) -> Result<CreateForm,SyncError> {
    let metadata = path.metadata()?;
    let path = path.strip_prefix(base)?.to_string_lossy().into_owned();
    let size: i64 = metadata.len().try_into()?;
    let time = metadata.modified()?;
    let modified_at = OffsetDateTime::from(time);

    let f = CreateForm {
        jid: None,
        path,
        size,
        format: "t".to_string(),
        modified_at,
    };

    Ok(f)

}
