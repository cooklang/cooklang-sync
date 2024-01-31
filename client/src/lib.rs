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
    api_endpoint: &str,
    remote_token: &str,
) -> Result<(), errors::SyncError> {
    let (mut watcher, local_file_update_rx) = async_watcher()?;
    let (local_registry_updated_tx, mut local_registry_updated_rx) = channel(CHANNEL_SIZE);

    let storage_dir = &PathBuf::from(storage_dir);
    let chunk_cache = InMemoryCache::new();
    let chunker = Chunker::new(chunk_cache);
    let remote = syncer::remote::Remote::new(remote_token);

    let pool = local_db::get_connection_pool(db_file_path);
    debug!("Started connection pool for {:?}", db_file_path);

    watcher.watch(storage_dir, RecursiveMode::Recursive)?;
    debug!("Started watcher on {:?}", storage_dir);

    let indexer = indexer::run(
        &pool,
        storage_dir,
        local_file_update_rx,
        local_registry_updated_tx,
    );
    debug!("Started indexer on {:?}", storage_dir);

    let syncer = syncer::run(
        &pool,
        chunker,
        remote,
        local_registry_updated_rx,
    );
    debug!("Started syncer");

    join!(indexer, syncer);

    Ok(())
}