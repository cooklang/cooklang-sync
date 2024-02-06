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

const INTERVAL_CHECK_DOWNLOAD_SEC: Duration = Duration::from_secs(23);
const INTERVAL_CHECK_UPLOAD_SEC: Duration = Duration::from_secs(47);

pub async fn run(
    pool: &ConnectionPool,
    storage_path: &Path,
    chunker: &mut Chunker,
    remote: &Remote,
    mut local_registry_updated_rx: Receiver<models::IndexerUpdateEvent>,
) {
    let chunker = Arc::new(Mutex::new(chunker));

    let interval_check_download = async {
        let chunker = Arc::clone(&chunker);

        loop {
            debug!("download scan");

            let conn = &mut get_connection(pool).unwrap();

            let latest_local = registry::latest_jid(conn).unwrap_or(0);
            let to_download = remote.list(latest_local).await.unwrap();

            for d in &to_download {
                trace!("to be downloaded {:?}", d);

                let mut chunker = chunker.lock().await;

                if d.deleted {
                    let form = build_delete_form(&d.path, storage_path, d.id);
                    // TODO atomic?
                    chunker.delete(&d.path);
                    registry::delete(conn, &vec![form]);
                } else {
                    let chunks: Vec<&str> = d.chunk_ids.split(',').collect();

                    for c in &chunks {
                        if !chunker.check_chunk(c).unwrap() {
                            chunker.save_chunk(c, remote.download(c).await.unwrap());
                        }
                    }

                    if let Err(e) = chunker.save(&d.path, chunks) {
                        error!("{:?}", e);
                    }

                    let form = build_file_record(&d.path, storage_path, d.id).unwrap();
                    registry::create(conn, &vec![form]);
                }
            }

            tokio::time::sleep(INTERVAL_CHECK_DOWNLOAD_SEC).await;
        }
    };

    let interval_check_upload = async {
        let chunker = Arc::clone(&chunker);

        loop {
            debug!("upload scan");

            let conn = &mut get_connection(pool).unwrap();

            let to_upload = registry::updated_locally(conn).unwrap();

            for f in &to_upload {
                let mut chunker = chunker.lock().await;
                let mut chunk_ids = vec![String::from("")];

                if !f.deleted {
                    chunk_ids = chunker.hashify(&f.path).unwrap();
                }

                trace!("interval_check_upload {:?} {:?}", f, chunk_ids);
                let r = remote.commit(&f.path, f.deleted, &chunk_ids.join(","), "t").await.unwrap();

                match r {
                    CommitResultStatus::Success(jid) => {
                        trace!("commited {:?}", jid);
                        registry::update_jid(conn, f, jid);
                    },
                    CommitResultStatus::NeedChunks(chunks) => {
                        trace!("need chunks {:?}", chunks);
                        for c in chunks.split(',') {
                            // TODO bundle multiple into one request
                            remote.upload(c, chunker.read_chunk(c).unwrap()).await;
                        }
                    },
                }
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

    // remote_polling to change from remote to local

    join!(interval_check_download, interval_check_upload);
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
        deleted: true,
        size: 0,
        format: "t".to_string(),
        modified_at: OffsetDateTime::now_utc()
    }
}
