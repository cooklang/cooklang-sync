pub mod chunker;
pub mod file_watcher;
pub mod indexer;
pub mod local_db;
pub mod models;
pub mod schema;
pub mod syncer;

use notify::{RecursiveMode, Watcher};

use crate::chunker::{Chunker, InMemoryCache};
use crate::file_watcher::async_watcher;
use futures::channel::mpsc::channel;

pub async fn run(storage_dir: &str, db_file_path: &str, _remote_token: &str) -> notify::Result<()> {
    let (mut watcher, local_file_update_rx) = async_watcher()?;
    let (local_base_updated_tx, _local_base_updated_rx) = channel(100);

    let chunk_cache = InMemoryCache::new();
    let _chunker = Chunker::new(chunk_cache);
    let pool = local_db::get_connection_pool(db_file_path);

    // let mut indexer = Indexer::new(db);
    // let mut remote = Remote(token);
    // let mut syncer = Syncer(remote, db, chunker, ready_to_updoad_rx);

    let watch_path = storage_dir.clone();
    let indexer_path = storage_dir;

    watcher.watch(watch_path.as_ref(), RecursiveMode::Recursive)?;

    crate::indexer::run(
        pool,
        indexer_path,
        local_file_update_rx,
        local_base_updated_tx,
    )
    .await;

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
