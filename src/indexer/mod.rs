use futures::channel::mpsc::{Receiver, Sender};
use futures::{join, StreamExt};
use std::fs::{self, Metadata};
use std::path::Path;

use notify::Event;
use tokio::time::Duration;
use time::OffsetDateTime;

use diesel::r2d2::{ConnectionManager, Pool};
use diesel::sqlite::SqliteConnection;

use log::{debug, error};

use crate::local_db::*;
use crate::models::{FileRecordCreateForm, FileRecordFilterForm};

const CHECK_INTERVAL_WAIT_SEC: Duration = Duration::from_secs(60);

pub async fn run(
    pool: &Pool<ConnectionManager<SqliteConnection>>,
    path: &Path,
    mut local_file_update_rx: Receiver<Result<Event, notify::Error>>,
    _local_db_record_updated_tx: Sender<()>,
) {
    let check_on_interval = async move {
        loop {
            visit_dirs(path, pool).expect("Directory traversal failed");

            // local_db_record_updated_tx.send(()).await;
            tokio::time::sleep(CHECK_INTERVAL_WAIT_SEC).await;
        }
    };

    let monitor_watcher_updates = async move {
        while let Some(res) = local_file_update_rx.next().await {
            match res {
                Ok(event) => {
                    debug!("changed: {:?}", event);

                    for p in event.paths {
                        if let Some(ext) = p.extension() {
                            if ext == "cook" {
                                let metadata = p.metadata().unwrap();
                                let _ = compare_and_update(path, metadata, pool);
                            }
                        }
                    }
                }
                Err(e) => error!("watch error: {:?}", e),
            }
        }
    };

    join!(check_on_interval, monitor_watcher_updates);
}

fn visit_dirs(dir: &Path, pool: &Pool<ConnectionManager<SqliteConnection>>) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, pool)?;
            } else if let Some(ext) = path.extension() {
                if ext == "cook" {
                    let metadata = entry.metadata()?;
                    let _ = compare_and_update(&path, metadata, pool);
                }
            }
        }
    }

    Ok(())
}

fn compare_and_update(
    path: &Path,
    metadata: Metadata,
    pool: &Pool<ConnectionManager<SqliteConnection>>,
) -> Result<usize, diesel::result::Error> {
    let path = &path.clone().to_str().expect("oops").to_string();
    let file_record = &FileRecordCreateForm {
        path,
        size: Some(metadata.len() as i64),
        format: "t",
        modified_at: Some(OffsetDateTime::from(metadata.modified().unwrap())),
    };

    let conn = &mut pool.get().unwrap();

    let filter_form = &FileRecordFilterForm { path };

    match latest_file_record(conn, filter_form) {
        Some(record_in_db) => {
            if &record_in_db != file_record {
                create_file_record(conn, file_record)
            } else {
                Ok(0)
            }
        }
        None => create_file_record(conn, file_record),
    }
}
