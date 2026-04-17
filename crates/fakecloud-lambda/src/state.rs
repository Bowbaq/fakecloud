use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaFunction {
    pub function_name: String,
    pub function_arn: String,
    pub runtime: String,
    pub role: String,
    pub handler: String,
    pub description: String,
    pub timeout: i64,
    pub memory_size: i64,
    pub code_sha256: String,
    pub code_size: i64,
    pub version: String,
    pub last_modified: DateTime<Utc>,
    pub tags: HashMap<String, String>,
    pub environment: HashMap<String, String>,
    pub architectures: Vec<String>,
    pub package_type: String,
    pub code_zip: Option<Vec<u8>>,
    /// Resource-based policy attached to this function via
    /// `AddPermission`, serialized as a full JSON policy document
    /// (`{"Version":"2012-10-17","Statement":[...]}`). `None` means
    /// the function has no resource policy attached, matching the
    /// `ResourceNotFoundException` AWS returns from `GetPolicy` in
    /// that state. `AddPermission` lazily initializes this; every
    /// `RemovePermission` leaves at least `{"Statement":[]}` behind,
    /// matching AWS behavior.
    pub policy: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSourceMapping {
    pub uuid: String,
    pub function_arn: String,
    pub event_source_arn: String,
    pub batch_size: i64,
    pub enabled: bool,
    pub state: String,
    pub last_modified: DateTime<Utc>,
}

/// A recorded Lambda invocation from cross-service delivery.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaInvocation {
    pub function_arn: String,
    pub payload: String,
    pub timestamp: DateTime<Utc>,
    pub source: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LambdaState {
    pub account_id: String,
    pub region: String,
    #[serde(default)]
    pub functions: HashMap<String, LambdaFunction>,
    #[serde(default)]
    pub event_source_mappings: HashMap<String, EventSourceMapping>,
    /// Recorded invocations from cross-service integrations — not persisted.
    #[serde(default, skip)]
    pub invocations: Vec<LambdaInvocation>,
}

impl LambdaState {
    pub fn new(account_id: &str, region: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            region: region.to_string(),
            functions: HashMap::new(),
            event_source_mappings: HashMap::new(),
            invocations: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.functions.clear();
        self.event_source_mappings.clear();
        self.invocations.clear();
    }
}

pub type SharedLambdaState =
    Arc<RwLock<fakecloud_core::multi_account::MultiAccountState<LambdaState>>>;

impl fakecloud_core::multi_account::AccountState for LambdaState {
    fn new_for_account(account_id: &str, region: &str, _endpoint: &str) -> Self {
        Self::new(account_id, region)
    }
}

pub const LAMBDA_SNAPSHOT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
pub struct LambdaSnapshot {
    pub schema_version: u32,
    #[serde(default)]
    pub accounts: Option<fakecloud_core::multi_account::MultiAccountState<LambdaState>>,
    #[serde(default)]
    pub state: Option<LambdaState>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_has_empty_collections() {
        let state = LambdaState::new("123456789012", "us-east-1");
        assert_eq!(state.account_id, "123456789012");
        assert_eq!(state.region, "us-east-1");
        assert!(state.functions.is_empty());
        assert!(state.event_source_mappings.is_empty());
        assert!(state.invocations.is_empty());
    }

    #[test]
    fn reset_clears_collections() {
        let mut state = LambdaState::new("123456789012", "us-east-1");
        state.invocations.push(LambdaInvocation {
            function_arn: "arn".to_string(),
            payload: "p".to_string(),
            timestamp: Utc::now(),
            source: "s".to_string(),
        });
        state.reset();
        assert!(state.invocations.is_empty());
    }
}
