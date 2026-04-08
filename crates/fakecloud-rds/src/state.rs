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

#[cfg(test)]
mod tests {
    use super::{DbInstance, RdsState};

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
}
