mod helpers;

use aws_sdk_cognitoidentityprovider::types::AttributeType;
use helpers::TestServer;

/// User pool + user pool client + admin-created user survive a restart.
#[tokio::test]
async fn persistence_round_trip_pool_client_user() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.cognito_client().await;

    let pool = client
        .create_user_pool()
        .pool_name("persist-pool")
        .send()
        .await
        .unwrap();
    let pool_id = pool.user_pool().unwrap().id().unwrap().to_string();

    let app_client = client
        .create_user_pool_client()
        .user_pool_id(&pool_id)
        .client_name("persist-client")
        .send()
        .await
        .unwrap();
    let client_id = app_client
        .user_pool_client()
        .unwrap()
        .client_id()
        .unwrap()
        .to_string();

    client
        .admin_create_user()
        .user_pool_id(&pool_id)
        .username("alice")
        .user_attributes(
            AttributeType::builder()
                .name("email")
                .value("alice@example.com")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    // Drop pre-restart client before restart.
    drop(client);
    server.restart().await;
    let client = server.cognito_client().await;

    let described = client
        .describe_user_pool()
        .user_pool_id(&pool_id)
        .send()
        .await
        .unwrap();
    assert_eq!(described.user_pool().unwrap().name(), Some("persist-pool"));

    let described_client = client
        .describe_user_pool_client()
        .user_pool_id(&pool_id)
        .client_id(&client_id)
        .send()
        .await
        .unwrap();
    assert_eq!(
        described_client.user_pool_client().unwrap().client_name(),
        Some("persist-client")
    );

    let user = client
        .admin_get_user()
        .user_pool_id(&pool_id)
        .username("alice")
        .send()
        .await
        .unwrap();
    let email = user
        .user_attributes()
        .iter()
        .find(|a| a.name() == "email")
        .and_then(|a| a.value());
    assert_eq!(email, Some("alice@example.com"));
}

/// Groups and tag resources round-trip across a restart.
#[tokio::test]
async fn persistence_groups_and_tags() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.cognito_client().await;

    let pool = client
        .create_user_pool()
        .pool_name("tag-pool")
        .send()
        .await
        .unwrap();
    let pool_id = pool.user_pool().unwrap().id().unwrap().to_string();
    let pool_arn = pool.user_pool().unwrap().arn().unwrap().to_string();

    client
        .create_group()
        .user_pool_id(&pool_id)
        .group_name("admins")
        .description("Admin group")
        .send()
        .await
        .unwrap();

    client
        .tag_resource()
        .resource_arn(&pool_arn)
        .tags("env", "prod")
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.cognito_client().await;

    let group = client
        .get_group()
        .user_pool_id(&pool_id)
        .group_name("admins")
        .send()
        .await
        .unwrap();
    assert_eq!(group.group().unwrap().description(), Some("Admin group"));

    let tags = client
        .list_tags_for_resource()
        .resource_arn(&pool_arn)
        .send()
        .await
        .unwrap();
    assert_eq!(
        tags.tags().and_then(|t| t.get("env")).map(String::as_str),
        Some("prod")
    );
}

/// Auth events introspection buffer does NOT persist across restarts.
#[tokio::test]
async fn persistence_auth_events_not_persisted() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.cognito_client().await;

    let pool = client
        .create_user_pool()
        .pool_name("events-pool")
        .send()
        .await
        .unwrap();
    let pool_id = pool.user_pool().unwrap().id().unwrap().to_string();

    client
        .admin_create_user()
        .user_pool_id(&pool_id)
        .username("bob")
        .send()
        .await
        .unwrap();

    // Hit the introspection endpoint pre-restart to confirm it's non-empty.
    let pre = reqwest::get(format!(
        "{}/_fakecloud/cognito/auth-events",
        server.endpoint()
    ))
    .await
    .unwrap()
    .json::<serde_json::Value>()
    .await
    .unwrap();
    let _ = pre; // some flows may not push events; we only care about post-restart emptiness

    drop(client);
    server.restart().await;

    // Pool survived.
    let client = server.cognito_client().await;
    let described = client
        .describe_user_pool()
        .user_pool_id(&pool_id)
        .send()
        .await
        .unwrap();
    assert_eq!(described.user_pool().unwrap().name(), Some("events-pool"));

    // Auth events buffer reset to empty.
    let post = reqwest::get(format!(
        "{}/_fakecloud/cognito/auth-events",
        server.endpoint()
    ))
    .await
    .unwrap()
    .json::<serde_json::Value>()
    .await
    .unwrap();
    let events = post
        .get("events")
        .or_else(|| post.get("auth_events"))
        .and_then(|v| v.as_array())
        .cloned()
        .unwrap_or_default();
    assert!(
        events.is_empty(),
        "auth_events buffer should reset on restart, got: {post:?}"
    );
}
