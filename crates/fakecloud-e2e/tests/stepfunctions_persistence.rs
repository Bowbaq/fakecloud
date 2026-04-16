mod helpers;

use aws_sdk_sfn::types::Tag;
use helpers::TestServer;

fn simple_definition() -> String {
    serde_json::json!({
        "StartAt": "Hello",
        "States": {
            "Hello": { "Type": "Pass", "Result": "Hello!", "End": true }
        }
    })
    .to_string()
}

/// State machine, tags and execution survive a restart.
#[tokio::test]
async fn persistence_round_trip_state_machine_and_execution() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.sfn_client().await;

    let create = client
        .create_state_machine()
        .name("persist-sm")
        .definition(simple_definition())
        .role_arn("arn:aws:iam::123456789012:role/test-role")
        .tags(Tag::builder().key("env").value("prod").build())
        .send()
        .await
        .unwrap();
    let sm_arn = create.state_machine_arn().to_string();

    let exec = client
        .start_execution()
        .state_machine_arn(&sm_arn)
        .name("persist-exec")
        .input("{}")
        .send()
        .await
        .unwrap();
    let exec_arn = exec.execution_arn().to_string();

    drop(client);
    server.restart().await;
    let client = server.sfn_client().await;

    let described = client
        .describe_state_machine()
        .state_machine_arn(&sm_arn)
        .send()
        .await
        .unwrap();
    assert_eq!(described.name(), "persist-sm");

    let tags = client
        .list_tags_for_resource()
        .resource_arn(&sm_arn)
        .send()
        .await
        .unwrap();
    assert!(tags
        .tags()
        .iter()
        .any(|t| t.key() == Some("env") && t.value() == Some("prod")));

    let exec_desc = client
        .describe_execution()
        .execution_arn(&exec_arn)
        .send()
        .await
        .unwrap();
    assert_eq!(exec_desc.execution_arn(), exec_arn.as_str());
    assert_eq!(exec_desc.state_machine_arn(), sm_arn.as_str());

    // Execution shows up in ListExecutions after restart.
    let listed = client
        .list_executions()
        .state_machine_arn(&sm_arn)
        .send()
        .await
        .unwrap();
    assert!(listed
        .executions()
        .iter()
        .any(|e| e.execution_arn() == exec_arn.as_str()));
}

/// Deletion survives a restart.
#[tokio::test]
async fn persistence_delete_state_machine_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.sfn_client().await;

    let create = client
        .create_state_machine()
        .name("doomed-sm")
        .definition(simple_definition())
        .role_arn("arn:aws:iam::123456789012:role/test-role")
        .send()
        .await
        .unwrap();
    let sm_arn = create.state_machine_arn().to_string();

    client
        .delete_state_machine()
        .state_machine_arn(&sm_arn)
        .send()
        .await
        .unwrap();

    drop(client);
    server.restart().await;
    let client = server.sfn_client().await;

    let listed = client.list_state_machines().send().await.unwrap();
    assert!(!listed
        .state_machines()
        .iter()
        .any(|s| s.name() == "doomed-sm"));
}
