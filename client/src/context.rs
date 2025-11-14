use std::sync::Arc;
use tokio_util::sync::CancellationToken;

use crate::models::SyncStatus;

/// Trait for receiving sync status updates
/// Implementations of this trait in foreign languages (Swift, etc.) will receive
/// real-time status updates during sync operations
#[uniffi::export(with_foreign)]
pub trait SyncStatusListener: Send + Sync {
    fn on_status_changed(&self, status: SyncStatus);
    fn on_complete(&self, success: bool, message: Option<String>);
}

/// Context for managing sync lifecycle, cancellation, and status updates
#[derive(uniffi::Object)]
pub struct SyncContext {
    cancellation_token: CancellationToken,
    status_listener: std::sync::Mutex<Option<Arc<dyn SyncStatusListener>>>,
}

#[uniffi::export]
impl SyncContext {
    /// Creates a new sync context
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            cancellation_token: CancellationToken::new(),
            status_listener: std::sync::Mutex::new(None),
        })
    }

    /// Sets the status listener for this context
    pub fn set_listener(&self, listener: Arc<dyn SyncStatusListener>) {
        // Handle poisoned mutex by recovering the guard
        let mut listener_lock = self
            .status_listener
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *listener_lock = Some(listener);
    }

    /// Cancels the sync operation
    pub fn cancel(&self) {
        self.cancellation_token.cancel();
    }
}

impl SyncContext {
    /// Notifies the status listener if one is set (internal use only)
    pub fn notify_status(&self, status: SyncStatus) {
        // Handle poisoned mutex by recovering the guard
        let listener_lock = self
            .status_listener
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        if let Some(listener) = listener_lock.as_ref() {
            listener.on_status_changed(status);
        }
    }

    /// Returns a child token for passing to async tasks (internal use only)
    ///
    /// This uses `child_token()` rather than `clone()` to enable hierarchical cancellation:
    /// - When the parent context is cancelled, all child tokens are automatically cancelled
    /// - This allows the sync operation to spawn multiple tasks that all respect cancellation
    /// - Cancelling a child token does NOT cancel the parent or sibling tasks
    ///
    /// This pattern is recommended by tokio-util for managing cancellation across task hierarchies.
    pub fn token(&self) -> CancellationToken {
        self.cancellation_token.child_token()
    }

    /// Returns a clone of the status listener (internal use only)
    pub fn listener(&self) -> Option<Arc<dyn SyncStatusListener>> {
        // Handle poisoned mutex by recovering the guard
        let listener_lock = self
            .status_listener
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        listener_lock.clone()
    }
}
