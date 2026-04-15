mod helpers;

use aws_sdk_secretsmanager::primitives::Blob;
use helpers::TestServer;

/// Secret metadata, tags, description, and current version payload
/// survive a restart.
#[tokio::test]
async fn persistence_round_trip_secret_with_versions_and_tags() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let sm = server.secretsmanager_client().await;

    sm.create_secret()
        .name("db/prod")
        .description("prod db credentials")
        .secret_string("v1-secret")
        .tags(
            aws_sdk_secretsmanager::types::Tag::builder()
                .key("env")
                .value("prod")
                .build(),
        )
        .send()
        .await
        .unwrap();

    // Write a second version.
    sm.put_secret_value()
        .secret_id("db/prod")
        .secret_string("v2-secret")
        .send()
        .await
        .unwrap();

    server.restart().await;
    let sm = server.secretsmanager_client().await;

    // Current version is v2-secret.
    let got = sm
        .get_secret_value()
        .secret_id("db/prod")
        .send()
        .await
        .unwrap();
    assert_eq!(got.secret_string(), Some("v2-secret"));

    // Description survives.
    let desc = sm
        .describe_secret()
        .secret_id("db/prod")
        .send()
        .await
        .unwrap();
    assert_eq!(desc.description(), Some("prod db credentials"));
    let tags: Vec<(String, String)> = desc
        .tags()
        .iter()
        .map(|t| {
            (
                t.key().unwrap_or_default().to_string(),
                t.value().unwrap_or_default().to_string(),
            )
        })
        .collect();
    assert!(tags.iter().any(|(k, v)| k == "env" && v == "prod"));

    // Both version ids are still listed.
    let versions = sm
        .list_secret_version_ids()
        .secret_id("db/prod")
        .include_deprecated(true)
        .send()
        .await
        .unwrap();
    assert!(versions.versions().len() >= 2);
}

/// Binary secret payload survives a restart.
#[tokio::test]
async fn persistence_round_trip_binary_secret() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let sm = server.secretsmanager_client().await;

    let bytes = b"\x00\x01\x02hello-binary\xff".to_vec();
    sm.create_secret()
        .name("blob")
        .secret_binary(Blob::new(bytes.clone()))
        .send()
        .await
        .unwrap();

    server.restart().await;
    let sm = server.secretsmanager_client().await;

    let got = sm
        .get_secret_value()
        .secret_id("blob")
        .send()
        .await
        .unwrap();
    assert_eq!(got.secret_binary().unwrap().as_ref(), bytes.as_slice());
}

/// DeleteSecret + RestoreSecret durability.
#[tokio::test]
async fn persistence_delete_and_restore_survive_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let sm = server.secretsmanager_client().await;

    sm.create_secret()
        .name("soft-delete")
        .secret_string("payload")
        .send()
        .await
        .unwrap();
    sm.delete_secret()
        .secret_id("soft-delete")
        .recovery_window_in_days(7)
        .send()
        .await
        .unwrap();

    server.restart().await;
    let sm = server.secretsmanager_client().await;

    // Described secret should still show as scheduled for deletion.
    let desc = sm
        .describe_secret()
        .secret_id("soft-delete")
        .send()
        .await
        .unwrap();
    assert!(desc.deleted_date().is_some());

    // Restore and then verify value still there.
    sm.restore_secret()
        .secret_id("soft-delete")
        .send()
        .await
        .unwrap();

    server.restart().await;
    let sm = server.secretsmanager_client().await;

    let got = sm
        .get_secret_value()
        .secret_id("soft-delete")
        .send()
        .await
        .unwrap();
    assert_eq!(got.secret_string(), Some("payload"));
}
