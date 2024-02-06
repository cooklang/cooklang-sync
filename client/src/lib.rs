pub mod chunker;
pub mod errors;
pub mod file_watcher;
pub mod indexer;
pub mod registry;
pub mod models;
pub mod schema;
pub mod syncer;
pub mod remote;
pub mod connection;

use futures::{channel::mpsc::channel, join};
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
    let (local_registry_updated_tx, local_registry_updated_rx) = channel(CHANNEL_SIZE);

    let storage_dir = &PathBuf::from(storage_dir);
    let chunk_cache = InMemoryCache::new(1000, 100_000_000);
    let chunker = &mut Chunker::new(chunk_cache, storage_dir.clone());
    let remote = &remote::Remote::new(api_endpoint, remote_token);

    let pool = connection::get_connection_pool(db_file_path)?;
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
        storage_dir,
        chunker,
        remote,
        local_registry_updated_rx,
    );
    debug!("Started syncer");

    join!(indexer, syncer);

    Ok(())
}
