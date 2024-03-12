pub mod chunker;
pub mod connection;
pub mod errors;
pub mod file_watcher;
pub mod indexer;
pub mod models;
pub mod registry;
pub mod remote;
pub mod schema;
pub mod syncer;

use futures::{channel::mpsc::channel, try_join};
use notify::RecursiveMode;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;

use log::debug;

use crate::chunker::{Chunker, InMemoryCache};
use crate::file_watcher::async_watcher;
use crate::indexer::check_index_once;
use crate::syncer::{check_download_once, check_upload_once};

const CHANNEL_SIZE: usize = 100;
const INMEMORY_CACHE_MAX_REC: usize = 100000;
const INMEMORY_CACHE_MAX_MEM: u64 = 100_000_000_000;

uniffi::setup_scaffolding!();

/// Synchronous alias to async run function.
/// Intended to used by external (written in other languages) callers.
#[uniffi::export]
pub fn run(
    storage_dir: &str,
    db_file_path: &str,
    api_endpoint: &str,
    remote_token: &str,
    download_only: bool,
) -> Result<(), errors::SyncError> {
    Runtime::new()?.block_on(run_async(
        storage_dir,
        db_file_path,
        api_endpoint,
        remote_token,
        download_only,
    ))?;

    Ok(())
}

/// Connects to the server and waits either when `wait_time` expires or
/// when there's a remote update for this client.
/// Note, it doesn't do the update itself, you need to use `run_download_once`
/// after this function completes.
#[uniffi::export]
pub fn wait_remote_update(
    api_endpoint: &str,
    remote_token: &str,
    wait_time: i32,
) -> Result<(), errors::SyncError> {
    Runtime::new()?.block_on(remote::Remote::new(api_endpoint, remote_token).poll(wait_time))?;

    Ok(())
}

/// Runs one-off download of updates from remote server.
/// Note, it's not very efficient as requires to re-initialize DB connection,
/// chunker, remote client, etc every time it runs.
#[uniffi::export]
pub fn run_download_once(
    storage_dir: &str,
    db_file_path: &str,
    api_endpoint: &str,
    remote_token: &str,
) -> Result<(), errors::SyncError> {
    let storage_dir = &PathBuf::from(storage_dir);
    let chunk_cache = InMemoryCache::new(INMEMORY_CACHE_MAX_REC, INMEMORY_CACHE_MAX_MEM);
    let chunker = &mut Chunker::new(chunk_cache, storage_dir.clone());
    let chunker = Arc::new(Mutex::new(chunker));
    let remote = &remote::Remote::new(api_endpoint, remote_token);

    let pool = connection::get_connection_pool(db_file_path)?;
    debug!("Started connection pool for {:?}", db_file_path);

    Runtime::new()?.block_on(check_download_once(
        &pool,
        Arc::clone(&chunker),
        remote,
        storage_dir,
    ))?;

    Ok(())
}

/// Runs one-off upload of updates to remote server.
/// Note, it's not very efficient as requires to re-initialize DB connection,
/// chunker, remote client, etc every time it runs.
#[uniffi::export]
pub fn run_upload_once(
    storage_dir: &str,
    db_file_path: &str,
    api_endpoint: &str,
    remote_token: &str,
) -> Result<(), errors::SyncError> {
    let storage_dir = &PathBuf::from(storage_dir);
    let chunk_cache = InMemoryCache::new(INMEMORY_CACHE_MAX_REC, INMEMORY_CACHE_MAX_MEM);
    let chunker = &mut Chunker::new(chunk_cache, storage_dir.clone());
    let chunker = Arc::new(Mutex::new(chunker));
    let remote = &remote::Remote::new(api_endpoint, remote_token);

    let pool = connection::get_connection_pool(db_file_path)?;
    debug!("Started connection pool for {:?}", db_file_path);

    check_index_once(&pool, storage_dir)?;

    let runtime = Runtime::new()?;

    // It requires first pass to upload missing chunks and second to
    // commit and update `jid` to local records.
    if !runtime.block_on(check_upload_once(&pool, Arc::clone(&chunker), remote))? {
        runtime.block_on(check_upload_once(&pool, Arc::clone(&chunker), remote))?;
    }

    Ok(())
}

/// Runs local files watch and sync from/to remote continuously.
async fn run_async(
    storage_dir: &str,
    db_file_path: &str,
    api_endpoint: &str,
    remote_token: &str,
    download_only: bool,
) -> Result<(), errors::SyncError> {
    let (mut debouncer, local_file_update_rx) = async_watcher()?;
    let (local_registry_updated_tx, local_registry_updated_rx) = channel(CHANNEL_SIZE);

    let storage_dir = &PathBuf::from(storage_dir);
    let chunk_cache = InMemoryCache::new(INMEMORY_CACHE_MAX_REC, INMEMORY_CACHE_MAX_MEM);
    let chunker = &mut Chunker::new(chunk_cache, storage_dir.clone());
    let remote = &remote::Remote::new(api_endpoint, remote_token);

    let pool = connection::get_connection_pool(db_file_path)?;
    debug!("Started connection pool for {:?}", db_file_path);

    if !download_only {
        debouncer
            .watcher()
            .watch(storage_dir, RecursiveMode::Recursive)?;
        debug!("Started watcher on {:?}", storage_dir);
    }

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
        download_only,
    );
    debug!("Started syncer");

    let _ = try_join!(indexer, syncer)?;

    Ok(())
}
