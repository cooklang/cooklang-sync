use futures::{
    channel::mpsc::{Receiver, Sender},
    join, StreamExt,
};
use notify::Event;
use std::fs::{self, Metadata};
use std::path::Path;
use time::OffsetDateTime;

use crate::local_db::LocalDB;
use crate::models::FileRecordCreateForm;

pub async fn run(
    db: &mut LocalDB,
    path: String,
    mut local_file_update_rx: Receiver<Result<Event, notify::Error>>,
    _local_db_record_updated_tx: Sender<()>,
) {
    let duration = tokio::time::Duration::from_secs(60);
    let path = Path::new(&path);

    let check_on_interval = async {
        loop {
            visit_dirs(path, db).expect("Directory traversal failed");

            // local_db_record_updated_tx.send(()).await;
            tokio::time::sleep(duration).await;
        }
    };

    let monitor_updates = async {
        while let Some(res) = local_file_update_rx.next().await {
            match res {
                Ok(event) => {
                    println!("changed: {:?}", event);

                    // for p in event.paths {
                    //     if let Some(ext) = p.extension() {
                    //         if ext == "cook" {
                    //             let metadata = p.metadata().unwrap();
                    //             compare_and_update(path, metadata, db);
                    //         }
                    //     }

                    // }
                }
                Err(e) => println!("watch error: {:?}", e),
            }
        }
    };

    join!(check_on_interval, monitor_updates);
}

fn visit_dirs(dir: &Path, db: &mut LocalDB) -> std::io::Result<()> {
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, db)?;
            } else if let Some(ext) = path.extension() {
                if ext == "cook" {
                    let metadata = entry.metadata()?;
                    compare_and_update(&path, metadata, db);
                }
            }
        }
    }
    Ok(())
}

fn compare_and_update(path: &Path, metadata: Metadata, db: &mut LocalDB) {
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

    match db.latest_file_record(search_path) {
        Some(record_in_db) => {
            if record_in_db != file_record {
                let r = db.create_file_record(file_record);

                println!("res {:?}", r);
            }
        }
        None => {
            let r = db.create_file_record(file_record);

            println!("res {:?}", r);
        }
    }
}
