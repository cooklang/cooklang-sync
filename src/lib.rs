pub mod chunker;
pub mod indexer;
pub mod syncer;
pub mod local_db;
pub mod file_watcher;
pub mod models;
pub mod schema;


use notify::{Config, Event, RecommendedWatcher, RecursiveMode, Watcher};
use std::path::Path;
use futures::{
    channel::mpsc::{channel, Receiver},
    SinkExt, StreamExt,
};
use crate::file_watcher::async_watcher;
use crate::chunker::{Chunker, InMemoryCache};
use crate::local_db::{LocalDB};



pub async fn run(storage_dir: String, db_file_path: String, remote_token: String) -> notify::Result<()> {
    let (mut watcher, mut rx) = async_watcher()?;

    let mut chunk_cache = InMemoryCache::new();
    let mut chunker = Chunker::new(chunk_cache);
    let mut db = LocalDB::new(db_file_path);

    // let mut indexer = Indexer::new(db);
    // let mut remote = Remote(token);
    // let mut syncer = Syncer(remote, db, chunker, ready_to_updoad_rx);

    let watch_path = storage_dir.clone();
    let indexer_path = storage_dir;

    watcher.watch(watch_path.as_ref(), RecursiveMode::Recursive)?;

    crate::indexer::run(indexer_path, rx).await;


    // let indexer_thread = std::thread::spawn(move ||{
    //     indexer.run(indexer_path, rx);
    // });

    // let syncer_upload_thread = std::thread::spawn({
    //     syncer.run_upload();
    // });

    // let syncer_download_thread = std::thread::spawn({
    //     syncer.run_download();
    // });

    // watcher_thread.join().unwrap();
    println!("hehe");
    // indexer_thread.join().unwrap();

    // should return a callback to notify about rebuilding
    // can fail if authorization didn't work
    //

    Ok(())
}

