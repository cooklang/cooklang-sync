pub mod chunker;
pub mod errors;
pub mod file_watcher;
pub mod indexer;
pub mod local_db;
pub mod models;
pub mod schema;
pub mod syncer;

use futures::{channel::mpsc::channel, join, StreamExt};
use notify::{RecursiveMode, Watcher};
use std::path::PathBuf;

use log::debug;

use crate::chunker::{Chunker, InMemoryCache};
use crate::file_watcher::async_watcher;

const CHANNEL_SIZE: usize = 100;

pub async fn run(
    storage_dir: &str,
    db_file_path: &str,
    _remote_token: &str,
) -> Result<(), errors::SyncError> {
    let (mut watcher, local_file_update_rx) = async_watcher()?;
    let (local_registry_updated_tx, mut local_registry_updated_rx) = channel(CHANNEL_SIZE);

    let chunk_cache = InMemoryCache::new();
    let storage_dir = &PathBuf::from(storage_dir);
    let _chunker = Chunker::new(chunk_cache);
    let pool = local_db::get_connection_pool(db_file_path);
    debug!("Started connection pool for {:?}", db_file_path);

    // let mut remote = Remote(token);
    // let mut syncer = Syncer(remote, db, chunker, ready_to_updoad_rx);

    watcher.watch(storage_dir, RecursiveMode::Recursive)?;
    debug!("Started watcher on {:?}", storage_dir);

    let indexer = crate::indexer::run(
        &pool,
        storage_dir,
        local_file_update_rx,
        local_registry_updated_tx,
    );
    debug!("Started indexer on {:?}", storage_dir);

    let indexer_updates = async {
        while let Some(event) = local_registry_updated_rx.next().await {
            debug!("indexer_updates {:?}", event)
        }
    };

    // let syncer_upload_thread = std::thread::spawn({
    //     syncer.run_upload();
    // });

    // let syncer_download_thread = std::thread::spawn({
    //     syncer.run_download();
    // });

    // watcher_thread.join().unwrap();
    // indexer_thread.join().unwrap();

    // should return a callback to notify about rebuilding
    // can fail if authorization didn't work
    //

    join!(indexer, indexer_updates);

    Ok(())
}
