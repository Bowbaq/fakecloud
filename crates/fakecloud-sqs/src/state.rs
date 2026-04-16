use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageAttribute {
    pub data_type: String,
    pub string_value: Option<String>,
    pub binary_value: Option<Vec<u8>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqsMessage {
    pub message_id: String,
    pub receipt_handle: Option<String>,
    pub body: String,
    pub md5_of_body: String,
    pub sent_timestamp: i64,
    pub attributes: HashMap<String, String>,
    pub message_attributes: HashMap<String, MessageAttribute>,
    /// When this message becomes visible again (after ReceiveMessage)
    pub visible_at: Option<DateTime<Utc>>,
    pub receive_count: u32,
    /// For FIFO: message group ID
    pub message_group_id: Option<String>,
    /// For FIFO: dedup ID
    pub message_dedup_id: Option<String>,
    /// When the message was created (for retention period expiry)
    pub created_at: DateTime<Utc>,
    /// FIFO sequence number
    pub sequence_number: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RedrivePolicy {
    pub dead_letter_target_arn: String,
    pub max_receive_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqsQueue {
    pub queue_name: String,
    pub queue_url: String,
    pub arn: String,
    pub created_at: DateTime<Utc>,
    pub messages: VecDeque<SqsMessage>,
    pub inflight: Vec<SqsMessage>,
    pub attributes: HashMap<String, String>,
    pub is_fifo: bool,
    /// For FIFO dedup: dedup_id -> expiry
    pub dedup_cache: HashMap<String, DateTime<Utc>>,
    /// DLQ redrive policy
    pub redrive_policy: Option<RedrivePolicy>,
    /// Queue tags (key -> value)
    pub tags: HashMap<String, String>,
    /// FIFO: next sequence number counter
    pub next_sequence_number: u64,
    /// Permission labels stored on the queue
    pub permission_labels: Vec<String>,
    /// Tracks message_id -> list of all receipt handles ever issued for that message
    pub receipt_handle_map: HashMap<String, Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqsState {
    pub account_id: String,
    pub region: String,
    pub endpoint: String,
    pub queues: HashMap<String, SqsQueue>, // queue_url -> queue
    pub name_to_url: HashMap<String, String>, // queue_name -> queue_url
}

impl SqsState {
    pub fn new(account_id: &str, region: &str, endpoint: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            region: region.to_string(),
            endpoint: endpoint.to_string(),
            queues: HashMap::new(),
            name_to_url: HashMap::new(),
        }
    }
}

impl SqsState {
    pub fn reset(&mut self) {
        self.queues.clear();
        self.name_to_url.clear();
    }
}

pub type SharedSqsState = Arc<RwLock<fakecloud_core::multi_account::MultiAccountState<SqsState>>>;

/// On-disk snapshot envelope for SQS. Mirrors the DynamoDB pattern: a
/// versioned wrapper around the full [`SqsState`] so format changes fail
/// loudly on upgrade instead of silently corrupting state.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SqsSnapshot {
    pub schema_version: u32,
    #[serde(default)]
    pub accounts: Option<fakecloud_core::multi_account::MultiAccountState<SqsState>>,
    #[serde(default)]
    pub state: Option<SqsState>,
}

pub const SQS_SNAPSHOT_SCHEMA_VERSION: u32 = 2;

impl fakecloud_core::multi_account::AccountState for SqsState {
    fn new_for_account(account_id: &str, region: &str, endpoint: &str) -> Self {
        Self::new(account_id, region, endpoint)
    }
}
