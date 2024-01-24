
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use futures::{
    channel::mpsc::{channel, Sender, Receiver},
    SinkExt, StreamExt,
    join,
};
use notify_debouncer_full::{DebounceEventResult};

use crate::local_db::LocalDB;

use tokio::time;

use std::collections::HashMap;
use std::fs::{self, Metadata};
use std::path::{Path, PathBuf};
use std::time::SystemTime;

#[derive(Debug, PartialEq, Eq)]
struct FileInfo {
    size: u64,
    modified_date: SystemTime,
}


enum FileCheckResult {
    Matched,
    NotMatched
}

pub async fn run(path: String,
                 mut local_file_update_rx: Receiver<Result<Event, notify::Error>>,
                 mut local_db_record_updated_tx: Sender<()>) {

    let duration = time::Duration::from_secs(5);
    let mut file_info_map: HashMap<PathBuf, FileInfo> = HashMap::new();

    let check_on_interval = async {
        loop {
            visit_dirs(Path::new(&path), &mut file_info_map).expect("Directory traversal failed");

            local_db_record_updated_tx.send(()).await;
            time::sleep(duration).await;
        }
    };

    let monitor_updates = async {
        while let Some(res) = local_file_update_rx.next().await {
            match res {
                Ok(event) => println!("changed: {:?}", event),
                Err(e) => println!("watch error: {:?}", e),
            }
        }
    };

    join!(check_on_interval, monitor_updates);
}


fn visit_dirs(dir: &Path, file_info_map: &mut HashMap<PathBuf, FileInfo>) -> std::io::Result<()> {
    println!("chedking path {}", dir.display());
    if dir.is_dir() {
        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                visit_dirs(&path, file_info_map)?;
            } else if let Some(ext) = path.extension() {
                if ext == "cook" {
                    let metadata = entry.metadata()?;
                    compare_and_update(&path, metadata, file_info_map);
                }
            }
        }
    }
    Ok(())
}

fn compare_and_update(path: &PathBuf, metadata: Metadata, file_info_map: &mut HashMap<PathBuf, FileInfo>) {
    let file_info = FileInfo {
        size: metadata.len(),
        modified_date: metadata.modified().unwrap_or(SystemTime::now()),
    };

    match file_info_map.get(path) {
        Some(existing_info) => {
            if existing_info != &file_info {
                println!("File changed: {:?}", path);
                // Update the hash map
                file_info_map.insert(path.clone(), file_info);
            }
        }
        None => {
            // File not in map, add it
            file_info_map.insert(path.clone(), file_info);
        }
    }
}

