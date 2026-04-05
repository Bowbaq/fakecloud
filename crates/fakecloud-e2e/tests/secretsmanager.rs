mod helpers;

use aws_sdk_secretsmanager::types::Tag;
use helpers::TestServer;

#[tokio::test]
async fn secretsmanager_create_get_delete() {
    let server = TestServer::start().await;
    let client = server.secretsmanager_client().await;

    // Create
    let resp = client
        .create_secret()
        .name("test/db-password")
        .secret_string("supersecret123")
        .send()
        .await
        .unwrap();

    assert_eq!(resp.name().unwrap(), "test/db-password");
    assert!(resp.arn().unwrap().contains("test/db-password"));
    let version_id = resp.version_id().unwrap().to_string();
    assert!(!version_id.is_empty());

    // Get
    let resp = client
        .get_secret_value()
        .secret_id("test/db-password")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.secret_string().unwrap(), "supersecret123");
    assert_eq!(resp.name().unwrap(), "test/db-password");

    // Delete (force)
    client
        .delete_secret()
        .secret_id("test/db-password")
        .force_delete_without_recovery(true)
        .send()
        .await
        .unwrap();

    // Verify deleted
    let result = client
        .get_secret_value()
        .secret_id("test/db-password")
        .send()
        .await;
    assert!(result.is_err());
}

#[tokio::test]
async fn secretsmanager_put_secret_value_versioning() {
    let server = TestServer::start().await;
    let client = server.secretsmanager_client().await;

    client
        .create_secret()
        .name("versioned")
        .secret_string("version1")
        .send()
        .await
        .unwrap();

    // Put new version
    let resp = client
        .put_secret_value()
        .secret_id("versioned")
        .secret_string("version2")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.name().unwrap(), "versioned");

    // Get should return version2
    let resp = client
        .get_secret_value()
        .secret_id("versioned")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.secret_string().unwrap(), "version2");

    // List versions
    let resp = client
        .list_secret_version_ids()
        .secret_id("versioned")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.versions().len(), 2);
}

#[tokio::test]
async fn secretsmanager_delete_and_restore() {
    let server = TestServer::start().await;
    let client = server.secretsmanager_client().await;

    client
        .create_secret()
        .name("restorable")
        .secret_string("myvalue")
        .send()
        .await
        .unwrap();

    // Soft delete
    let resp = client
        .delete_secret()
        .secret_id("restorable")
        .send()
        .await
        .unwrap();
    assert!(resp.deletion_date().is_some());

    // Get should fail
    let result = client
        .get_secret_value()
        .secret_id("restorable")
        .send()
        .await;
    assert!(result.is_err());

    // Restore
    client
        .restore_secret()
        .secret_id("restorable")
        .send()
        .await
        .unwrap();

    // Get should work again
    let resp = client
        .get_secret_value()
        .secret_id("restorable")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.secret_string().unwrap(), "myvalue");
}

#[tokio::test]
async fn secretsmanager_list_secrets() {
    let server = TestServer::start().await;
    let client = server.secretsmanager_client().await;

    for name in &["secret-a", "secret-b", "secret-c"] {
        client
            .create_secret()
            .name(*name)
            .secret_string("val")
            .send()
            .await
            .unwrap();
    }

    let resp = client.list_secrets().send().await.unwrap();
    assert_eq!(resp.secret_list().len(), 3);
}

#[tokio::test]
async fn secretsmanager_tags() {
    let server = TestServer::start().await;
    let client = server.secretsmanager_client().await;

    client
        .create_secret()
        .name("tagged-secret")
        .secret_string("val")
        .send()
        .await
        .unwrap();

    // Tag
    client
        .tag_resource()
        .secret_id("tagged-secret")
        .tags(
            Tag::builder()
                .key("environment")
                .value("production")
                .build(),
        )
        .tags(Tag::builder().key("team").value("backend").build())
        .send()
        .await
        .unwrap();

    // Describe to check tags
    let resp = client
        .describe_secret()
        .secret_id("tagged-secret")
        .send()
        .await
        .unwrap();
    let tags = resp.tags();
    assert_eq!(tags.len(), 2);

    // Untag
    client
        .untag_resource()
        .secret_id("tagged-secret")
        .tag_keys("team")
        .send()
        .await
        .unwrap();

    let resp = client
        .describe_secret()
        .secret_id("tagged-secret")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.tags().len(), 1);
}

#[tokio::test]
async fn secretsmanager_describe_secret() {
    let server = TestServer::start().await;
    let client = server.secretsmanager_client().await;

    client
        .create_secret()
        .name("described")
        .secret_string("value")
        .description("A test secret")
        .send()
        .await
        .unwrap();

    let resp = client
        .describe_secret()
        .secret_id("described")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.name().unwrap(), "described");
    assert_eq!(resp.description().unwrap(), "A test secret");
    assert!(resp.created_date().is_some());
    assert!(!resp.version_ids_to_stages().unwrap().is_empty());
}

#[tokio::test]
async fn secretsmanager_duplicate_create_fails() {
    let server = TestServer::start().await;
    let client = server.secretsmanager_client().await;

    client
        .create_secret()
        .name("dup-secret")
        .secret_string("val")
        .send()
        .await
        .unwrap();

    let result = client
        .create_secret()
        .name("dup-secret")
        .secret_string("val2")
        .send()
        .await;
    assert!(result.is_err());
}
