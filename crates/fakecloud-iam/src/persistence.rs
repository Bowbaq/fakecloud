//! Shared IAM snapshot persistence.
//!
//! Both `IamService` and `StsService` operate on the same `SharedIamState`
//! and therefore share a single on-disk snapshot. The save routine and the
//! serializing Mutex live here so both services route through the same
//! critical section.

use std::sync::Arc;

use fakecloud_persistence::SnapshotStore;
use tokio::sync::Mutex as AsyncMutex;

use crate::state::{IamSnapshot, SharedIamState, IAM_SNAPSHOT_SCHEMA_VERSION};

/// Serializes concurrent snapshot writes across both IAM and STS services.
/// Without it, two tasks could clone state under the RwLock, serialize
/// independently, and race on `store.save()`, leaving older bytes as the
/// final on-disk state.
pub type IamSnapshotLock = Arc<AsyncMutex<()>>;

pub fn new_snapshot_lock() -> IamSnapshotLock {
    Arc::new(AsyncMutex::new(()))
}

/// Persist the current IAM state as a snapshot. Offloads the serde +
/// blocking file write to the Tokio blocking pool so the async runtime
/// stays responsive.
///
/// Noop when `store` is `None` (memory mode).
pub async fn save_iam_snapshot(
    state: &SharedIamState,
    store: Option<Arc<dyn SnapshotStore>>,
    lock: &IamSnapshotLock,
) {
    let Some(store) = store else {
        return;
    };
    let _guard = lock.lock().await;
    let snapshot = IamSnapshot {
        schema_version: IAM_SNAPSHOT_SCHEMA_VERSION,
        accounts: Some(state.read().clone()),
        state: None,
    };
    let join = tokio::task::spawn_blocking(move || -> std::io::Result<()> {
        let bytes = serde_json::to_vec(&snapshot)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))?;
        store.save(&bytes)
    })
    .await;
    match join {
        Ok(Ok(())) => {}
        Ok(Err(err)) => tracing::error!(%err, "failed to write iam snapshot"),
        Err(err) => tracing::error!(%err, "iam snapshot task panicked"),
    }
}
