//! Integration tests for `cooklang_sync_client::indexer::run` (the async loop).

mod common;

use cooklang_sync_client::connection::get_connection;
use cooklang_sync_client::indexer::run;
use cooklang_sync_client::models::{FileRecord, IndexerUpdateEvent};
use cooklang_sync_client::schema::file_records;
use diesel::prelude::*;
use futures::channel::mpsc;
use futures::{SinkExt, StreamExt};
use std::fs;
use std::time::Duration;
use tokio::time::timeout;
use tokio_util::sync::CancellationToken;

const NS: i32 = 1;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_emits_update_event_on_initial_scan_and_on_subsequent_fs_event() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = tempfile::TempDir::new().unwrap();

    // Seed one file BEFORE the loop starts so the initial scan finds work.
    fs::write(storage.path().join("a.cook"), b"v1").unwrap();

    let (fs_tx, fs_rx) = mpsc::channel::<notify_debouncer_mini::DebounceEventResult>(8);
    let (updated_tx, mut updated_rx) = mpsc::channel::<IndexerUpdateEvent>(8);

    let token = CancellationToken::new();
    let token_for_loop = token.clone();

    let pool_cloned = pool.clone();
    let storage_path = storage.path().to_path_buf();
    let join = tokio::spawn(async move {
        run(
            token_for_loop,
            None, // no listener
            &pool_cloned,
            &storage_path,
            NS,
            fs_rx,
            updated_tx,
        )
        .await
    });

    // 1. Initial scan must emit an update (the seeded file is new).
    let first = timeout(Duration::from_secs(5), updated_rx.next())
        .await
        .expect("initial Updated event within 5s")
        .expect("channel not closed");
    assert!(matches!(first, IndexerUpdateEvent::Updated));

    // 2. Create a new file and push a synthetic FS event so the loop re-scans.
    //    Sleep >1s so modified_at truncated-to-seconds differs from the seed.
    tokio::time::sleep(Duration::from_millis(1100)).await;
    fs::write(storage.path().join("b.cook"), b"new").unwrap();
    fs_tx
        .clone()
        .send(Ok(Vec::new()))
        .await
        .expect("push synthetic debounce event");

    let second = timeout(Duration::from_secs(5), updated_rx.next())
        .await
        .expect("second Updated event within 5s")
        .expect("channel not closed");
    assert!(matches!(second, IndexerUpdateEvent::Updated));

    // 3. Cancel and verify clean exit.
    token.cancel();
    let res = timeout(Duration::from_secs(5), join)
        .await
        .expect("loop must exit within 5s of cancel")
        .expect("task joined");
    res.expect("run returns Ok on cancel");

    // 4. Both files should be in the registry.
    let conn = &mut get_connection(&pool).unwrap();
    let mut paths: Vec<String> = file_records::table
        .filter(file_records::deleted.eq(false))
        .select(FileRecord::as_select())
        .load(conn)
        .unwrap()
        .into_iter()
        .map(|r| r.path)
        .collect();
    paths.sort();
    assert_eq!(paths, vec!["a.cook".to_string(), "b.cook".to_string()]);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn run_exits_cleanly_when_cancelled_immediately() {
    let (pool, _db_dir) = common::fresh_client_pool();
    let storage = tempfile::TempDir::new().unwrap();
    let (_fs_tx, fs_rx) = mpsc::channel::<notify_debouncer_mini::DebounceEventResult>(1);
    let (updated_tx, _updated_rx) = mpsc::channel::<IndexerUpdateEvent>(1);

    let token = CancellationToken::new();
    token.cancel(); // pre-cancelled

    // Nothing to assert beyond "it exited" — no seed files, no events.
    timeout(
        Duration::from_secs(5),
        run(
            token,
            None,
            &pool,
            storage.path(),
            NS,
            fs_rx,
            updated_tx,
        ),
    )
    .await
    .expect("run must exit within 5s when token is already cancelled")
    .expect("run returns Ok");
}
