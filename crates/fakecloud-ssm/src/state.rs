use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use std::collections::{BTreeMap, HashMap};
use std::sync::Arc;

#[derive(Debug, Clone)]
pub struct SsmParameter {
    pub name: String,
    pub value: String,
    pub param_type: String, // String, StringList, SecureString
    pub version: i64,
    pub arn: String,
    pub last_modified: DateTime<Utc>,
    pub history: Vec<SsmParameterVersion>,
    pub tags: HashMap<String, String>,
    pub labels: HashMap<i64, Vec<String>>, // version -> labels
    pub description: Option<String>,
    pub allowed_pattern: Option<String>,
    pub key_id: Option<String>,
    pub data_type: String, // "text" or "aws:ec2:image"
    pub tier: String,      // "Standard", "Advanced", "Intelligent-Tiering"
    pub policies: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SsmParameterVersion {
    pub value: String,
    pub version: i64,
    pub last_modified: DateTime<Utc>,
    pub param_type: String,
    pub description: Option<String>,
    pub key_id: Option<String>,
    pub labels: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SsmDocument {
    pub name: String,
    pub content: String,
    pub document_type: String,
    pub document_format: String,
    pub target_type: Option<String>,
    pub version_name: Option<String>,
    pub tags: HashMap<String, String>,
    pub versions: Vec<SsmDocumentVersion>,
    pub default_version: String,
    pub latest_version: String,
    pub created_date: DateTime<Utc>,
    pub owner: String,
    pub status: String,
    pub permissions: HashMap<String, Vec<String>>, // permission_type -> account_ids
}

#[derive(Debug, Clone)]
pub struct SsmDocumentVersion {
    pub content: String,
    pub document_version: String,
    pub version_name: Option<String>,
    pub created_date: DateTime<Utc>,
    pub status: String,
    pub document_format: String,
    pub is_default_version: bool,
}

#[derive(Debug, Clone)]
pub struct SsmCommand {
    pub command_id: String,
    pub document_name: String,
    pub instance_ids: Vec<String>,
    pub parameters: HashMap<String, Vec<String>>,
    pub status: String,
    pub requested_date_time: DateTime<Utc>,
    pub comment: Option<String>,
    pub output_s3_bucket_name: Option<String>,
    pub output_s3_key_prefix: Option<String>,
    pub timeout_seconds: Option<i64>,
    pub service_role_arn: Option<String>,
    pub notification_config: Option<serde_json::Value>,
    pub targets: Vec<serde_json::Value>,
}

#[derive(Debug, Clone)]
pub struct MaintenanceWindowTarget {
    pub window_target_id: String,
    pub window_id: String,
    pub resource_type: String,
    pub targets: Vec<serde_json::Value>,
    pub name: Option<String>,
    pub description: Option<String>,
    pub owner_information: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MaintenanceWindowTask {
    pub window_task_id: String,
    pub window_id: String,
    pub task_arn: String,
    pub task_type: String,
    pub targets: Vec<serde_json::Value>,
    pub max_concurrency: Option<String>,
    pub max_errors: Option<String>,
    pub priority: i64,
    pub service_role_arn: Option<String>,
    pub name: Option<String>,
    pub description: Option<String>,
}

#[derive(Debug, Clone)]
pub struct MaintenanceWindow {
    pub id: String,
    pub name: String,
    pub schedule: String,
    pub duration: i64,
    pub cutoff: i64,
    pub allow_unassociated_targets: bool,
    pub enabled: bool,
    pub description: Option<String>,
    pub tags: HashMap<String, String>,
    pub targets: Vec<MaintenanceWindowTarget>,
    pub tasks: Vec<MaintenanceWindowTask>,
    pub schedule_timezone: Option<String>,
    pub schedule_offset: Option<i64>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
}

#[derive(Debug, Clone)]
pub struct PatchBaseline {
    pub id: String,
    pub name: String,
    pub operating_system: String,
    pub description: Option<String>,
    pub approval_rules: Option<serde_json::Value>,
    pub approved_patches: Vec<String>,
    pub rejected_patches: Vec<String>,
    pub tags: HashMap<String, String>,
    pub approved_patches_compliance_level: String,
    pub rejected_patches_action: String,
    pub global_filters: Option<serde_json::Value>,
    pub sources: Vec<serde_json::Value>,
    pub approved_patches_enable_non_security: bool,
}

#[derive(Debug, Clone)]
pub struct PatchGroup {
    pub baseline_id: String,
    pub patch_group: String,
}

pub struct SsmState {
    pub account_id: String,
    pub region: String,
    pub parameters: BTreeMap<String, SsmParameter>, // name -> param (BTreeMap for path queries)
    pub documents: BTreeMap<String, SsmDocument>,
    pub commands: Vec<SsmCommand>,
    pub maintenance_windows: HashMap<String, MaintenanceWindow>,
    pub patch_baselines: HashMap<String, PatchBaseline>,
    pub patch_groups: Vec<PatchGroup>,
}

impl SsmState {
    pub fn new(account_id: &str, region: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            region: region.to_string(),
            parameters: BTreeMap::new(),
            documents: BTreeMap::new(),
            commands: Vec::new(),
            maintenance_windows: HashMap::new(),
            patch_baselines: HashMap::new(),
            patch_groups: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.parameters.clear();
        self.documents.clear();
        self.commands.clear();
        self.maintenance_windows.clear();
        self.patch_baselines.clear();
        self.patch_groups.clear();
    }
}

pub type SharedSsmState = Arc<RwLock<SsmState>>;
