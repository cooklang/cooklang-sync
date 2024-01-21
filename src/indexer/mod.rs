
use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt, StreamExt,
};
use notify_debouncer_full::{DebounceEventResult};

use crate::local_db::LocalDB;


enum FileCheckResult {
    Matched,
    NotMatched
}

pub async fn run(path: String, mut local_updated_rx: Receiver<Result<Event, notify::Error>>) {
    check_all_files();

    // on timer should check if any files


    // on rx that file updated:
    // check file
    // callback(result)
    // emit ready to be synced ready_to_updoad_tx

   while let Some(res) = local_updated_rx.next().await {
        match res {
            Ok(event) => println!("changed: {:?}", event),
            Err(e) => println!("watch error: {:?}", e),
        }
    }

}

// should be done on app start and on regular intervals after
fn check_all_files() {
    // all_files().each {
    //     let result = check_file(path);
    //     callback(result)
    // }
}

fn callback(result: FileCheckResult) {
    // match result {
    //     Matched => nothing to do,
    //     NotMatched => store in db without jid, tell that local_file_updated
    // }
}


fn check_file(path: String) {
    // query from db
    // file stored
    // compare_with_db

    // returns FileCheckResult
}

// fn file_stored(path) -> FileOnDisk {
//     // get file metadata
// }

// fn compare_with_db(file_stored, db_record) {
//     // compare metadata, size
// }
