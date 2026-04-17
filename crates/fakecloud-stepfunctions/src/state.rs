use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use serde_json::Value;

pub type SharedStepFunctionsState =
    Arc<RwLock<fakecloud_core::multi_account::MultiAccountState<StepFunctionsState>>>;

impl fakecloud_core::multi_account::AccountState for StepFunctionsState {
    fn new_for_account(account_id: &str, region: &str, _endpoint: &str) -> Self {
        Self::new(account_id, region)
    }
}

pub const STEPFUNCTIONS_SNAPSHOT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
pub struct StepFunctionsSnapshot {
    pub schema_version: u32,
    #[serde(default)]
    pub accounts: Option<fakecloud_core::multi_account::MultiAccountState<StepFunctionsState>>,
    #[serde(default)]
    pub state: Option<StepFunctionsState>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepFunctionsState {
    pub account_id: String,
    pub region: String,
    /// State machines keyed by ARN.
    #[serde(default)]
    pub state_machines: HashMap<String, StateMachine>,
    /// Executions keyed by execution ARN.
    #[serde(default)]
    pub executions: HashMap<String, Execution>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StateMachine {
    pub name: String,
    pub arn: String,
    pub definition: String,
    pub role_arn: String,
    pub machine_type: StateMachineType,
    pub status: StateMachineStatus,
    pub creation_date: DateTime<Utc>,
    pub update_date: DateTime<Utc>,
    pub tags: HashMap<String, String>,
    pub revision_id: String,
    pub logging_configuration: Option<Value>,
    pub tracing_configuration: Option<Value>,
    pub description: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateMachineType {
    Standard,
    Express,
}

impl StateMachineType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Standard => "STANDARD",
            Self::Express => "EXPRESS",
        }
    }

    pub fn parse(s: &str) -> Option<Self> {
        match s {
            "STANDARD" => Some(Self::Standard),
            "EXPRESS" => Some(Self::Express),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StateMachineStatus {
    Active,
    Deleting,
}

impl StateMachineStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Deleting => "DELETING",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Execution {
    pub execution_arn: String,
    pub state_machine_arn: String,
    pub state_machine_name: String,
    pub name: String,
    pub status: ExecutionStatus,
    pub input: Option<String>,
    pub output: Option<String>,
    pub start_date: DateTime<Utc>,
    pub stop_date: Option<DateTime<Utc>>,
    pub error: Option<String>,
    pub cause: Option<String>,
    pub history_events: Vec<HistoryEvent>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExecutionStatus {
    Running,
    Succeeded,
    Failed,
    TimedOut,
    Aborted,
    PendingRedrive,
}

impl ExecutionStatus {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Running => "RUNNING",
            Self::Succeeded => "SUCCEEDED",
            Self::Failed => "FAILED",
            Self::TimedOut => "TIMED_OUT",
            Self::Aborted => "ABORTED",
            Self::PendingRedrive => "PENDING_REDRIVE",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HistoryEvent {
    pub id: i64,
    pub event_type: String,
    pub timestamp: DateTime<Utc>,
    pub previous_event_id: i64,
    pub details: Value,
}

impl StepFunctionsState {
    pub fn new(account_id: &str, region: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            region: region.to_string(),
            state_machines: HashMap::new(),
            executions: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        self.state_machines.clear();
        self.executions.clear();
    }

    pub fn state_machine_arn(&self, name: &str) -> String {
        format!(
            "arn:aws:states:{}:{}:stateMachine:{}",
            self.region, self.account_id, name
        )
    }

    pub fn execution_arn(&self, state_machine_name: &str, execution_name: &str) -> String {
        format!(
            "arn:aws:states:{}:{}:execution:{}:{}",
            self.region, self.account_id, state_machine_name, execution_name
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn state_machine_type_as_str() {
        assert_eq!(StateMachineType::Standard.as_str(), "STANDARD");
        assert_eq!(StateMachineType::Express.as_str(), "EXPRESS");
    }

    #[test]
    fn state_machine_type_parse() {
        assert_eq!(
            StateMachineType::parse("STANDARD"),
            Some(StateMachineType::Standard)
        );
        assert_eq!(
            StateMachineType::parse("EXPRESS"),
            Some(StateMachineType::Express)
        );
        assert_eq!(StateMachineType::parse("bogus"), None);
    }

    #[test]
    fn state_machine_status_as_str() {
        assert_eq!(StateMachineStatus::Active.as_str(), "ACTIVE");
        assert_eq!(StateMachineStatus::Deleting.as_str(), "DELETING");
    }

    #[test]
    fn execution_status_as_str() {
        assert_eq!(ExecutionStatus::Running.as_str(), "RUNNING");
        assert_eq!(ExecutionStatus::Succeeded.as_str(), "SUCCEEDED");
        assert_eq!(ExecutionStatus::Failed.as_str(), "FAILED");
        assert_eq!(ExecutionStatus::TimedOut.as_str(), "TIMED_OUT");
        assert_eq!(ExecutionStatus::Aborted.as_str(), "ABORTED");
        assert_eq!(ExecutionStatus::PendingRedrive.as_str(), "PENDING_REDRIVE");
    }

    #[test]
    fn state_machine_arn_format() {
        let state = StepFunctionsState::new("123456789012", "us-east-1");
        assert_eq!(
            state.state_machine_arn("my-sm"),
            "arn:aws:states:us-east-1:123456789012:stateMachine:my-sm"
        );
    }

    #[test]
    fn execution_arn_format() {
        let state = StepFunctionsState::new("123456789012", "us-east-1");
        assert_eq!(
            state.execution_arn("sm", "exec-1"),
            "arn:aws:states:us-east-1:123456789012:execution:sm:exec-1"
        );
    }

    #[test]
    fn state_reset_clears_all() {
        let mut state = StepFunctionsState::new("123456789012", "us-east-1");
        state.state_machines.insert(
            "x".to_string(),
            StateMachine {
                name: "sm".to_string(),
                arn: "arn:aws:states:us-east-1:123:stateMachine:sm".to_string(),
                definition: "{}".to_string(),
                role_arn: "r".to_string(),
                machine_type: StateMachineType::Standard,
                status: StateMachineStatus::Active,
                creation_date: Utc::now(),
                update_date: Utc::now(),
                tags: HashMap::new(),
                revision_id: "v1".to_string(),
                logging_configuration: None,
                tracing_configuration: None,
                description: String::new(),
            },
        );
        state.reset();
        assert!(state.state_machines.is_empty());
        assert!(state.executions.is_empty());
    }
}
