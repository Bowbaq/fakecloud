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
