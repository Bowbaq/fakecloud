use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StackResource {
    pub logical_id: String,
    pub physical_id: String,
    pub resource_type: String,
    pub status: String,
    /// For custom resources, the Lambda ARN (ServiceToken) used for invocation.
    pub service_token: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Stack {
    pub name: String,
    pub stack_id: String,
    pub template: String,
    pub status: String,
    pub resources: Vec<StackResource>,
    pub parameters: HashMap<String, String>,
    pub tags: HashMap<String, String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: Option<DateTime<Utc>>,
    pub description: Option<String>,
    pub notification_arns: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudFormationState {
    pub account_id: String,
    pub region: String,
    #[serde(default)]
    pub stacks: HashMap<String, Stack>,
}

impl CloudFormationState {
    pub fn new(account_id: &str, region: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            region: region.to_string(),
            stacks: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        self.stacks.clear();
    }
}

pub type SharedCloudFormationState = Arc<RwLock<CloudFormationState>>;

pub const CLOUDFORMATION_SNAPSHOT_SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
pub struct CloudFormationSnapshot {
    pub schema_version: u32,
    pub state: CloudFormationState,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_initializes_empty() {
        let state = CloudFormationState::new("123456789012", "us-east-1");
        assert_eq!(state.account_id, "123456789012");
        assert_eq!(state.region, "us-east-1");
        assert!(state.stacks.is_empty());
    }

    #[test]
    fn reset_clears_stacks() {
        let mut state = CloudFormationState::new("123456789012", "us-east-1");
        state.stacks.insert(
            "s1".to_string(),
            Stack {
                name: "s1".to_string(),
                stack_id: "id".to_string(),
                template: "{}".to_string(),
                status: "CREATE_COMPLETE".to_string(),
                resources: vec![],
                parameters: HashMap::new(),
                tags: HashMap::new(),
                created_at: Utc::now(),
                updated_at: None,
                description: None,
                notification_arns: vec![],
            },
        );
        state.reset();
        assert!(state.stacks.is_empty());
    }
}
