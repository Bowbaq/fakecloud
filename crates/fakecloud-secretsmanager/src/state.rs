use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct Secret {
    pub name: String,
    pub arn: String,
    pub description: String,
    pub kms_key_id: Option<String>,
    pub versions: HashMap<String, SecretVersion>,
    pub current_version_id: String,
    pub tags: HashMap<String, String>,
    pub deleted: bool,
    pub deletion_date: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub last_changed_at: DateTime<Utc>,
    pub last_accessed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
pub struct SecretVersion {
    pub version_id: String,
    pub secret_string: Option<String>,
    pub secret_binary: Option<Vec<u8>>,
    pub stages: Vec<String>,
    pub created_at: DateTime<Utc>,
}

pub struct SecretsManagerState {
    pub account_id: String,
    pub region: String,
    pub secrets: HashMap<String, Secret>,
}

impl SecretsManagerState {
    pub fn new(account_id: &str, region: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            region: region.to_string(),
            secrets: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        self.secrets.clear();
    }
}

pub type SharedSecretsManagerState = Arc<RwLock<SecretsManagerState>>;
