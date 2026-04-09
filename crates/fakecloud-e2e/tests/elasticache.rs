mod helpers;

use helpers::TestServer;

#[tokio::test]
async fn elasticache_describe_cache_engine_versions_all() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    let response = client
        .describe_cache_engine_versions()
        .send()
        .await
        .unwrap();

    let versions = response.cache_engine_versions();
    assert!(versions.len() >= 2);

    let redis = versions.iter().find(|v| v.engine() == Some("redis"));
    assert!(redis.is_some());
    assert_eq!(redis.unwrap().engine_version(), Some("7.1"));

    let valkey = versions.iter().find(|v| v.engine() == Some("valkey"));
    assert!(valkey.is_some());
    assert_eq!(valkey.unwrap().engine_version(), Some("8.0"));
}

#[tokio::test]
async fn elasticache_describe_cache_engine_versions_filter_by_engine() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    let response = client
        .describe_cache_engine_versions()
        .engine("valkey")
        .send()
        .await
        .unwrap();

    let versions = response.cache_engine_versions();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].engine(), Some("valkey"));
    assert_eq!(versions[0].engine_version(), Some("8.0"));
    assert_eq!(versions[0].cache_parameter_group_family(), Some("valkey8"));
}

#[tokio::test]
async fn elasticache_describe_engine_default_parameters_redis7() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    let response = client
        .describe_engine_default_parameters()
        .cache_parameter_group_family("redis7")
        .send()
        .await
        .unwrap();

    let defaults = response.engine_defaults().expect("engine defaults");
    assert_eq!(defaults.cache_parameter_group_family(), Some("redis7"));
    let params = defaults.parameters();
    assert_eq!(params.len(), 3);

    let maxmemory = params
        .iter()
        .find(|p| p.parameter_name() == Some("maxmemory-policy"))
        .expect("maxmemory-policy parameter");
    assert_eq!(maxmemory.parameter_value(), Some("volatile-lru"));
    assert_eq!(maxmemory.is_modifiable(), Some(true));
}

#[tokio::test]
async fn elasticache_describe_engine_default_parameters_valkey8() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    let response = client
        .describe_engine_default_parameters()
        .cache_parameter_group_family("valkey8")
        .send()
        .await
        .unwrap();

    let defaults = response.engine_defaults().expect("engine defaults");
    assert_eq!(defaults.cache_parameter_group_family(), Some("valkey8"));
    let params = defaults.parameters();
    assert_eq!(params.len(), 3);
}

#[tokio::test]
async fn elasticache_describe_cache_parameter_groups_all() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    let response = client
        .describe_cache_parameter_groups()
        .send()
        .await
        .unwrap();

    let groups = response.cache_parameter_groups();
    assert!(groups.len() >= 2);

    let redis_group = groups
        .iter()
        .find(|g| g.cache_parameter_group_name() == Some("default.redis7"));
    assert!(redis_group.is_some());
    assert_eq!(
        redis_group.unwrap().cache_parameter_group_family(),
        Some("redis7")
    );

    let valkey_group = groups
        .iter()
        .find(|g| g.cache_parameter_group_name() == Some("default.valkey8"));
    assert!(valkey_group.is_some());
}

#[tokio::test]
async fn elasticache_describe_cache_parameter_groups_by_name() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    let response = client
        .describe_cache_parameter_groups()
        .cache_parameter_group_name("default.redis7")
        .send()
        .await
        .unwrap();

    let groups = response.cache_parameter_groups();
    assert_eq!(groups.len(), 1);
    assert_eq!(
        groups[0].cache_parameter_group_name(),
        Some("default.redis7")
    );
}
