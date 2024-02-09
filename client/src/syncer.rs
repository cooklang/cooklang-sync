use futures::{
    channel::mpsc::{Receiver},
    join, StreamExt,
};
use std::path::Path;

use time::OffsetDateTime;
use tokio::time::Duration;
use std::sync::Arc;
use tokio::sync::Mutex;

use log::{debug, trace, error};

use crate::registry;
use crate::connection::{ConnectionPool, get_connection};
use crate::models;
use crate::errors::SyncError;
use crate::chunker::Chunker;
use crate::remote::{Remote, CommitResultStatus};

const INTERVAL_CHECK_DOWNLOAD_SEC: i32 = 307;
const INTERVAL_CHECK_UPLOAD_SEC: Duration = Duration::from_secs(47);

pub async fn run(
    pool: &ConnectionPool,
    storage_path: &Path,
    chunker: &mut Chunker,
    remote: &Remote,
    mut local_registry_updated_rx: Receiver<models::IndexerUpdateEvent>,
) {
    let chunker = Arc::new(Mutex::new(chunker));

    let check_download = async {
        let chunker = Arc::clone(&chunker);

        loop {
            debug!("download scan");

            let conn = &mut get_connection(pool).unwrap();

            let latest_local = registry::latest_jid(conn).unwrap_or(0);
            let to_download = remote.list(latest_local).await.unwrap();

            for d in &to_download {
                trace!("to download {:?}", d);

                let mut chunker = chunker.lock().await;

                if d.deleted {
                    let form = build_delete_form(&d.path, storage_path, d.id);
                    // TODO atomic?
                    registry::delete(conn, &vec![form]);
                    chunker.delete(&d.path);
                } else {
                    let chunks: Vec<&str> = d.chunk_ids.split(',').collect();

                    // Warm-up cache to include chunks from an old file
                    if chunker.exists(&d.path) {
                        chunker.hashify(&d.path);
                    }

                    for c in &chunks {
                        if !chunker.check_chunk(c).unwrap() {
                            chunker.save_chunk(c, remote.download(c).await.unwrap());
                        }
                    }

                    // TODO atomic? store in tmp first and then move?
                    // TODO should be after we create record in db
                    if let Err(e) = chunker.save(&d.path, chunks) {
                        error!("{:?}", e);
                    }

                    let form = build_file_record(&d.path, storage_path, d.id).unwrap();
                    registry::create(conn, &vec![form]);
                }
            }

            remote.poll(INTERVAL_CHECK_DOWNLOAD_SEC).await;
        }
    };

    let check_upload = async {
        // wait for indexer to work first
        tokio::time::sleep(Duration::from_secs(5)).await;

        let chunker = Arc::clone(&chunker);

        loop {
            debug!("upload scan");

            let conn = &mut get_connection(pool).unwrap();

            let to_upload = registry::updated_locally(conn).unwrap();

            let mut chunker = chunker.lock().await;
            let mut upload_payload: Vec<Vec<(String, Vec<u8>)>> = vec![vec![]];
            let mut size = 0;
            let mut last = upload_payload.last_mut().unwrap();

            for f in &to_upload {
                trace!("to upload {:?}", f);

                let mut chunk_ids = vec![String::from("")];

                if !f.deleted {
                    // Also warms up the cache
                    chunk_ids = chunker.hashify(&f.path).unwrap();
                }

                let r = remote.commit(&f.path, f.deleted, &chunk_ids.join(","), "t").await.unwrap();

                match r {
                    CommitResultStatus::Success(jid) => {
                        registry::update_jid(conn, f, jid);
                    },
                    CommitResultStatus::NeedChunks(chunks) => {
                        for c in chunks.split(',') {
                            let data = chunker.read_chunk(c).unwrap();
                            size += data.len();
                            last.push((c.into(), data));

                            if size > 1_000_000 {
                                upload_payload.push(vec![]);
                                last = upload_payload.last_mut().unwrap();
                                size = 0;
                            }
                        }
                    },
                }
            }

            for batch in upload_payload {
                remote.upload_batch(batch).await;
            }

            // need to wait only if we didn't upload anything
            // otherwise it should re-run immideately
            if to_upload.is_empty() {
                // TODO test that it doesn't cancle stream
                tokio::select! {
                    _ = tokio::time::sleep(INTERVAL_CHECK_UPLOAD_SEC) => {},
                    Some(_) = local_registry_updated_rx.next() => {},
                };
            }
        }
    };

    join!(check_download, check_upload);
}

fn build_file_record(path: &str, base: &Path, jid: i32) -> Result<models::CreateForm,SyncError> {
    let mut full_path = base.to_path_buf();
    full_path.push(path);
    let metadata =full_path.metadata()?;
    let size: i64 = metadata.len().try_into()?;
    let time = metadata.modified()?;
    let modified_at = OffsetDateTime::from(time);

    let form = models::CreateForm {
        jid: Some(jid),
        path: path.to_string(),
        deleted: false,
        size,
        format: "t".to_string(),
        modified_at,
    };

    Ok(form)
}

fn build_delete_form(path: &str, base: &Path, jid: i32) -> models::DeleteForm {
    let mut full_path = base.to_path_buf();
    full_path.push(path);

    models::DeleteForm {
        path: path.to_string(),
        jid: Some(jid),
        deleted: true,
        size: 0,
        format: "t".to_string(),
        modified_at: OffsetDateTime::now_utc()
    }
}
