use futures::{channel::mpsc::Receiver, try_join, StreamExt};
use std::path::Path;

use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::Mutex;
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use log::{debug, error, trace};

use crate::chunker::Chunker;
use crate::connection::{get_connection, ConnectionPool};
use crate::errors::SyncError;
use crate::models;
use crate::registry;
use crate::remote::{CommitResultStatus, Remote};
use crate::{SyncStatus, SyncStatusListener};

type Result<T, E = SyncError> = std::result::Result<T, E>;

const INTERVAL_CHECK_UPLOAD_SEC: Duration = Duration::from_secs(47);
// TODO should be in sync in multiple places
const MAX_UPLOAD_SIZE: usize = 3_000_000;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    token: CancellationToken,
    listener: Option<Arc<dyn SyncStatusListener>>,
    pool: &ConnectionPool,
    storage_path: &Path,
    namespace_id: i32,
    chunker: &mut Chunker,
    remote: &Remote,
    local_registry_updated_rx: Receiver<models::IndexerUpdateEvent>,
    read_only: bool,
) -> Result<()> {
    let chunker = Arc::new(Mutex::new(chunker));

    if read_only {
        let _ = try_join!(download_loop(
            token.clone(),
            listener.clone(),
            pool,
            Arc::clone(&chunker),
            remote,
            storage_path,
            namespace_id
        ))?;
    } else {
        let _ = try_join!(
            download_loop(
                token.clone(),
                listener.clone(),
                pool,
                Arc::clone(&chunker),
                remote,
                storage_path,
                namespace_id
            ),
            upload_loop(
                token.clone(),
                listener.clone(),
                pool,
                Arc::clone(&chunker),
                remote,
                namespace_id,
                local_registry_updated_rx
            ),
        )?;
    }

    Ok(())
}

async fn download_loop(
    token: CancellationToken,
    listener: Option<Arc<dyn SyncStatusListener>>,
    pool: &ConnectionPool,
    chunker: Arc<Mutex<&mut Chunker>>,
    remote: &Remote,
    storage_path: &Path,
    namespace_id: i32,
) -> Result<()> {
    loop {
        // Check for cancellation at loop start
        if token.is_cancelled() {
            debug!("Download loop received shutdown signal");
            break;
        }

        // Notify that we're downloading
        if let Some(ref cb) = listener {
            cb.on_status_changed(SyncStatus::Downloading);
        }

        match check_download_once(
            pool,
            Arc::clone(&chunker),
            remote,
            storage_path,
            namespace_id,
        )
        .await
        {
            Ok(v) => v,
            Err(SyncError::Unauthorized) => return Err(SyncError::Unauthorized),
            Err(e) => return Err(SyncError::Unknown(format!("Check download failed: {}", e))),
        };

        // Return to idle after downloading
        if let Some(ref cb) = listener {
            cb.on_status_changed(SyncStatus::Idle);
        }

        // need to be longer than request timeout to make sure we don't get
        // client side timeout error
        tokio::select! {
            _ = token.cancelled() => {
                debug!("Download loop shutting down");
                break;
            }
            result = remote.poll() => {
                result?;
            }
        }
    }

    Ok(())
}

pub async fn upload_loop(
    token: CancellationToken,
    listener: Option<Arc<dyn SyncStatusListener>>,
    pool: &ConnectionPool,
    chunker: Arc<Mutex<&mut Chunker>>,
    remote: &Remote,
    namespace_id: i32,
    mut local_registry_updated_rx: Receiver<models::IndexerUpdateEvent>,
) -> Result<()> {
    // wait for indexer to work first
    tokio::time::sleep(Duration::from_secs(5)).await;

    loop {
        // Check for cancellation at loop start
        if token.is_cancelled() {
            debug!("Upload loop received shutdown signal");
            break;
        }

        // Notify that we're uploading
        if let Some(ref cb) = listener {
            cb.on_status_changed(SyncStatus::Uploading);
        }

        // need to wait only if we didn't upload anything
        // otherwise it should re-run immideately
        if check_upload_once(pool, Arc::clone(&chunker), remote, namespace_id).await? {
            // Return to idle after uploading
            if let Some(ref cb) = listener {
                cb.on_status_changed(SyncStatus::Idle);
            }

            // TODO test that it doesn't cancle stream
            tokio::select! {
                _ = token.cancelled() => {
                    debug!("Upload loop shutting down");
                    break;
                }
                _ = tokio::time::sleep(INTERVAL_CHECK_UPLOAD_SEC) => {},
                Some(_) = local_registry_updated_rx.next() => {},
            };
        } else {
            // If we still have work to do, don't set to idle - keep uploading status
            // and immediately continue the loop
        }
    }

    Ok(())
}

pub async fn check_upload_once(
    pool: &ConnectionPool,
    chunker: Arc<Mutex<&mut Chunker>>,
    remote: &Remote,
    namespace_id: i32,
) -> Result<bool> {
    debug!("upload scan");

    let conn = &mut get_connection(pool)?;
    let to_upload = registry::updated_locally(conn, namespace_id)?;

    let mut upload_queue: Vec<Vec<(String, Vec<u8>)>> = vec![vec![]];
    let mut size = 0;
    let mut last = upload_queue.last_mut().unwrap();
    let mut all_commited = true;

    for f in &to_upload {
        trace!("to upload {:?}", f);
        let mut chunker = chunker.lock().await;
        let mut chunk_ids = vec![String::from("")];

        if !f.deleted {
            // Also warms up the cache
            chunk_ids = chunker.hashify(&f.path).await?;
        }

        let r = remote
            .commit(&f.path, f.deleted, &chunk_ids.join(","))
            .await?;

        match r {
            CommitResultStatus::Success(jid) => {
                trace!("commit success");
                registry::update_jid(conn, f, jid)?;
            }
            CommitResultStatus::NeedChunks(chunks) => {
                trace!("need chunks");

                all_commited = false;

                for c in chunks.split(',') {
                    let data = chunker.read_chunk(c)?;
                    size += data.len();
                    last.push((c.into(), data));

                    if size > MAX_UPLOAD_SIZE {
                        upload_queue.push(vec![]);
                        last = upload_queue.last_mut().unwrap();
                        size = 0;
                    }
                }
            }
        }
    }

    for batch in upload_queue {
        if !batch.is_empty() {
            remote.upload_batch(batch).await?;
        }
    }

    Ok(all_commited)
}

pub async fn check_download_once(
    pool: &ConnectionPool,
    chunker: Arc<Mutex<&mut Chunker>>,
    remote: &Remote,
    storage_path: &Path,
    namespace_id: i32,
) -> Result<bool> {
    debug!("download scan");

    let conn = &mut get_connection(pool)?;

    let latest_local = registry::latest_jid(conn, namespace_id).unwrap_or(0);
    let to_download = remote.list(latest_local).await?;
    // TODO maybe should limit one download at a time and use batches
    // it can also overflow in-memory cache
    let mut download_queue: Vec<&str> = vec![];

    for d in &to_download {
        trace!("collecting needed chunks for {:?}", d);

        if d.deleted {
            continue;
        }

        let mut chunker = chunker.lock().await;

        // Warm-up cache to include chunks from an old file
        if chunker.exists(&d.path) {
            chunker.hashify(&d.path).await?;
        }

        for c in d.chunk_ids.split(',') {
            if chunker.check_chunk(c) {
                continue;
            }

            download_queue.push(c);
        }
    }

    if !download_queue.is_empty() {
        let mut chunker = chunker.lock().await;

        let mut downloaded = remote.download_batch(download_queue).await;

        while let Some(result) = downloaded.next().await {
            match result {
                Ok((chunk_id, data)) => {
                    chunker.save_chunk(&chunk_id, data)?;
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
    }

    for d in &to_download {
        trace!("udpating downloaded files {:?}", d);

        let mut chunker = chunker.lock().await;

        if d.deleted {
            let form = build_delete_form(&d.path, storage_path, d.id, namespace_id);
            // TODO atomic?
            registry::delete(conn, &vec![form])?;
            if chunker.exists(&d.path) {
                chunker.delete(&d.path).await?;
            }
        } else {
            let chunks: Vec<&str> = d.chunk_ids.split(',').collect();
            // TODO atomic? store in tmp first and then move?
            // TODO should be after we create record in db
            if let Err(e) = chunker.save(&d.path, chunks).await {
                error!("{:?}", e);
                return Err(e);
            }

            let form = build_file_record(&d.path, storage_path, d.id, namespace_id)?;
            registry::create(conn, &vec![form])?;
        }
    }

    Ok(!to_download.is_empty())
}

fn build_file_record(
    path: &str,
    base: &Path,
    jid: i32,
    namespace_id: i32,
) -> Result<models::CreateForm, SyncError> {
    let mut full_path = base.to_path_buf();
    full_path.push(path);
    let metadata = full_path
        .metadata()
        .map_err(|e| SyncError::from_io_error(path, e))?;
    let size: i64 = metadata.len().try_into()?;
    let time = metadata
        .modified()
        .map_err(|e| SyncError::from_io_error(path, e))?;
    let modified_at = OffsetDateTime::from(time);

    let form = models::CreateForm {
        jid: Some(jid),
        path: path.to_string(),
        deleted: false,
        size,
        modified_at,
        namespace_id,
    };

    Ok(form)
}

fn build_delete_form(path: &str, base: &Path, jid: i32, namespace_id: i32) -> models::DeleteForm {
    let mut full_path = base.to_path_buf();
    full_path.push(path);

    models::DeleteForm {
        path: path.to_string(),
        jid: Some(jid),
        deleted: true,
        size: 0,
        modified_at: OffsetDateTime::now_utc(),
        namespace_id,
    }
}
