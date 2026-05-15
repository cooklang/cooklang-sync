//! Integration tests for `async_watcher`.
//!
//! The debouncer is configured with a 2-second flush window in production,
//! so these tests wait up to 6 seconds for events. Slow, but correct.

mod common;

use cooklang_sync_client::file_watcher::async_watcher;
use futures::StreamExt;
use notify::RecursiveMode;
use std::time::Duration;
use tempfile::TempDir;
use tokio::time::timeout;

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn async_watcher_reports_file_creation() {
    let dir = TempDir::new().unwrap();
    // On macOS, TempDir paths are symlinks under /var; canonicalize so the
    // expected path matches what the OS watcher reports.
    let dir_real = dir.path().canonicalize().unwrap();
    let (mut debouncer, mut rx) = async_watcher().expect("build debouncer");

    debouncer
        .watcher()
        .watch(&dir_real, RecursiveMode::Recursive)
        .expect("watch tempdir");

    // Create a file after a short delay so the watcher is definitely armed.
    let path = dir_real.join("hello.cook");
    tokio::spawn({
        let path = path.clone();
        async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            tokio::fs::write(&path, b"content").await.expect("write");
        }
    });

    // Debouncer fires after DEBOUNCE_SEC (2s); allow up to 6s.
    let result = timeout(Duration::from_secs(6), rx.next())
        .await
        .expect("watcher event should arrive before timeout");

    let events = result
        .expect("channel should deliver at least one event")
        .expect("watcher result should not be Err");
    // On macOS with FSEvents the debouncer may report the parent directory
    // rather than the individual file; accept either the exact file path or
    // any ancestor directory that contains it.
    assert!(
        events.iter().any(|e| e.path == path || path.starts_with(&e.path)),
        "event list should include the created file or its parent dir; got: {:?}",
        events
    );
}
