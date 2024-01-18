pub mod chunker;
pub mod indexer;
pub mod syncer;
pub mod local_db;
pub mod file_watcher;
pub mod models;
pub mod schema;


use std::sync::mpsc::{channel, Sender, Receiver};
use notify_debouncer_full::{DebounceEventResult};
use crate::file_watcher::FileWatcher;
use crate::chunker::{Chunker, InMemoryCache};
use crate::local_db::{LocalDB};


fn run(storage_dir: String, db_file_path: String, remote_token: String) {
    let (local_updated_tx, local_updated_rx): (Sender<DebounceEventResult>, Receiver<DebounceEventResult>) = channel();
    // let (ready_to_updoad_tx, ready_to_updoad_rx) = channel();

    let mut chunk_cache = InMemoryCache::new();
    let mut chunker = Chunker::new(chunk_cache);
    let mut watcher = FileWatcher {};
    let mut db = LocalDB::new(db_file_path);

    // let mut indexer = Indexer(db, chunker, ready_to_updoad_tx, local_updated_rx, storage_dir);
    // let mut remote = Remote(token);
    // let mut syncer = Syncer(remote, db, chunker, ready_to_updoad_rx);

    let watcher_thread = std::thread::spawn(move || {
        watcher.run(storage_dir, local_updated_tx);
    });

    // let indexer_thread = std::thread::spawn({
    //     indexer.run();
    // });

    // let syncer_upload_thread = std::thread::spawn({
    //     syncer.run_upload();
    // });

    // let syncer_download_thread = std::thread::spawn({
    //     syncer.run_download();
    // });

}

