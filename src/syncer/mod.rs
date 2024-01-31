pub mod remote;

use futures::{
    channel::mpsc::{Receiver, Sender},
    join, SinkExt, StreamExt,
};

use notify::Event;
use time::OffsetDateTime;
use tokio::time::Duration;

use log::{debug, error, trace};

use crate::local_db::*;
use crate::models::*;
use crate::chunker::*;

const CHECK_INTERVAL_WAIT_SEC: Duration = Duration::from_secs(47);

pub async fn run(
    pool: &ConnectionPool,
    chunker: Chunker<InMemoryCache>,
    remote: remote::Remote,
    mut local_registry_updated_rx: Receiver<IndexerUpdateEvent>,
) {
    let check_on_interval = async {
        loop {
            debug!("[syncer] interval scan");
            if let Err(e) = do_sync(pool).await {
                // Handle the error, for example, log it
                error!("Error in do_sync: {}", e);
                break; // or continue, depending on how you want to handle errors
            }

            tokio::time::sleep(CHECK_INTERVAL_WAIT_SEC).await;
        }
    };

    let monitor_indexer_updates = async {
        while let Some(event) = local_registry_updated_rx.next().await {
            trace!("fs event triggered {:?}", event);

            if let Err(e) = do_sync(pool).await {
                // Handle the error, for example, log it
                error!("Error in do_sync: {}", e);
                break; // or continue, depending on how you want to handle errors
            }
        }
    };

    // remote_polling to change from remote to local

    join!(check_on_interval, monitor_indexer_updates);
}

async fn do_sync(
    pool: &ConnectionPool
) -> Result<(), Box<dyn std::error::Error>> {

    let conn = &mut pool.get().unwrap();

    let to_upload = updated_locally_file_records(conn);

    trace!("{:?}", to_update);

    Ok(())
}

fn run_upload() {
    // sync_with_remote in intervals
}

fn run_download() {
    // watch on local updates and then local_file_updated
}

//
// upload changes logic
//

// should be called after indexer figures out that local file was changed
// if local file was updated but app just started? - maybe indexer will check modified time stored in DB
// and find ones which are different from stored in DB
fn local_file_updated(path: String) {
    // match commit() {
    //     NeedUpload(hashes) => {
    //         while {
    //             store_batch // optimised by content size
    //         }

    //         local_file_updated()
    //     }
    //     Commited => store new jid to local DB
    //     Synced =>
    //     Error =>
    // }
}

// content should be data bytes instead
fn commit(namespace: i64, path: String, chunks: Vec<String>) {
    // will try to upload list of chunks and get a new jid for path
    // it can get two options in response:
    //   - list of chunks that's missing
    //   - success message with new jid
    // can also fail
}

fn store_batch(chunks: Vec<String>, content: Vec<String>) {
    // upload content of chunks to remote
    // one option in response:
    //   - success
    // can also fail
}

fn is_synced(namespace: i64, path: String, jid: i64) {
    // query latest remote jid and compare with local one
    // two options can be in response:
    //   - we're in sync
    //   - we're behind
    // can also fail
}

//
// download changes logic
//

// should be called on cron or on long-living request
fn sync_with_remote() {
    // let jid = localDB.latestJid
    // let updates = list(jid)

    // let chunks_to_retrieve: Vec<String>
    // updates.each {
    //     update.each_chunk {
    //         chunks_to_retrieve << chunk unless chunker.contains
    //     }

    // }

    // chunks_to_retrieve.each {
    //     retrieve_batch
    // }

    // updates.each. {
    //     store to db filepath + jid
    // }
}

fn list(namespace: i64, jid: i64) {
    // will return list of all changes since jid
    // one response:
    //   - an array of changes for each file [(ns_id: 1, jid: 15, "/breakfast/Burrito.cook", "chunk1,chunk2")] (only latest for a specific file)
}

fn retrieve_batch(chunks: Vec<String>, content: Vec<String>) {
    // download content of chunks from remote
    // one option in response:
    //   - success
    // can also fail
}

