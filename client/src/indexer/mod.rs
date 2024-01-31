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

use log::{debug, error, trace};

use crate::local_db::*;
use crate::models::*;

type DBFiles = HashMap<String, FileRecord>;
type DiskFiles = HashMap<String, FileRecordCreateForm>;

const CHECK_INTERVAL_WAIT_SEC: Duration = Duration::from_secs(61);

pub async fn run(
    pool: &ConnectionPool,
    storage_path: &Path,
    mut local_file_update_rx: Receiver<Result<Event, notify::Error>>,
    updated_tx: Sender<IndexerUpdateEvent>,
) {
    let check_on_interval = async {
        loop {
            debug!("interval scan");
            let channel = updated_tx.clone();
            if let Err(e) = do_sync(pool, storage_path, channel).await {
                // Handle the error, for example, log it
                error!("Error in do_sync: {}", e);
                break; // or continue, depending on how you want to handle errors
            }

            tokio::time::sleep(CHECK_INTERVAL_WAIT_SEC).await;
        }
    };

    let monitor_watcher_updates = async {
        while let Some(res) = local_file_update_rx.next().await {
            match res {
                Ok(event) => {
                    trace!("fs event triggered {:?}", event);

                    let channel = updated_tx.clone();
                    if let Err(e) = do_sync(pool, storage_path, channel).await {
                        // Handle the error, for example, log it
                        error!("Error in do_sync: {}", e);
                        break; // or continue, depending on how you want to handle errors
                    }
                }
                Err(e) => error!("watch error: {:?}", e),
            }
        }
    };

    join!(check_on_interval, monitor_watcher_updates);
}

async fn do_sync(
    pool: &ConnectionPool,
    storage_path: &Path,
    mut updated_tx: Sender<IndexerUpdateEvent>,
) -> Result<(), Box<dyn std::error::Error>> {
    let db_files = get_file_records_from_registry(pool);
    let disk_files = get_file_records_from_disk(storage_path);

    let (to_remove, to_add) = compare_records(db_files, disk_files);

    if !to_remove.is_empty() || !to_add.is_empty() {
        let conn = &mut pool.get().unwrap();

        if !to_remove.is_empty() {
            delete_file_records(conn, &to_remove.iter().map(|r| r.id).collect())?;
        }

        if !to_add.is_empty() {
            create_file_records(conn, &to_add)?;
        }

        updated_tx.send(IndexerUpdateEvent::Updated).await?;
    }

    Ok(())
}

fn filter_eligible(p: &Path) -> bool {
    // TODO properly follow symlinks, they can be broken as well
    if p.is_symlink() {
        return false;
    }

    if let Some(ext) = p.extension() {
        ext == "cook"
    } else {
        false
    }
}

fn get_file_records_from_disk(p: &Path) -> HashMap<String, FileRecordCreateForm> {
    let mut cache = HashMap::new();

    let iter = WalkDir::new(p)
        .into_iter()
        .filter_map(|e| e.ok())
        .map(|p| p.into_path())
        .filter(|p| filter_eligible(p));

    for p in iter {
        let record = build_file_record(&p);

        cache.insert(p.to_string_lossy().into_owned(), record);
    }

    cache
}

fn get_file_records_from_registry(pool: &ConnectionPool) -> HashMap<String, FileRecord> {
    let mut cache = HashMap::new();

    let conn = &mut pool.get().unwrap();

    for record in non_deleted_file_records(conn) {
        cache.insert(record.path.clone(), record);
    }

    cache
}

fn compare_records(
    db_files: DBFiles,
    disk_files: DiskFiles,
) -> (Vec<FileRecord>, Vec<FileRecordCreateForm>) {
    let mut to_remove: Vec<FileRecord> = Vec::new();
    let mut to_add: Vec<FileRecordCreateForm> = Vec::new();

    for (p, db_file) in &db_files {
        match disk_files.get(p) {
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

    for (p, disk_file) in &disk_files {
        if db_files.get(p).is_none() {
            to_add.push(disk_file.clone());
        }
    }

    (to_remove, to_add)
}


fn build_file_record(path: &Path) -> FileRecordCreateForm {
    let metadata = path.metadata().unwrap();
    let path = path.to_string_lossy().into_owned();
    let size: i64 = metadata.len().try_into().unwrap();
    let modified_at = OffsetDateTime::from(metadata.modified().unwrap());

    FileRecordCreateForm {
        path,
        size,
        format: "t".to_string(),
        modified_at,
    }
}
