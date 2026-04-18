//! Snapshot load helper for the scheduler state.
//!
//! Keeps the server's `main.rs` wiring block thin (single fn call)
//! while the interesting branches — schema-version gate, migration-
//! point, empty-startup — get unit-tested here.

use fakecloud_persistence::SnapshotStore;

use crate::state::{SchedulerSnapshot, SharedSchedulerState, SCHEDULER_SNAPSHOT_SCHEMA_VERSION};

#[derive(Debug, PartialEq, Eq)]
pub enum LoadOutcome {
    /// No snapshot file on disk; start with fresh state.
    Empty,
    /// Snapshot loaded successfully; returns the restored account count.
    Loaded(usize),
}

#[derive(Debug, thiserror::Error)]
pub enum LoadError {
    #[error("failed to read scheduler persistence snapshot: {0}")]
    Io(String),
    #[error("failed to parse scheduler persistence snapshot: {0}")]
    Parse(String),
    #[error("scheduler persistence schema too new: on-disk={on_disk}, max supported={supported}")]
    SchemaTooNew { on_disk: u32, supported: u32 },
}

/// Load a snapshot into `state`. Returns `Empty` when the store has
/// nothing saved, `Loaded(n)` after a successful restore, or a
/// descriptive error the server turns into a fatal startup message.
pub fn load_into(
    store: &dyn SnapshotStore,
    state: &SharedSchedulerState,
) -> Result<LoadOutcome, LoadError> {
    let Some(bytes) = store.load().map_err(|e| LoadError::Io(e.to_string()))? else {
        return Ok(LoadOutcome::Empty);
    };
    let snapshot: SchedulerSnapshot =
        serde_json::from_slice(&bytes).map_err(|e| LoadError::Parse(e.to_string()))?;
    if snapshot.schema_version > SCHEDULER_SNAPSHOT_SCHEMA_VERSION {
        return Err(LoadError::SchemaTooNew {
            on_disk: snapshot.schema_version,
            supported: SCHEDULER_SNAPSHOT_SCHEMA_VERSION,
        });
    }
    let accounts = snapshot.accounts.account_count();
    *state.write() = snapshot.accounts;
    Ok(LoadOutcome::Loaded(accounts))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{SchedulerSnapshot, SchedulerState};
    use fakecloud_core::multi_account::MultiAccountState;
    use parking_lot::RwLock;
    use std::sync::Arc;
    use std::sync::Mutex;

    fn make_state() -> SharedSchedulerState {
        Arc::new(RwLock::new(MultiAccountState::new(
            "000000000000",
            "us-east-1",
            "",
        )))
    }

    struct MemStore {
        data: Mutex<Option<Vec<u8>>>,
    }
    impl MemStore {
        fn new(data: Option<Vec<u8>>) -> Self {
            Self {
                data: Mutex::new(data),
            }
        }
    }
    impl SnapshotStore for MemStore {
        fn load(&self) -> std::io::Result<Option<Vec<u8>>> {
            Ok(self.data.lock().unwrap().clone())
        }
        fn save(&self, bytes: &[u8]) -> std::io::Result<()> {
            *self.data.lock().unwrap() = Some(bytes.to_vec());
            Ok(())
        }
    }

    #[test]
    fn load_into_empty_returns_empty() {
        let state = make_state();
        let store = MemStore::new(None);
        let outcome = load_into(&store, &state).unwrap();
        assert_eq!(outcome, LoadOutcome::Empty);
    }

    #[test]
    fn load_into_valid_snapshot_restores_accounts() {
        let state = make_state();
        let mut mas: MultiAccountState<SchedulerState> =
            MultiAccountState::new("999999999999", "us-east-1", "");
        mas.get_or_create("999999999999");
        let snap = SchedulerSnapshot {
            schema_version: SCHEDULER_SNAPSHOT_SCHEMA_VERSION,
            accounts: mas,
        };
        let bytes = serde_json::to_vec(&snap).unwrap();
        let store = MemStore::new(Some(bytes));
        let outcome = load_into(&store, &state).unwrap();
        assert_eq!(outcome, LoadOutcome::Loaded(1));
        let accounts = state.read();
        assert!(accounts.get("999999999999").is_some());
    }

    #[test]
    fn load_into_rejects_future_schema() {
        let state = make_state();
        let mas: MultiAccountState<SchedulerState> =
            MultiAccountState::new("000000000000", "us-east-1", "");
        let snap = SchedulerSnapshot {
            schema_version: SCHEDULER_SNAPSHOT_SCHEMA_VERSION + 1,
            accounts: mas,
        };
        let bytes = serde_json::to_vec(&snap).unwrap();
        let store = MemStore::new(Some(bytes));
        let err = load_into(&store, &state).err().unwrap();
        assert!(matches!(err, LoadError::SchemaTooNew { .. }));
    }

    #[test]
    fn load_into_reports_parse_errors() {
        let state = make_state();
        let store = MemStore::new(Some(b"not json".to_vec()));
        let err = load_into(&store, &state).err().unwrap();
        assert!(matches!(err, LoadError::Parse(_)));
    }
}
