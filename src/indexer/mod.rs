use std::path::{Path, PathBuf};
use std::collections::HashMap;
use futures::channel::mpsc::{Receiver, Sender};
use futures::{join, StreamExt};
use walkdir::WalkDir;

use notify::Event;
use notify::event::{EventKind};
use time::OffsetDateTime;
use tokio::time::Duration;

use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;

use log::{debug, error, trace};

use crate::local_db::*;
use crate::models::{FileRecord, FileRecordCreateForm, FileRecordFilterForm};

const CHECK_INTERVAL_WAIT_SEC: Duration = Duration::from_secs(60);

pub async fn run(
    pool: &Pool<ConnectionManager<SqliteConnection>>,
    path: &PathBuf,
    mut local_file_update_rx: Receiver<Result<Event, notify::Error>>,
    _local_db_record_updated_tx: Sender<()>,
) {
    let check_file = {
        |p: &PathBuf| {
            if filter_eligible(p) {
                // let _ = compare_and_update(p, pool);
            }
        }
    };

    let check_on_interval = async move {
        loop {
            let db_files = get_file_records_from_registry(pool);
            let disk_files = get_file_records_from_disk(path);

            let (to_update, to_remove, to_add) = compare_records(db_files, disk_files);

            trace!("to_update: {:?}", to_update);
            trace!("to_remove: {:?}", to_remove);
            trace!("to_add: {:?}", to_add);

            // local_db_record_updated_tx.send(()).await;
            tokio::time::sleep(CHECK_INTERVAL_WAIT_SEC).await;
        }
    };

    let monitor_watcher_updates = async move {
        while let Some(res) = local_file_update_rx.next().await {
            match res {
                Ok(event) => {
                    trace!("changed: {:?}", event);

                    match event.kind {
                        EventKind::Create(_) =>  trace!("event kind Create"),
                        EventKind::Access(_) =>  trace!("event kind Access"),
                        EventKind::Modify(_) =>  trace!("event kind Modify"),
                        EventKind::Remove(_) =>  trace!("event kind Remove"),
                        EventKind::Any =>  trace!("event kind Any"),
                        EventKind::Other =>  trace!("event kind Other"),
                    }
                }
                Err(e) => error!("watch error: {:?}", e),
            }
        }
    };

    join!(check_on_interval, monitor_watcher_updates);
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

    let iter = WalkDir::new(p).into_iter()
        .filter_map(|e| e.ok())
        .map(|p| p.into_path())
        .filter(|p| filter_eligible(p));

    for p in iter {
        let record = build_file_record(&p);

        cache.insert(p.to_string_lossy().into_owned(), record);
    }

    cache
}

fn get_file_records_from_registry(pool: &Pool<ConnectionManager<SqliteConnection>>) -> HashMap<String, FileRecord> {
    let mut cache = HashMap::new();

    let conn = &mut pool.get().unwrap();
    // TODO
    // let filter_form = &build_filter_form(path);

    for record in latest_file_records(conn) {
        cache.insert(record.path.clone(), record);
    }

    cache
}

fn compare_records(db_files: HashMap<String, FileRecord>, disk_files: HashMap<String, FileRecordCreateForm>) -> (Vec<FileRecordCreateForm>, Vec<FileRecord>, Vec<FileRecordCreateForm>) {
    let mut to_update: Vec<FileRecordCreateForm> = Vec::new();
    let mut to_remove: Vec<FileRecord> = Vec::new();
    let mut to_add: Vec<FileRecordCreateForm> = Vec::new();

    for (p, db_file) in &db_files {
        match disk_files.get(p) {
            Some(disk_file) => {
                if db_file != disk_file {
                   to_update.push(disk_file.clone());
                }
            },
            None => {
                to_remove.push(db_file.clone())
            },
        }
    }

    for (p, disk_file) in &disk_files {
        match db_files.get(p) {
            None => {
                to_add.push(disk_file.clone());
            },
            _ => {},
        }
    }

    (to_update, to_remove, to_add)
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
