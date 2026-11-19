use futures::{channel::mpsc::Receiver, try_join, StreamExt};
use std::collections::HashSet;
use std::path::Path;

use std::sync::Arc;
use time::OffsetDateTime;
use tokio::sync::{Mutex, OwnedMutexGuard};
use tokio::time::Duration;
use tokio_util::sync::CancellationToken;

use log::{debug, error, trace};

use crate::chunker::{estimated_chunk_size, Chunker};
use crate::connection::{get_connection, ConnectionPool};
use crate::errors::SyncError;
use crate::indexer::truncate_to_seconds;
use crate::models;
use crate::registry;
use crate::remote::{CommitResultStatus, Remote, ResponseFileRecord};
use crate::{SyncStatus, SyncStatusListener};

type Result<T, E = SyncError> = std::result::Result<T, E>;

const INTERVAL_CHECK_UPLOAD_SEC: Duration = Duration::from_secs(47);
// TODO should be in sync in multiple places
const MAX_UPLOAD_SIZE: usize = 3_000_000;

/// Most chunks one download request asks for. A text file is one chunk per
/// line, so this is what bounds a batch of recipes.
pub const DOWNLOAD_BATCH_MAX_CHUNKS: usize = 2_000;
/// Roughly how much content one download request asks for; this is what
/// bounds a batch of photos. Small enough that a slow connection finishes a
/// batch well inside the request timeout, and that files show up every few
/// seconds on a first sync.
pub const DOWNLOAD_BATCH_MAX_BYTES: usize = 16 * 1024 * 1024;

#[allow(clippy::too_many_arguments)]
pub async fn run(
    token: CancellationToken,
    listener: Option<Arc<dyn SyncStatusListener>>,
    pool: &ConnectionPool,
    storage_path: &Path,
    namespace_id: i32,
    chunker: Chunker,
    remote: &Remote,
    local_registry_updated_rx: Receiver<models::IndexerUpdateEvent>,
    read_only: bool,
) -> Result<()> {
    let chunker = Arc::new(Mutex::new(chunker));

    // Downloads and uploads take turns, and the first turn is a download.
    // The next download asks the server for everything after the highest id
    // recorded locally, and a commit records an id higher than anything
    // already on the server: one that lands before a download has finished
    // would move that point past files not fetched yet, and an interrupted
    // download would never go back for them.
    let turn = Arc::new(Mutex::new(()));
    let first_turn = Arc::clone(&turn).lock_owned().await;

    if read_only {
        let _ = try_join!(download_loop(
            token.clone(),
            listener.clone(),
            pool,
            Arc::clone(&chunker),
            remote,
            storage_path,
            namespace_id,
            Arc::clone(&turn),
            first_turn
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
                namespace_id,
                Arc::clone(&turn),
                first_turn
            ),
            upload_loop_in_turn(
                token.clone(),
                listener.clone(),
                pool,
                Arc::clone(&chunker),
                remote,
                namespace_id,
                local_registry_updated_rx,
                turn
            ),
        )?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn download_loop(
    token: CancellationToken,
    listener: Option<Arc<dyn SyncStatusListener>>,
    pool: &ConnectionPool,
    chunker: Arc<Mutex<Chunker>>,
    remote: &Remote,
    storage_path: &Path,
    namespace_id: i32,
    turn: Arc<Mutex<()>>,
    first_turn: OwnedMutexGuard<()>,
) -> Result<()> {
    let mut held_turn = Some(first_turn);

    loop {
        // Check for cancellation at loop start
        if token.is_cancelled() {
            debug!("Download loop received shutdown signal");
            break;
        }

        // Held until this pass has everything, so no commit lands mid-way.
        let download_turn = match held_turn.take() {
            Some(turn) => turn,
            None => tokio::select! {
                _ = token.cancelled() => {
                    debug!("Download loop shutting down");
                    break;
                }
                turn = Arc::clone(&turn).lock_owned() => turn,
            },
        };

        // Notify that we're downloading
        if let Some(ref cb) = listener {
            cb.on_status_changed(SyncStatus::Downloading);
        }

        match check_download(
            pool,
            Arc::clone(&chunker),
            remote,
            storage_path,
            namespace_id,
            Some(&token),
        )
        .await
        {
            Ok(v) => v,
            Err(SyncError::Unauthorized) => return Err(SyncError::Unauthorized),
            // 402 must survive to the caller typed — every client keys its
            // "sync needs a plan" state off this variant, and the download
            // check is usually the first call to hit the server.
            Err(SyncError::PaymentRequired) => return Err(SyncError::PaymentRequired),
            Err(e) => return Err(SyncError::Unknown(format!("Check download failed: {}", e))),
        };

        // A stopped download is unfinished: reporting idle would tell the
        // caller everything has arrived.
        if token.is_cancelled() {
            debug!("Download loop shutting down");
            break;
        }

        drop(download_turn);

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
    chunker: Arc<Mutex<Chunker>>,
    remote: &Remote,
    namespace_id: i32,
    local_registry_updated_rx: Receiver<models::IndexerUpdateEvent>,
) -> Result<()> {
    upload_loop_in_turn(
        token,
        listener,
        pool,
        chunker,
        remote,
        namespace_id,
        local_registry_updated_rx,
        Arc::new(Mutex::new(())),
    )
    .await
}

/// The upload loop, taking turns with the download loop over `turn`.
#[allow(clippy::too_many_arguments)]
async fn upload_loop_in_turn(
    token: CancellationToken,
    listener: Option<Arc<dyn SyncStatusListener>>,
    pool: &ConnectionPool,
    chunker: Arc<Mutex<Chunker>>,
    remote: &Remote,
    namespace_id: i32,
    mut local_registry_updated_rx: Receiver<models::IndexerUpdateEvent>,
    turn: Arc<Mutex<()>>,
) -> Result<()> {
    // wait for indexer to work first
    tokio::time::sleep(Duration::from_secs(5)).await;

    loop {
        // Check for cancellation at loop start
        if token.is_cancelled() {
            debug!("Upload loop received shutdown signal");
            break;
        }

        // Wait for a download in progress to finish before committing
        // anything (see `run`).
        let upload_turn = tokio::select! {
            _ = token.cancelled() => {
                debug!("Upload loop shutting down");
                break;
            }
            turn = turn.lock() => turn,
        };

        // Notify that we're uploading
        if let Some(ref cb) = listener {
            cb.on_status_changed(SyncStatus::Uploading);
        }

        let all_committed =
            check_upload_once(pool, Arc::clone(&chunker), remote, namespace_id).await?;
        drop(upload_turn);

        // need to wait only if we didn't upload anything
        // otherwise it should re-run immideately
        if all_committed {
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
    chunker: Arc<Mutex<Chunker>>,
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
    chunker: Arc<Mutex<Chunker>>,
    remote: &Remote,
    storage_path: &Path,
    namespace_id: i32,
) -> Result<bool> {
    check_download(pool, chunker, remote, storage_path, namespace_id, None).await
}

/// Downloads everything the server has that this client has not, a batch at a
/// time. Each batch is fetched, written to disk and recorded before the next
/// one starts, so files appear as they arrive and an interrupted run resumes
/// after the last finished batch instead of from the beginning.
async fn check_download(
    pool: &ConnectionPool,
    chunker: Arc<Mutex<Chunker>>,
    remote: &Remote,
    storage_path: &Path,
    namespace_id: i32,
    token: Option<&CancellationToken>,
) -> Result<bool> {
    debug!("download scan");

    let conn = &mut get_connection(pool)?;

    let latest_local = registry::latest_jid(conn, namespace_id).unwrap_or(0);
    let mut to_download = remote.list(latest_local).await?;

    // The next scan resumes after the highest id recorded locally, so records
    // have to be applied lowest id first: anything skipped below that id
    // would never be asked for again.
    to_download.sort_by_key(|d| d.id);

    for batch in download_batches(&to_download) {
        if token.is_some_and(|t| t.is_cancelled()) {
            debug!("download stopped between batches");
            break;
        }

        let mut chunker = chunker.lock().await;

        let mut download_queue: Vec<&str> = vec![];
        let mut queued: HashSet<&str> = HashSet::new();

        for d in batch {
            trace!("collecting needed chunks for {:?}", d);

            if d.deleted {
                continue;
            }

            // Warm-up cache to include chunks from an old file
            if chunker.exists(&d.path) {
                chunker.hashify(&d.path).await?;
            }

            for c in d.chunk_ids.split(',') {
                if chunker.check_chunk(c) || !queued.insert(c) {
                    continue;
                }

                download_queue.push(c);
            }
        }

        if !download_queue.is_empty() {
            let mut downloaded = remote.download_batch(download_queue).await;

            loop {
                // A batch can take minutes on a slow connection, or hang on a
                // dead one: a stop must not wait for it.
                let next = match token {
                    Some(token) => tokio::select! {
                        _ = token.cancelled() => {
                            debug!("download stopped mid-batch");
                            return Ok(true);
                        }
                        next = downloaded.next() => next,
                    },
                    None => downloaded.next().await,
                };

                match next {
                    Some(result) => {
                        let (chunk_id, data) = result?;
                        chunker.save_chunk(&chunk_id, data)?;
                    }
                    None => break,
                }
            }
        }

        for d in batch {
            trace!("udpating downloaded files {:?}", d);

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
    }

    Ok(!to_download.is_empty())
}

/// Splits records into runs that each fit one download request: at most
/// `DOWNLOAD_BATCH_MAX_CHUNKS` chunks and about `DOWNLOAD_BATCH_MAX_BYTES`
/// of content. A file is never split, so one larger than the limits is a
/// batch of its own.
fn download_batches(records: &[ResponseFileRecord]) -> Vec<&[ResponseFileRecord]> {
    let mut batches = vec![];
    let mut start = 0;
    let mut chunks = 0;
    let mut bytes = 0;

    for (i, record) in records.iter().enumerate() {
        let (record_chunks, record_bytes) = if record.deleted {
            (0, 0)
        } else {
            record
                .chunk_ids
                .split(',')
                .fold((0, 0), |(n, size), c| (n + 1, size + estimated_chunk_size(c)))
        };

        let over = chunks + record_chunks > DOWNLOAD_BATCH_MAX_CHUNKS
            || bytes + record_bytes > DOWNLOAD_BATCH_MAX_BYTES;

        if over && i > start {
            batches.push(&records[start..i]);
            start = i;
            chunks = 0;
            bytes = 0;
        }

        chunks += record_chunks;
        bytes += record_bytes;
    }

    if start < records.len() {
        batches.push(&records[start..]);
    }

    batches
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
    // Keep mtime precision aligned with the indexer so round-tripping a
    // downloaded file through the local DB doesn't re-trigger uploads.
    let modified_at = truncate_to_seconds(OffsetDateTime::from(time));

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
        modified_at: truncate_to_seconds(OffsetDateTime::now_utc()),
        namespace_id,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const BINARY_CHUNK: &str = "0123456789abcdef0123456789abcdef";
    const TEXT_CHUNK: &str = "0123456789";

    fn record(id: i32, chunk: &str, chunks: usize) -> ResponseFileRecord {
        ResponseFileRecord {
            id,
            path: format!("{id}.bin"),
            deleted: false,
            chunk_ids: vec![chunk; chunks].join(","),
        }
    }

    fn ids(batches: &[&[ResponseFileRecord]]) -> Vec<Vec<i32>> {
        batches
            .iter()
            .map(|b| b.iter().map(|r| r.id).collect())
            .collect()
    }

    #[test]
    fn download_batches_of_nothing_is_empty() {
        assert!(download_batches(&[]).is_empty());
    }

    #[test]
    fn download_batches_keeps_small_files_together() {
        let records = vec![record(1, TEXT_CHUNK, 30), record(2, TEXT_CHUNK, 30)];
        assert_eq!(ids(&download_batches(&records)), vec![vec![1, 2]]);
    }

    #[test]
    fn download_batches_closes_a_batch_at_the_size_limit() {
        // Binary chunks count as 1 MB each: 10 + 10 MB does not fit 16 MB.
        let records = vec![
            record(1, BINARY_CHUNK, 10),
            record(2, BINARY_CHUNK, 10),
            record(3, BINARY_CHUNK, 2),
        ];
        assert_eq!(ids(&download_batches(&records)), vec![vec![1], vec![2, 3]]);
    }

    #[test]
    fn download_batches_gives_an_oversized_file_a_batch_of_its_own() {
        let records = vec![
            record(1, TEXT_CHUNK, 5),
            record(2, BINARY_CHUNK, 40),
            record(3, TEXT_CHUNK, 5),
        ];
        assert_eq!(ids(&download_batches(&records)), vec![vec![1], vec![2], vec![3]]);
    }

    #[test]
    fn download_batches_carries_deletions_at_no_cost() {
        let mut deleted = record(2, BINARY_CHUNK, 1);
        deleted.deleted = true;
        deleted.chunk_ids = String::new();
        let records = vec![record(1, BINARY_CHUNK, 16), deleted];
        assert_eq!(ids(&download_batches(&records)), vec![vec![1, 2]]);
    }
}
