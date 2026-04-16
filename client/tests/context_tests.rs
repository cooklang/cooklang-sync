//! Integration tests for `SyncContext` lifecycle and status listener.

use cooklang_sync_client::{SyncContext, SyncStatus, SyncStatusListener};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct RecordingListener {
    statuses: Mutex<Vec<String>>, // Stored as strings because SyncStatus is not PartialEq.
    completions: Mutex<Vec<(bool, Option<String>)>>,
}

impl RecordingListener {
    fn statuses(&self) -> Vec<String> {
        self.statuses.lock().unwrap().clone()
    }
    fn completions(&self) -> Vec<(bool, Option<String>)> {
        self.completions.lock().unwrap().clone()
    }
}

impl SyncStatusListener for RecordingListener {
    fn on_status_changed(&self, status: SyncStatus) {
        self.statuses
            .lock()
            .unwrap()
            .push(format!("{:?}", status));
    }
    fn on_complete(&self, success: bool, message: Option<String>) {
        self.completions.lock().unwrap().push((success, message));
    }
}

#[test]
fn new_context_starts_with_no_listener() {
    let ctx = SyncContext::new();
    assert!(ctx.listener().is_none());
}

#[test]
fn set_listener_then_listener_returns_same_arc() {
    let ctx = SyncContext::new();
    let listener: Arc<RecordingListener> = Arc::new(RecordingListener::default());
    ctx.set_listener(listener.clone() as Arc<dyn SyncStatusListener>);

    let got = ctx.listener().expect("listener should be set");
    // Trigger a status; the recording listener we handed in should see it.
    got.on_status_changed(SyncStatus::Indexing);
    assert_eq!(listener.statuses(), vec!["Indexing".to_string()]);
}

#[test]
fn notify_status_forwards_to_listener() {
    let ctx = SyncContext::new();
    let listener: Arc<RecordingListener> = Arc::new(RecordingListener::default());
    ctx.set_listener(listener.clone() as Arc<dyn SyncStatusListener>);

    ctx.notify_status(SyncStatus::Syncing);
    ctx.notify_status(SyncStatus::Uploading);
    ctx.notify_status(SyncStatus::Idle);

    assert_eq!(
        listener.statuses(),
        vec!["Syncing".to_string(), "Uploading".to_string(), "Idle".to_string()]
    );
}

#[test]
fn notify_status_without_listener_is_silent() {
    let ctx = SyncContext::new();
    // Should not panic.
    ctx.notify_status(SyncStatus::Idle);
}

#[test]
fn cancel_propagates_to_child_token() {
    let ctx = SyncContext::new();
    let child = ctx.token();
    assert!(!child.is_cancelled(), "precondition: child not cancelled");
    ctx.cancel();
    assert!(child.is_cancelled(), "child should be cancelled after parent.cancel()");
}

#[test]
fn child_tokens_are_independent_of_each_other() {
    let ctx = SyncContext::new();
    let a = ctx.token();
    let b = ctx.token();
    a.cancel();
    assert!(a.is_cancelled());
    assert!(!b.is_cancelled(), "sibling token should not be affected");
}
