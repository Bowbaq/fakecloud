mod helpers;

use helpers::TestServer;

/// Stack, template, parameters and tags survive a restart.
#[tokio::test]
async fn persistence_round_trip_stack() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let cf = server.cloudformation_client().await;

    let template = r#"{
        "Parameters": { "QueueName": { "Type": "String" } },
        "Resources": {
            "MyQueue": {
                "Type": "AWS::SQS::Queue",
                "Properties": { "QueueName": { "Ref": "QueueName" } }
            }
        }
    }"#;

    cf.create_stack()
        .stack_name("persist-stack")
        .template_body(template)
        .parameters(
            aws_sdk_cloudformation::types::Parameter::builder()
                .parameter_key("QueueName")
                .parameter_value("persist-queue")
                .build(),
        )
        .tags(
            aws_sdk_cloudformation::types::Tag::builder()
                .key("env")
                .value("prod")
                .build(),
        )
        .send()
        .await
        .unwrap();

    drop(cf);
    server.restart().await;
    let cf = server.cloudformation_client().await;

    let described = cf
        .describe_stacks()
        .stack_name("persist-stack")
        .send()
        .await
        .unwrap();
    let stacks = described.stacks();
    assert_eq!(stacks.len(), 1);
    let stack = &stacks[0];
    assert_eq!(stack.stack_name(), Some("persist-stack"));
    assert!(stack
        .tags()
        .iter()
        .any(|t| t.key() == Some("env") && t.value() == Some("prod")));
    assert!(stack
        .parameters()
        .iter()
        .any(|p| p.parameter_key() == Some("QueueName")
            && p.parameter_value() == Some("persist-queue")));

    // Template body recoverable.
    let got_template = cf
        .get_template()
        .stack_name("persist-stack")
        .send()
        .await
        .unwrap();
    assert!(got_template
        .template_body()
        .unwrap_or_default()
        .contains("MyQueue"));

    // Resources list still populated.
    let resources = cf
        .list_stack_resources()
        .stack_name("persist-stack")
        .send()
        .await
        .unwrap();
    assert!(resources
        .stack_resource_summaries()
        .iter()
        .any(|r| r.logical_resource_id() == Some("MyQueue")));
}

/// Deletion survives a restart: a deleted stack does not reappear.
#[tokio::test]
async fn persistence_deletion_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let cf = server.cloudformation_client().await;

    let template = r#"{
        "Resources": {
            "MyQueue": {
                "Type": "AWS::SQS::Queue",
                "Properties": { "QueueName": "to-delete" }
            }
        }
    }"#;

    cf.create_stack()
        .stack_name("doomed")
        .template_body(template)
        .send()
        .await
        .unwrap();

    cf.delete_stack().stack_name("doomed").send().await.unwrap();

    drop(cf);
    server.restart().await;
    let cf = server.cloudformation_client().await;

    let listed = cf.list_stacks().send().await.unwrap();
    assert!(!listed
        .stack_summaries()
        .iter()
        .any(|s| s.stack_name() == Some("doomed")
            && s.stack_status()
                != Some(&aws_sdk_cloudformation::types::StackStatus::DeleteComplete)));
}
