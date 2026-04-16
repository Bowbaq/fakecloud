mod helpers;

use helpers::TestServer;

/// Users and user groups survive a restart.
#[tokio::test]
async fn persistence_round_trip_user_and_group() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.elasticache_client().await;

    client
        .create_user()
        .user_id("persist-user")
        .user_name("persist-user")
        .engine("redis")
        .access_string("on ~* +@all")
        .no_password_required(true)
        .send()
        .await
        .unwrap();

    client
        .create_user_group()
        .user_group_id("persist-group")
        .engine("redis")
        .user_ids("default")
        .user_ids("persist-user")
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.elasticache_client().await;

    // User survives
    let users = client
        .describe_users()
        .user_id("persist-user")
        .send()
        .await
        .unwrap();
    assert_eq!(users.users().len(), 1);
    assert_eq!(users.users()[0].user_id(), Some("persist-user"));

    // User group survives
    let groups = client
        .describe_user_groups()
        .user_group_id("persist-group")
        .send()
        .await
        .unwrap();
    assert_eq!(groups.user_groups().len(), 1);
    let group = &groups.user_groups()[0];
    assert_eq!(group.user_group_id(), Some("persist-group"));
    assert!(group.user_ids().contains(&"persist-user".to_string()));
    assert!(group.user_ids().contains(&"default".to_string()));
}

/// Subnet groups survive a restart.
#[tokio::test]
async fn persistence_round_trip_subnet_group() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.elasticache_client().await;

    client
        .create_cache_subnet_group()
        .cache_subnet_group_name("persist-sg")
        .cache_subnet_group_description("Persistence test subnet group")
        .subnet_ids("subnet-aaa")
        .subnet_ids("subnet-bbb")
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.elasticache_client().await;

    let groups = client
        .describe_cache_subnet_groups()
        .cache_subnet_group_name("persist-sg")
        .send()
        .await
        .unwrap();
    let sgs = groups.cache_subnet_groups();
    assert_eq!(sgs.len(), 1);
    assert_eq!(sgs[0].cache_subnet_group_name(), Some("persist-sg"));
    assert_eq!(
        sgs[0].cache_subnet_group_description(),
        Some("Persistence test subnet group")
    );
}

/// Tags survive a restart.
#[tokio::test]
async fn persistence_round_trip_tags() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.elasticache_client().await;

    client
        .create_user()
        .user_id("tagged-user")
        .user_name("tagged-user")
        .engine("redis")
        .access_string("on ~* +@all")
        .no_password_required(true)
        .send()
        .await
        .unwrap();

    // Get the ARN
    let users = client
        .describe_users()
        .user_id("tagged-user")
        .send()
        .await
        .unwrap();
    let arn = users.users()[0].arn().unwrap().to_string();

    client
        .add_tags_to_resource()
        .resource_name(&arn)
        .tags(
            aws_sdk_elasticache::types::Tag::builder()
                .key("env")
                .value("prod")
                .build(),
        )
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.elasticache_client().await;

    let tags = client
        .list_tags_for_resource()
        .resource_name(&arn)
        .send()
        .await
        .unwrap();
    assert!(tags
        .tag_list()
        .iter()
        .any(|t| t.key() == Some("env") && t.value() == Some("prod")));
}

/// Deletion survives a restart.
#[tokio::test]
async fn persistence_deletion_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.elasticache_client().await;

    client
        .create_user()
        .user_id("doomed-user")
        .user_name("doomed-user")
        .engine("redis")
        .access_string("on ~* +@all")
        .no_password_required(true)
        .send()
        .await
        .unwrap();

    client
        .delete_user()
        .user_id("doomed-user")
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.elasticache_client().await;

    let users = client.describe_users().send().await.unwrap();
    assert!(
        !users
            .users()
            .iter()
            .any(|u| u.user_id() == Some("doomed-user")),
        "deleted user should not reappear"
    );
}
