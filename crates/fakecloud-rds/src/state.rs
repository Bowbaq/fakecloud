use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

pub type SharedRdsState = Arc<RwLock<RdsState>>;

#[derive(Debug, Clone)]
pub struct DbInstance {
    pub db_instance_identifier: String,
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
