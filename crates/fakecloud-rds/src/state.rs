use std::collections::HashMap;
use std::sync::Arc;

use chrono::{DateTime, Utc};
use fakecloud_aws::arn::Arn;
use parking_lot::RwLock;
use uuid::Uuid;

pub type SharedRdsState = Arc<RwLock<RdsState>>;

#[derive(Debug, Clone)]
pub struct DbInstance {
    pub db_instance_identifier: String,
    pub db_instance_arn: String,
    pub db_instance_class: String,
    pub engine: String,
    pub engine_version: String,
    pub db_instance_status: String,
    pub master_username: String,
    pub db_name: Option<String>,
    pub endpoint_address: String,
    pub port: i32,
    pub allocated_storage: i32,
    pub publicly_accessible: bool,
    pub deletion_protection: bool,
    pub created_at: DateTime<Utc>,
    pub dbi_resource_id: String,
    pub master_user_password: String,
    pub container_id: String,
    pub host_port: u16,
}

#[derive(Debug)]
pub struct RdsState {
    pub account_id: String,
    pub region: String,
    pub instances: HashMap<String, DbInstance>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineVersionInfo {
    pub engine: String,
    pub engine_version: String,
    pub db_parameter_group_family: String,
    pub db_engine_description: String,
    pub db_engine_version_description: String,
    pub status: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrderableDbInstanceOption {
    pub engine: String,
    pub engine_version: String,
    pub db_instance_class: String,
    pub license_model: String,
    pub storage_type: String,
    pub min_storage_size: i32,
    pub max_storage_size: i32,
}

impl RdsState {
    pub fn new(account_id: &str, region: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            region: region.to_string(),
            instances: HashMap::new(),
        }
    }

    pub fn reset(&mut self) {
        self.instances.clear();
    }

    pub fn db_instance_arn(&self, db_instance_identifier: &str) -> String {
        Arn::new(
            "rds",
            &self.region,
            &self.account_id,
            &format!("db:{db_instance_identifier}"),
        )
        .to_string()
    }

    pub fn next_dbi_resource_id(&self) -> String {
        format!("db-{}", Uuid::new_v4().simple())
    }
}

pub fn default_engine_versions() -> Vec<EngineVersionInfo> {
    vec![EngineVersionInfo {
        engine: "postgres".to_string(),
        engine_version: "16.3".to_string(),
        db_parameter_group_family: "postgres16".to_string(),
        db_engine_description: "PostgreSQL".to_string(),
        db_engine_version_description: "PostgreSQL 16.3".to_string(),
        status: "available".to_string(),
    }]
}

pub fn default_orderable_options() -> Vec<OrderableDbInstanceOption> {
    vec![OrderableDbInstanceOption {
        engine: "postgres".to_string(),
        engine_version: "16.3".to_string(),
        db_instance_class: "db.t3.micro".to_string(),
        license_model: "postgresql-license".to_string(),
        storage_type: "gp2".to_string(),
        min_storage_size: 20,
        max_storage_size: 16384,
    }]
}

#[cfg(test)]
mod tests {
    use chrono::Utc;

    use super::{default_engine_versions, default_orderable_options, DbInstance, RdsState};

    #[test]
    fn new_initializes_account_and_region() {
        let state = RdsState::new("123456789012", "us-east-1");

        assert_eq!(state.account_id, "123456789012");
        assert_eq!(state.region, "us-east-1");
        assert!(state.instances.is_empty());
    }

    #[test]
    fn reset_clears_instances() {
        let mut state = RdsState::new("123456789012", "us-east-1");
        state.instances.insert(
            "db-1".to_string(),
            DbInstance {
                db_instance_identifier: "db-1".to_string(),
                db_instance_arn: "arn:aws:rds:us-east-1:123456789012:db:db-1".to_string(),
                db_instance_class: "db.t3.micro".to_string(),
                engine: "postgres".to_string(),
                engine_version: "16.3".to_string(),
                db_instance_status: "available".to_string(),
                master_username: "admin".to_string(),
                db_name: Some("postgres".to_string()),
                endpoint_address: "127.0.0.1".to_string(),
                port: 5432,
                allocated_storage: 20,
                publicly_accessible: true,
                deletion_protection: false,
                created_at: Utc::now(),
                dbi_resource_id: "db-test".to_string(),
                master_user_password: "secret123".to_string(),
                container_id: "container-id".to_string(),
                host_port: 15432,
            },
        );

        state.reset();

        assert!(state.instances.is_empty());
    }

    #[test]
    fn default_engine_versions_are_postgres_metadata() {
        let versions = default_engine_versions();

        assert_eq!(versions.len(), 1);
        assert_eq!(versions[0].engine, "postgres");
        assert_eq!(versions[0].engine_version, "16.3");
        assert_eq!(versions[0].db_parameter_group_family, "postgres16");
    }

    #[test]
    fn default_orderable_options_match_engine_versions() {
        let versions = default_engine_versions();
        let options = default_orderable_options();

        assert_eq!(options.len(), 1);
        assert_eq!(options[0].engine, versions[0].engine);
        assert_eq!(options[0].engine_version, versions[0].engine_version);
        assert_eq!(options[0].db_instance_class, "db.t3.micro");
    }
}
