use std::collections::HashMap;
use std::sync::Arc;

use parking_lot::RwLock;

pub type SharedElastiCacheState = Arc<RwLock<ElastiCacheState>>;

#[derive(Debug, Clone)]
pub struct CacheEngineVersion {
    pub engine: String,
    pub engine_version: String,
    pub cache_parameter_group_family: String,
    pub cache_engine_description: String,
    pub cache_engine_version_description: String,
}

#[derive(Debug, Clone)]
pub struct CacheParameterGroup {
    pub cache_parameter_group_name: String,
    pub cache_parameter_group_family: String,
    pub description: String,
    pub is_global: bool,
    pub arn: String,
}

#[derive(Debug, Clone)]
pub struct EngineDefaultParameter {
    pub parameter_name: String,
    pub parameter_value: String,
    pub description: String,
    pub source: String,
    pub data_type: String,
    pub allowed_values: String,
    pub is_modifiable: bool,
    pub minimum_engine_version: String,
}

#[derive(Debug, Clone)]
pub struct CacheSubnetGroup {
    pub cache_subnet_group_name: String,
    pub cache_subnet_group_description: String,
    pub vpc_id: String,
    pub subnet_ids: Vec<String>,
    pub arn: String,
}

#[derive(Debug)]
pub struct ElastiCacheState {
    pub account_id: String,
    pub region: String,
    pub parameter_groups: Vec<CacheParameterGroup>,
    pub subnet_groups: HashMap<String, CacheSubnetGroup>,
}

impl ElastiCacheState {
    pub fn new(account_id: &str, region: &str) -> Self {
        let parameter_groups = default_parameter_groups(account_id, region);
        let subnet_groups = default_subnet_groups(account_id, region);
        Self {
            account_id: account_id.to_string(),
            region: region.to_string(),
            parameter_groups,
            subnet_groups,
        }
    }

    pub fn reset(&mut self) {
        self.parameter_groups = default_parameter_groups(&self.account_id, &self.region);
        self.subnet_groups = default_subnet_groups(&self.account_id, &self.region);
    }
}

pub fn default_engine_versions() -> Vec<CacheEngineVersion> {
    vec![
        CacheEngineVersion {
            engine: "redis".to_string(),
            engine_version: "7.1".to_string(),
            cache_parameter_group_family: "redis7".to_string(),
            cache_engine_description: "Redis".to_string(),
            cache_engine_version_description: "Redis 7.1".to_string(),
        },
        CacheEngineVersion {
            engine: "valkey".to_string(),
            engine_version: "8.0".to_string(),
            cache_parameter_group_family: "valkey8".to_string(),
            cache_engine_description: "Valkey".to_string(),
            cache_engine_version_description: "Valkey 8.0".to_string(),
        },
    ]
}

fn default_parameter_groups(account_id: &str, region: &str) -> Vec<CacheParameterGroup> {
    vec![
        CacheParameterGroup {
            cache_parameter_group_name: "default.redis7".to_string(),
            cache_parameter_group_family: "redis7".to_string(),
            description: "Default parameter group for redis7".to_string(),
            is_global: false,
            arn: format!("arn:aws:elasticache:{region}:{account_id}:parametergroup:default.redis7"),
        },
        CacheParameterGroup {
            cache_parameter_group_name: "default.valkey8".to_string(),
            cache_parameter_group_family: "valkey8".to_string(),
            description: "Default parameter group for valkey8".to_string(),
            is_global: false,
            arn: format!(
                "arn:aws:elasticache:{region}:{account_id}:parametergroup:default.valkey8"
            ),
        },
    ]
}

fn default_subnet_groups(account_id: &str, region: &str) -> HashMap<String, CacheSubnetGroup> {
    let default_group = CacheSubnetGroup {
        cache_subnet_group_name: "default".to_string(),
        cache_subnet_group_description: "Default CacheSubnetGroup".to_string(),
        vpc_id: "vpc-00000000".to_string(),
        subnet_ids: vec!["subnet-00000000".to_string()],
        arn: format!("arn:aws:elasticache:{region}:{account_id}:subnetgroup:default"),
    };
    let mut map = HashMap::new();
    map.insert("default".to_string(), default_group);
    map
}

pub fn default_parameters_for_family(family: &str) -> Vec<EngineDefaultParameter> {
    match family {
        "redis7" => vec![
            EngineDefaultParameter {
                parameter_name: "maxmemory-policy".to_string(),
                parameter_value: "volatile-lru".to_string(),
                description: "Max memory policy".to_string(),
                source: "system".to_string(),
                data_type: "string".to_string(),
                allowed_values: "volatile-lru,allkeys-lru,volatile-lfu,allkeys-lfu,volatile-random,allkeys-random,volatile-ttl,noeviction".to_string(),
                is_modifiable: true,
                minimum_engine_version: "7.0.0".to_string(),
            },
            EngineDefaultParameter {
                parameter_name: "cluster-enabled".to_string(),
                parameter_value: "no".to_string(),
                description: "Enable or disable Redis Cluster mode".to_string(),
                source: "system".to_string(),
                data_type: "string".to_string(),
                allowed_values: "yes,no".to_string(),
                is_modifiable: false,
                minimum_engine_version: "7.0.0".to_string(),
            },
            EngineDefaultParameter {
                parameter_name: "activedefrag".to_string(),
                parameter_value: "no".to_string(),
                description: "Enable active defragmentation".to_string(),
                source: "system".to_string(),
                data_type: "string".to_string(),
                allowed_values: "yes,no".to_string(),
                is_modifiable: true,
                minimum_engine_version: "7.0.0".to_string(),
            },
        ],
        "valkey8" => vec![
            EngineDefaultParameter {
                parameter_name: "maxmemory-policy".to_string(),
                parameter_value: "volatile-lru".to_string(),
                description: "Max memory policy".to_string(),
                source: "system".to_string(),
                data_type: "string".to_string(),
                allowed_values: "volatile-lru,allkeys-lru,volatile-lfu,allkeys-lfu,volatile-random,allkeys-random,volatile-ttl,noeviction".to_string(),
                is_modifiable: true,
                minimum_engine_version: "8.0.0".to_string(),
            },
            EngineDefaultParameter {
                parameter_name: "cluster-enabled".to_string(),
                parameter_value: "no".to_string(),
                description: "Enable or disable cluster mode".to_string(),
                source: "system".to_string(),
                data_type: "string".to_string(),
                allowed_values: "yes,no".to_string(),
                is_modifiable: false,
                minimum_engine_version: "8.0.0".to_string(),
            },
            EngineDefaultParameter {
                parameter_name: "activedefrag".to_string(),
                parameter_value: "no".to_string(),
                description: "Enable active defragmentation".to_string(),
                source: "system".to_string(),
                data_type: "string".to_string(),
                allowed_values: "yes,no".to_string(),
                is_modifiable: true,
                minimum_engine_version: "8.0.0".to_string(),
            },
        ],
        _ => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_engine_versions_contains_redis_and_valkey() {
        let versions = default_engine_versions();
        assert_eq!(versions.len(), 2);
        assert_eq!(versions[0].engine, "redis");
        assert_eq!(versions[0].engine_version, "7.1");
        assert_eq!(versions[1].engine, "valkey");
        assert_eq!(versions[1].engine_version, "8.0");
    }

    #[test]
    fn state_new_creates_default_parameter_groups() {
        let state = ElastiCacheState::new("123456789012", "us-east-1");
        assert_eq!(state.parameter_groups.len(), 2);
        assert_eq!(
            state.parameter_groups[0].cache_parameter_group_name,
            "default.redis7"
        );
        assert_eq!(
            state.parameter_groups[1].cache_parameter_group_name,
            "default.valkey8"
        );
    }

    #[test]
    fn state_new_creates_default_subnet_group() {
        let state = ElastiCacheState::new("123456789012", "us-east-1");
        assert_eq!(state.subnet_groups.len(), 1);
        let default = state.subnet_groups.get("default").unwrap();
        assert_eq!(default.cache_subnet_group_name, "default");
        assert_eq!(
            default.cache_subnet_group_description,
            "Default CacheSubnetGroup"
        );
        assert_eq!(default.vpc_id, "vpc-00000000");
        assert!(!default.subnet_ids.is_empty());
        assert!(default.arn.contains("subnetgroup:default"));
    }

    #[test]
    fn reset_restores_default_parameter_groups() {
        let mut state = ElastiCacheState::new("123456789012", "us-east-1");
        state.parameter_groups.clear();
        assert!(state.parameter_groups.is_empty());
        state.reset();
        assert_eq!(state.parameter_groups.len(), 2);
    }

    #[test]
    fn reset_restores_default_subnet_groups() {
        let mut state = ElastiCacheState::new("123456789012", "us-east-1");
        state.subnet_groups.clear();
        assert!(state.subnet_groups.is_empty());
        state.reset();
        assert_eq!(state.subnet_groups.len(), 1);
        assert!(state.subnet_groups.contains_key("default"));
    }

    #[test]
    fn default_parameters_for_redis7_returns_parameters() {
        let params = default_parameters_for_family("redis7");
        assert_eq!(params.len(), 3);
        assert_eq!(params[0].parameter_name, "maxmemory-policy");
    }

    #[test]
    fn default_parameters_for_unknown_family_returns_empty() {
        let params = default_parameters_for_family("unknown");
        assert!(params.is_empty());
    }
}
