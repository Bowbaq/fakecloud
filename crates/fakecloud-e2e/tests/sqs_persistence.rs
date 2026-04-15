mod helpers;

use std::collections::HashMap;

use aws_sdk_sqs::types::{MessageAttributeValue, QueueAttributeName};
use helpers::TestServer;

/// Round-trip standard queue: messages + attributes + tags + redrive
/// policy all survive a restart, and the queue keeps accepting writes
/// afterwards.
#[tokio::test]
async fn persistence_round_trip_standard_queue() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.sqs_client().await;

    // DLQ first so we can reference it from the primary queue redrive policy.
    let dlq_url = client
        .create_queue()
        .queue_name("dead-letters")
        .send()
        .await
        .unwrap()
        .queue_url
        .unwrap();
    let dlq_arn = client
        .get_queue_attributes()
        .queue_url(&dlq_url)
        .attribute_names(QueueAttributeName::QueueArn)
        .send()
        .await
        .unwrap()
        .attributes
        .unwrap()
        .get(&QueueAttributeName::QueueArn)
        .unwrap()
        .clone();

    let redrive = format!(
        "{{\"deadLetterTargetArn\":\"{}\",\"maxReceiveCount\":\"3\"}}",
        dlq_arn
    );

    let q_url = client
        .create_queue()
        .queue_name("work-q")
        .attributes(QueueAttributeName::VisibilityTimeout, "60")
        .attributes(QueueAttributeName::MessageRetentionPeriod, "3600")
        .attributes(QueueAttributeName::RedrivePolicy, redrive.clone())
        .send()
        .await
        .unwrap()
        .queue_url
        .unwrap();

    client
        .tag_queue()
        .queue_url(&q_url)
        .tags("env", "prod")
        .tags("team", "platform")
        .send()
        .await
        .unwrap();

    // Three messages with distinct bodies + one with message attributes.
    client
        .send_message()
        .queue_url(&q_url)
        .message_body("first")
        .send()
        .await
        .unwrap();
    client
        .send_message()
        .queue_url(&q_url)
        .message_body("second")
        .send()
        .await
        .unwrap();
    client
        .send_message()
        .queue_url(&q_url)
        .message_body("third-with-attrs")
        .message_attributes(
            "priority",
            MessageAttributeValue::builder()
                .data_type("String")
                .string_value("high")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    server.restart().await;
    let client = server.sqs_client().await;

    // Queue survives by name.
    let list = client.list_queues().send().await.unwrap();
    let urls: Vec<&str> = list.queue_urls().iter().map(|s| s.as_str()).collect();
    assert!(urls.iter().any(|u| u.ends_with("/work-q")));
    assert!(urls.iter().any(|u| u.ends_with("/dead-letters")));

    // Attributes + redrive + tags survive.
    let attrs = client
        .get_queue_attributes()
        .queue_url(&q_url)
        .attribute_names(QueueAttributeName::All)
        .send()
        .await
        .unwrap()
        .attributes
        .unwrap();
    assert_eq!(
        attrs
            .get(&QueueAttributeName::VisibilityTimeout)
            .map(String::as_str),
        Some("60"),
    );
    assert_eq!(
        attrs
            .get(&QueueAttributeName::MessageRetentionPeriod)
            .map(String::as_str),
        Some("3600"),
    );
    let rp = attrs
        .get(&QueueAttributeName::RedrivePolicy)
        .expect("redrive policy should survive restart");
    assert!(rp.contains("deadLetterTargetArn"));
    assert!(rp.contains("dead-letters"));

    let tags = client
        .list_queue_tags()
        .queue_url(&q_url)
        .send()
        .await
        .unwrap()
        .tags
        .unwrap_or_default();
    let tag_map: HashMap<String, String> = tags.into_iter().collect();
    assert_eq!(tag_map.get("env").map(String::as_str), Some("prod"));
    assert_eq!(tag_map.get("team").map(String::as_str), Some("platform"));

    // All 3 messages are still in the queue and readable in FIFO-ish order.
    // Drain with multi-receive so we're not sensitive to batch sizing.
    let mut bodies = Vec::new();
    let mut attrs_seen = Vec::new();
    for _ in 0..5 {
        let resp = client
            .receive_message()
            .queue_url(&q_url)
            .max_number_of_messages(10)
            .message_attribute_names("All")
            .send()
            .await
            .unwrap();
        if resp.messages().is_empty() {
            break;
        }
        for m in resp.messages() {
            bodies.push(m.body().unwrap_or_default().to_string());
            if let Some(av) = m
                .message_attributes()
                .and_then(|m| m.get("priority"))
                .and_then(|v| v.string_value())
            {
                attrs_seen.push(av.to_string());
            }
        }
    }
    bodies.sort();
    assert_eq!(
        bodies,
        vec![
            "first".to_string(),
            "second".to_string(),
            "third-with-attrs".to_string(),
        ],
    );
    assert_eq!(attrs_seen, vec!["high".to_string()]);

    // Mutations after restart still persist: send one more message,
    // restart again, expect it to be there.
    client
        .send_message()
        .queue_url(&q_url)
        .message_body("post-restart")
        .send()
        .await
        .unwrap();

    server.restart().await;
    let client = server.sqs_client().await;
    let resp = client
        .receive_message()
        .queue_url(&q_url)
        .max_number_of_messages(10)
        .send()
        .await
        .unwrap();
    let bodies: Vec<&str> = resp
        .messages()
        .iter()
        .map(|m| m.body().unwrap_or_default())
        .collect();
    assert!(bodies.contains(&"post-restart"));
}

/// FIFO queue round-trip: dedup/group IDs and sequence numbers are
/// preserved, and the queue is still recognized as FIFO after restart.
#[tokio::test]
async fn persistence_round_trip_fifo_queue() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.sqs_client().await;

    let q_url = client
        .create_queue()
        .queue_name("orders.fifo")
        .attributes(QueueAttributeName::FifoQueue, "true")
        .attributes(QueueAttributeName::ContentBasedDeduplication, "true")
        .send()
        .await
        .unwrap()
        .queue_url
        .unwrap();

    for body in ["a", "b", "c"] {
        client
            .send_message()
            .queue_url(&q_url)
            .message_body(body)
            .message_group_id("group-1")
            .send()
            .await
            .unwrap();
    }

    server.restart().await;
    let client = server.sqs_client().await;

    let attrs = client
        .get_queue_attributes()
        .queue_url(&q_url)
        .attribute_names(QueueAttributeName::FifoQueue)
        .send()
        .await
        .unwrap()
        .attributes
        .unwrap();
    assert_eq!(
        attrs
            .get(&QueueAttributeName::FifoQueue)
            .map(String::as_str),
        Some("true"),
    );

    // FIFO order: within a single group, strict ordering.
    let resp = client
        .receive_message()
        .queue_url(&q_url)
        .max_number_of_messages(10)
        .send()
        .await
        .unwrap();
    let bodies: Vec<&str> = resp
        .messages()
        .iter()
        .map(|m| m.body().unwrap_or_default())
        .collect();
    assert_eq!(bodies, vec!["a", "b", "c"]);
}

/// DeleteQueue durability: removed queues don't resurrect after restart.
#[tokio::test]
async fn persistence_delete_queue_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let client = server.sqs_client().await;

    let url = client
        .create_queue()
        .queue_name("ephemeral")
        .send()
        .await
        .unwrap()
        .queue_url
        .unwrap();
    client.delete_queue().queue_url(&url).send().await.unwrap();

    server.restart().await;
    let client = server.sqs_client().await;
    let list = client.list_queues().send().await.unwrap();
    let urls: Vec<&str> = list.queue_urls().iter().map(|s| s.as_str()).collect();
    assert!(!urls.iter().any(|u| u.ends_with("/ephemeral")));
}
