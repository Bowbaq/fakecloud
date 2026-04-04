mod helpers;

use aws_sdk_eventbridge::types::{PutEventsRequestEntry, Target};
use helpers::TestServer;

#[tokio::test]
async fn eb_list_default_bus() {
    let server = TestServer::start().await;
    let client = server.eventbridge_client().await;

    let resp = client.list_event_buses().send().await.unwrap();
    let buses = resp.event_buses();
    assert!(buses.iter().any(|b| b.name().unwrap() == "default"));
}

#[tokio::test]
async fn eb_create_delete_event_bus() {
    let server = TestServer::start().await;
    let client = server.eventbridge_client().await;

    let resp = client
        .create_event_bus()
        .name("custom-bus")
        .send()
        .await
        .unwrap();
    assert!(resp.event_bus_arn().unwrap().contains("custom-bus"));

    let resp = client.list_event_buses().send().await.unwrap();
    assert!(resp
        .event_buses()
        .iter()
        .any(|b| b.name().unwrap() == "custom-bus"));

    client
        .delete_event_bus()
        .name("custom-bus")
        .send()
        .await
        .unwrap();

    let resp = client.list_event_buses().send().await.unwrap();
    assert!(!resp
        .event_buses()
        .iter()
        .any(|b| b.name().unwrap() == "custom-bus"));
}

#[tokio::test]
async fn eb_put_rule_with_targets() {
    let server = TestServer::start().await;
    let client = server.eventbridge_client().await;

    // Put rule
    let resp = client
        .put_rule()
        .name("my-rule")
        .event_pattern(r#"{"source": ["my.app"]}"#)
        .send()
        .await
        .unwrap();
    assert!(resp.rule_arn().unwrap().contains("my-rule"));

    // List rules
    let resp = client.list_rules().send().await.unwrap();
    assert_eq!(resp.rules().len(), 1);
    assert_eq!(resp.rules()[0].name().unwrap(), "my-rule");

    // Put targets
    client
        .put_targets()
        .rule("my-rule")
        .targets(
            Target::builder()
                .id("target-1")
                .arn("arn:aws:sqs:us-east-1:000000000000:my-queue")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    // List targets
    let resp = client
        .list_targets_by_rule()
        .rule("my-rule")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.targets().len(), 1);
    assert_eq!(resp.targets()[0].id(), "target-1");

    // Remove targets
    client
        .remove_targets()
        .rule("my-rule")
        .ids("target-1")
        .send()
        .await
        .unwrap();

    let resp = client
        .list_targets_by_rule()
        .rule("my-rule")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.targets().len(), 0);

    // Delete rule
    client.delete_rule().name("my-rule").send().await.unwrap();

    let resp = client.list_rules().send().await.unwrap();
    assert_eq!(resp.rules().len(), 0);
}

#[tokio::test]
async fn eb_put_events() {
    let server = TestServer::start().await;
    let client = server.eventbridge_client().await;

    let resp = client
        .put_events()
        .entries(
            PutEventsRequestEntry::builder()
                .source("my.app")
                .detail_type("OrderPlaced")
                .detail(r#"{"orderId": "123"}"#)
                .build(),
        )
        .entries(
            PutEventsRequestEntry::builder()
                .source("my.app")
                .detail_type("OrderShipped")
                .detail(r#"{"orderId": "456"}"#)
                .build(),
        )
        .send()
        .await
        .unwrap();

    assert_eq!(resp.failed_entry_count(), 0);
    assert_eq!(resp.entries().len(), 2);
    assert!(resp.entries()[0].event_id().is_some());
}

#[tokio::test]
async fn eb_cli_put_events() {
    let server = TestServer::start().await;
    let output = server
        .aws_cli(&[
            "events",
            "put-events",
            "--entries",
            r#"[{"Source":"test","DetailType":"Test","Detail":"{}"}]"#,
        ])
        .await;
    assert!(output.success(), "failed: {}", output.stderr_text());
    let json = output.stdout_json();
    assert_eq!(json["FailedEntryCount"], 0);
}
