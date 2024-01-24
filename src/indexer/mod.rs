use futures::{
    channel::mpsc::{Receiver, Sender},
    join, StreamExt,
};
use notify::Event;
use std::fs::{self, Metadata};
use std::path::Path;
use time::OffsetDateTime;

use diesel::r2d2::ConnectionManager;
use diesel::r2d2::Pool;
use diesel::sqlite::SqliteConnection;

use crate::local_db::*;
use crate::models::FileRecordCreateForm;

pub async fn run(
    pool: Pool<ConnectionManager<SqliteConnection>>,
    path: &str,
    mut local_file_update_rx: Receiver<Result<Event, notify::Error>>,
    _local_db_record_updated_tx: Sender<()>,
) {
    let duration = tokio::time::Duration::from_secs(60);
    let path = Path::new(path);

    let interval_pool = pool.clone();
    let check_on_interval = async move {
        loop {
            visit_dirs(path, &interval_pool).expect("Directory traversal failed");

            // local_db_record_updated_tx.send(()).await;
            tokio::time::sleep(duration).await;
        }
    };

    let monitor_updates = async move {
        let pool = pool.clone();
        while let Some(res) = local_file_update_rx.next().await {
            match res {
                Ok(event) => {
                    println!("changed: {:?}", event);

                    for p in event.paths {
                        if let Some(ext) = p.extension() {
                            if ext == "cook" {
                                let metadata = p.metadata().unwrap();
                                compare_and_update(path, metadata, &pool);
                            }
                        }
                    }
                }
                Err(e) => println!("watch error: {:?}", e),
            }
        }
    };

    join!(check_on_interval, monitor_updates);
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
                    compare_and_update(&path, metadata, pool);
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
) {
    let now = OffsetDateTime::now_utc();
    let path = path.clone().to_str().expect("oops").to_string();
    let search_path = path.clone();
    let file_record = FileRecordCreateForm {
        path: &path,
        size: Some(metadata.len() as i64),
        format: "t",
        modified_at: Some(OffsetDateTime::from(metadata.modified().unwrap())),
        created_at: now,
    };

    let conn = &mut pool.get().unwrap();

    match latest_file_record(conn, search_path) {
        Some(record_in_db) => {
            if record_in_db != file_record {
                let r = create_file_record(conn, file_record);

                println!("res {:?}", r);
            }
        }
        None => {
            let r = create_file_record(conn, file_record);

            println!("res {:?}", r);
        }
    }
}
