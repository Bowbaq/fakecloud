mod helpers;

use aws_sdk_bedrock::types::Tag;
use helpers::TestServer;

/// Guardrails and their versions survive a restart.
#[tokio::test]
async fn persistence_round_trip_guardrail() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.bedrock_client().await;

    let created = client
        .create_guardrail()
        .name("persist-guard")
        .description("Persistence test guardrail")
        .blocked_input_messaging("blocked")
        .blocked_outputs_messaging("blocked")
        .send()
        .await
        .unwrap();
    let guardrail_id = created.guardrail_id().to_string();

    // Create a version
    client
        .create_guardrail_version()
        .guardrail_identifier(&guardrail_id)
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.bedrock_client().await;

    // Guardrail survives
    let got = client
        .get_guardrail()
        .guardrail_identifier(&guardrail_id)
        .send()
        .await
        .unwrap();
    assert_eq!(got.name(), "persist-guard");
    assert_eq!(got.description(), Some("Persistence test guardrail"));

    // List should include it
    let list = client.list_guardrails().send().await.unwrap();
    assert!(list
        .guardrails()
        .iter()
        .any(|g| g.name() == "persist-guard"));
}

/// Tags survive a restart.
#[tokio::test]
async fn persistence_round_trip_tags() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.bedrock_client().await;

    let created = client
        .create_guardrail()
        .name("tagged-guard")
        .blocked_input_messaging("blocked")
        .blocked_outputs_messaging("blocked")
        .send()
        .await
        .unwrap();
    let guardrail_arn = created.guardrail_arn().to_string();

    client
        .tag_resource()
        .resource_arn(&guardrail_arn)
        .tags(Tag::builder().key("env").value("prod").build().unwrap())
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.bedrock_client().await;

    let tags = client
        .list_tags_for_resource()
        .resource_arn(&guardrail_arn)
        .send()
        .await
        .unwrap();
    assert!(tags
        .tags()
        .iter()
        .any(|t: &Tag| t.key() == "env" && t.value() == "prod"));
}

/// Deletion survives a restart.
#[tokio::test]
async fn persistence_deletion_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.bedrock_client().await;

    let created = client
        .create_guardrail()
        .name("doomed-guard")
        .blocked_input_messaging("blocked")
        .blocked_outputs_messaging("blocked")
        .send()
        .await
        .unwrap();
    let guardrail_id = created.guardrail_id().to_string();

    client
        .delete_guardrail()
        .guardrail_identifier(&guardrail_id)
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.bedrock_client().await;

    let list = client.list_guardrails().send().await.unwrap();
    assert!(
        !list.guardrails().iter().any(|g| g.name() == "doomed-guard"),
        "deleted guardrail should not reappear"
    );
}
