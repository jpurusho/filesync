//! Tracks active sync operations and provides cancellation support

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Shared state for tracking active sync operations
#[derive(Clone)]
pub struct SyncTracker {
    inner: Arc<Mutex<HashMap<Uuid, CancellationToken>>>,
}

impl SyncTracker {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Register a new sync run and return a cancellation token
    pub async fn register(&self, run_id: Uuid) -> CancellationToken {
        let token = CancellationToken::new();
        self.inner.lock().await.insert(run_id, token.clone());
        token
    }

    /// Cancel a specific sync run
    pub async fn cancel(&self, run_id: Uuid) -> bool {
        if let Some(token) = self.inner.lock().await.get(&run_id) {
            token.cancel();
            true
        } else {
            false
        }
    }

    /// Unregister a sync run (called on completion or error)
    pub async fn unregister(&self, run_id: Uuid) {
        self.inner.lock().await.remove(&run_id);
    }
}
