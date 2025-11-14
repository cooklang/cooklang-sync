use futures::{channel::mpsc::channel, try_join};
use jsonwebtoken::{decode, Algorithm, DecodingKey, TokenData, Validation};
use notify::RecursiveMode;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::runtime::Runtime;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;

use log::debug;

use crate::chunker::{Chunker, InMemoryCache};
use crate::file_watcher::async_watcher;
use crate::indexer::check_index_once;
use crate::syncer::{check_download_once, check_upload_once};

const CHANNEL_SIZE: usize = 100;
const INMEMORY_CACHE_MAX_REC: usize = 100000;
const INMEMORY_CACHE_MAX_MEM: u64 = 100_000_000_000;
const DUMMY_SECRET: &[u8] = b"dummy_secret";

pub mod chunker;
pub mod connection;
pub mod context;
pub mod errors;
pub mod file_watcher;
pub mod indexer;
pub mod models;
pub mod registry;
pub mod remote;
pub mod schema;
pub mod syncer;

// Export SyncStatus and context types for external use
pub use context::{SyncContext, SyncStatusListener};
pub use models::SyncStatus;

pub fn extract_uid_from_jwt(token: &str) -> i32 {
    let mut validation = Validation::new(Algorithm::HS256);

    // Disabling signature validation because we don't know real secret
    validation.insecure_disable_signature_validation();

    #[derive(Debug, Clone, Serialize, Deserialize)]
    struct Claims {
        uid: i32,
    }

    let token_data: TokenData<Claims> =
        decode::<Claims>(token, &DecodingKey::from_secret(DUMMY_SECRET), &validation)
            .expect("Failed to decode token");

    token_data.claims.uid
}

uniffi::setup_scaffolding!();

/// Synchronous alias to async run function.
/// Intended to used by external (written in other languages) callers.
#[uniffi::export]
pub fn run(
    context: Arc<SyncContext>,
    storage_dir: &str,
    db_file_path: &str,
    api_endpoint: &str,
    remote_token: &str,
    namespace_id: i32,
    download_only: bool,
) -> Result<(), errors::SyncError> {
    let token = context.token();
    let listener = context.listener();

    Runtime::new()?.block_on(run_async(
        token,
        listener,
        storage_dir,
        db_file_path,
        api_endpoint,
        remote_token,
        namespace_id,
        download_only,
    ))?;

    Ok(())
}

/// Connects to the server and waits either when `wait_time` expires or
/// when there's a remote update for this client.
/// Note, it doesn't do the update itself, you need to use `run_download_once`
/// after this function completes.
#[uniffi::export]
pub fn wait_remote_update(api_endpoint: &str, remote_token: &str) -> Result<(), errors::SyncError> {
    Runtime::new()?.block_on(remote::Remote::new(api_endpoint, remote_token).poll())?;

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
    namespace_id: i32,
) -> Result<(), errors::SyncError> {
    use std::env;

    env::set_var("CARGO_LOG", "trace");

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
        namespace_id,
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
    namespace_id: i32,
) -> Result<(), errors::SyncError> {
    let storage_dir = &PathBuf::from(storage_dir);
    let chunk_cache = InMemoryCache::new(INMEMORY_CACHE_MAX_REC, INMEMORY_CACHE_MAX_MEM);
    let chunker = &mut Chunker::new(chunk_cache, storage_dir.clone());
    let chunker = Arc::new(Mutex::new(chunker));
    let remote = &remote::Remote::new(api_endpoint, remote_token);

    let pool = connection::get_connection_pool(db_file_path)?;
    debug!("Started connection pool for {:?}", db_file_path);

    check_index_once(&pool, storage_dir, namespace_id)?;

    let runtime = Runtime::new()?;

    // It requires first pass to upload missing chunks and second to
    // commit and update `jid` to local records.
    if !runtime.block_on(check_upload_once(
        &pool,
        Arc::clone(&chunker),
        remote,
        namespace_id,
    ))? {
        runtime.block_on(check_upload_once(
            &pool,
            Arc::clone(&chunker),
            remote,
            namespace_id,
        ))?;
    }

    Ok(())
}

/// Runs local files watch and sync from/to remote continuously.
#[allow(clippy::too_many_arguments)]
pub async fn run_async(
    token: CancellationToken,
    listener: Option<Arc<dyn SyncStatusListener>>,
    storage_dir: &str,
    db_file_path: &str,
    api_endpoint: &str,
    remote_token: &str,
    namespace_id: i32,
    download_only: bool,
) -> Result<(), errors::SyncError> {
    // Initialize all components first
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

    // Notify syncing status after successful initialization
    if let Some(ref cb) = listener {
        cb.on_status_changed(SyncStatus::Syncing);
    }

    let indexer = indexer::run(
        token.clone(),
        listener.clone(),
        &pool,
        storage_dir,
        namespace_id,
        local_file_update_rx,
        local_registry_updated_tx,
    );
    debug!("Started indexer on {:?}", storage_dir);

    let syncer = syncer::run(
        token.clone(),
        listener.clone(),
        &pool,
        storage_dir,
        namespace_id,
        chunker,
        remote,
        local_registry_updated_rx,
        download_only,
    );
    debug!("Started syncer");

    let result = try_join!(indexer, syncer);

    // Notify completion (on_complete includes success status and optional error message)
    if let Some(ref cb) = listener {
        match result {
            Ok(_) => cb.on_complete(true, None),
            Err(ref e) => cb.on_complete(false, Some(format!("{:?}", e))),
        }
    }

    result?;
    Ok(())
}
