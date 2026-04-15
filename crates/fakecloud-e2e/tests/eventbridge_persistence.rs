mod helpers;

use aws_sdk_eventbridge::types::{RuleState, Target};
use helpers::TestServer;

/// Custom bus + rule + targets + tags survive restart.
#[tokio::test]
async fn persistence_round_trip_bus_rule_and_targets() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let eb = server.eventbridge_client().await;
    let sqs = server.sqs_client().await;

    eb.create_event_bus()
        .name("orders-bus")
        .send()
        .await
        .unwrap();

    eb.put_rule()
        .name("order-created")
        .event_bus_name("orders-bus")
        .event_pattern(r#"{"source":["shop.orders"]}"#)
        .state(RuleState::Enabled)
        .description("route order.created events")
        .send()
        .await
        .unwrap();

    let q_url = sqs
        .create_queue()
        .queue_name("orders-target")
        .send()
        .await
        .unwrap()
        .queue_url
        .unwrap();
    let q_arn = sqs
        .get_queue_attributes()
        .queue_url(&q_url)
        .attribute_names(aws_sdk_sqs::types::QueueAttributeName::QueueArn)
        .send()
        .await
        .unwrap()
        .attributes
        .unwrap()
        .get(&aws_sdk_sqs::types::QueueAttributeName::QueueArn)
        .unwrap()
        .clone();

    eb.put_targets()
        .rule("order-created")
        .event_bus_name("orders-bus")
        .targets(Target::builder().id("sqs-1").arn(&q_arn).build().unwrap())
        .send()
        .await
        .unwrap();

    eb.tag_resource()
        .resource_arn(
            eb.describe_rule()
                .name("order-created")
                .event_bus_name("orders-bus")
                .send()
                .await
                .unwrap()
                .arn
                .unwrap(),
        )
        .tags(
            aws_sdk_eventbridge::types::Tag::builder()
                .key("env")
                .value("prod")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    server.restart().await;
    let eb = server.eventbridge_client().await;

    // Bus survives.
    let buses = eb.list_event_buses().send().await.unwrap();
    assert!(buses
        .event_buses()
        .iter()
        .any(|b| b.name() == Some("orders-bus")));

    // Rule survives on the custom bus.
    let rules = eb
        .list_rules()
        .event_bus_name("orders-bus")
        .send()
        .await
        .unwrap();
    let rule = rules
        .rules()
        .iter()
        .find(|r| r.name() == Some("order-created"))
        .unwrap();
    assert_eq!(rule.state(), Some(&RuleState::Enabled));
    assert_eq!(rule.description(), Some("route order.created events"));

    // Target survives.
    let targets = eb
        .list_targets_by_rule()
        .rule("order-created")
        .event_bus_name("orders-bus")
        .send()
        .await
        .unwrap();
    assert!(targets
        .targets()
        .iter()
        .any(|t| t.id() == "sqs-1" && t.arn() == q_arn));

    // Describe rule returns description still.
    let desc = eb
        .describe_rule()
        .name("order-created")
        .event_bus_name("orders-bus")
        .send()
        .await
        .unwrap();
    assert_eq!(desc.description(), Some("route order.created events"));
}

/// Disabled rule stays disabled after restart.
#[tokio::test]
async fn persistence_disabled_rule_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let eb = server.eventbridge_client().await;

    eb.put_rule()
        .name("off-rule")
        .schedule_expression("rate(5 minutes)")
        .state(RuleState::Disabled)
        .send()
        .await
        .unwrap();

    server.restart().await;
    let eb = server.eventbridge_client().await;

    let desc = eb.describe_rule().name("off-rule").send().await.unwrap();
    assert_eq!(desc.state(), Some(&RuleState::Disabled));
    assert_eq!(desc.schedule_expression(), Some("rate(5 minutes)"));
}

/// DeleteRule is durable.
#[tokio::test]
async fn persistence_delete_rule_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let eb = server.eventbridge_client().await;

    eb.put_rule()
        .name("ephemeral")
        .event_pattern(r#"{"source":["x"]}"#)
        .send()
        .await
        .unwrap();
    eb.delete_rule().name("ephemeral").send().await.unwrap();

    server.restart().await;
    let eb = server.eventbridge_client().await;
    let rules = eb.list_rules().send().await.unwrap();
    assert!(!rules.rules().iter().any(|r| r.name() == Some("ephemeral")));
}
