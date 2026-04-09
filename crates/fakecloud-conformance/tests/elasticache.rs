mod helpers;

use fakecloud_conformance_macros::test_action;
use helpers::TestServer;

#[test_action("elasticache", "CreateCacheSubnetGroup", checksum = "84cb3eb4")]
#[tokio::test]
async fn elasticache_create_cache_subnet_group() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    let response = client
        .create_cache_subnet_group()
        .cache_subnet_group_name("test-subnet-group")
        .cache_subnet_group_description("Test subnet group")
        .subnet_ids("subnet-abc123")
        .send()
        .await
        .unwrap();

    let group = response.cache_subnet_group().expect("cache subnet group");
    assert_eq!(group.cache_subnet_group_name(), Some("test-subnet-group"));
    assert_eq!(
        group.cache_subnet_group_description(),
        Some("Test subnet group")
    );
}

#[test_action("elasticache", "DeleteCacheSubnetGroup", checksum = "9ffab4c4")]
#[tokio::test]
async fn elasticache_delete_cache_subnet_group() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    client
        .create_cache_subnet_group()
        .cache_subnet_group_name("to-delete")
        .cache_subnet_group_description("Will be deleted")
        .subnet_ids("subnet-abc123")
        .send()
        .await
        .unwrap();

    client
        .delete_cache_subnet_group()
        .cache_subnet_group_name("to-delete")
        .send()
        .await
        .unwrap();
}

#[test_action("elasticache", "DescribeCacheSubnetGroups", checksum = "0f6a2b15")]
#[tokio::test]
async fn elasticache_describe_cache_subnet_groups() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    let response = client.describe_cache_subnet_groups().send().await.unwrap();

    let groups = response.cache_subnet_groups();
    assert!(!groups.is_empty());
    assert!(groups
        .iter()
        .any(|g| g.cache_subnet_group_name() == Some("default")));
}

#[test_action("elasticache", "ModifyCacheSubnetGroup", checksum = "ebab21f4")]
#[tokio::test]
async fn elasticache_modify_cache_subnet_group() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    client
        .create_cache_subnet_group()
        .cache_subnet_group_name("to-modify")
        .cache_subnet_group_description("Original description")
        .subnet_ids("subnet-abc123")
        .send()
        .await
        .unwrap();

    let response = client
        .modify_cache_subnet_group()
        .cache_subnet_group_name("to-modify")
        .cache_subnet_group_description("Updated description")
        .send()
        .await
        .unwrap();

    let group = response.cache_subnet_group().expect("cache subnet group");
    assert_eq!(
        group.cache_subnet_group_description(),
        Some("Updated description")
    );
}

#[test_action("elasticache", "DescribeCacheEngineVersions", checksum = "a71c9f1a")]
#[tokio::test]
async fn elasticache_describe_cache_engine_versions() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    let response = client
        .describe_cache_engine_versions()
        .engine("redis")
        .send()
        .await
        .unwrap();

    let versions = response.cache_engine_versions();
    assert_eq!(versions.len(), 1);
    assert_eq!(versions[0].engine(), Some("redis"));
    assert_eq!(versions[0].engine_version(), Some("7.1"));
    assert_eq!(versions[0].cache_parameter_group_family(), Some("redis7"));
}

#[test_action(
    "elasticache",
    "DescribeEngineDefaultParameters",
    checksum = "0b34416b"
)]
#[tokio::test]
async fn elasticache_describe_engine_default_parameters() {
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
    assert!(!params.is_empty());
    assert_eq!(params[0].parameter_name(), Some("maxmemory-policy"));
}

#[test_action("elasticache", "DescribeCacheParameterGroups", checksum = "f2d641d8")]
#[tokio::test]
async fn elasticache_describe_cache_parameter_groups() {
    let server = TestServer::start().await;
    let client = server.elasticache_client().await;

    let response = client
        .describe_cache_parameter_groups()
        .send()
        .await
        .unwrap();

    let groups = response.cache_parameter_groups();
    assert!(groups.len() >= 2);
    assert_eq!(
        groups[0].cache_parameter_group_name(),
        Some("default.redis7")
    );
    assert_eq!(
        groups[1].cache_parameter_group_name(),
        Some("default.valkey8")
    );
}
