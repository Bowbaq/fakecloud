mod helpers;

use std::collections::HashMap;

use helpers::TestServer;

/// Standard topic round-trip: attributes, tags, subscriptions (standard +
/// SQS target), and subscription attributes survive a restart.
#[tokio::test]
async fn persistence_round_trip_topic_and_subscriptions() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let sns = server.sns_client().await;
    let sqs = server.sqs_client().await;

    // Create a standard topic.
    let topic = sns.create_topic().name("orders").send().await.unwrap();
    let topic_arn = topic.topic_arn().unwrap().to_string();

    // Set topic attributes.
    sns.set_topic_attributes()
        .topic_arn(&topic_arn)
        .attribute_name("DisplayName")
        .attribute_value("Orders Topic")
        .send()
        .await
        .unwrap();

    // Tags.
    sns.tag_resource()
        .resource_arn(&topic_arn)
        .tags(
            aws_sdk_sns::types::Tag::builder()
                .key("env")
                .value("prod")
                .build()
                .unwrap(),
        )
        .send()
        .await
        .unwrap();

    // Subscribe an SQS queue to the topic.
    let q = sqs
        .create_queue()
        .queue_name("orders-downstream")
        .send()
        .await
        .unwrap()
        .queue_url
        .unwrap();
    let q_arn = sqs
        .get_queue_attributes()
        .queue_url(&q)
        .attribute_names(aws_sdk_sqs::types::QueueAttributeName::QueueArn)
        .send()
        .await
        .unwrap()
        .attributes
        .unwrap()
        .get(&aws_sdk_sqs::types::QueueAttributeName::QueueArn)
        .unwrap()
        .clone();

    let sub_arn = sns
        .subscribe()
        .topic_arn(&topic_arn)
        .protocol("sqs")
        .endpoint(&q_arn)
        .attributes("RawMessageDelivery", "true")
        .return_subscription_arn(true)
        .send()
        .await
        .unwrap()
        .subscription_arn
        .unwrap();

    server.restart().await;
    let sns = server.sns_client().await;

    // Topic survives.
    let topics = sns.list_topics().send().await.unwrap();
    assert!(topics
        .topics()
        .iter()
        .any(|t| t.topic_arn() == Some(topic_arn.as_str())));

    // Topic attributes survive.
    let attrs = sns
        .get_topic_attributes()
        .topic_arn(&topic_arn)
        .send()
        .await
        .unwrap();
    let am: HashMap<String, String> = attrs.attributes().unwrap_or(&HashMap::new()).clone();
    assert_eq!(
        am.get("DisplayName").map(String::as_str),
        Some("Orders Topic")
    );

    // Tags survive.
    let tags = sns
        .list_tags_for_resource()
        .resource_arn(&topic_arn)
        .send()
        .await
        .unwrap();
    let tag_map: HashMap<String, String> = tags
        .tags()
        .iter()
        .map(|t| (t.key().to_string(), t.value().to_string()))
        .collect();
    assert_eq!(tag_map.get("env").map(String::as_str), Some("prod"));

    // Subscription survives, with RawMessageDelivery=true still set.
    let subs = sns
        .list_subscriptions_by_topic()
        .topic_arn(&topic_arn)
        .send()
        .await
        .unwrap();
    assert!(subs
        .subscriptions()
        .iter()
        .any(|s| s.subscription_arn() == Some(sub_arn.as_str())));
    let sub_attrs = sns
        .get_subscription_attributes()
        .subscription_arn(&sub_arn)
        .send()
        .await
        .unwrap()
        .attributes
        .unwrap_or_default();
    assert_eq!(
        sub_attrs.get("RawMessageDelivery").map(String::as_str),
        Some("true"),
    );
}

/// FIFO topic persists and is still recognized as FIFO after restart.
#[tokio::test]
async fn persistence_round_trip_fifo_topic() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let sns = server.sns_client().await;

    let topic_arn = sns
        .create_topic()
        .name("events.fifo")
        .attributes("FifoTopic", "true")
        .attributes("ContentBasedDeduplication", "true")
        .send()
        .await
        .unwrap()
        .topic_arn
        .unwrap();

    server.restart().await;
    let sns = server.sns_client().await;

    let attrs = sns
        .get_topic_attributes()
        .topic_arn(&topic_arn)
        .send()
        .await
        .unwrap()
        .attributes
        .unwrap_or_default();
    assert_eq!(attrs.get("FifoTopic").map(String::as_str), Some("true"));
    assert_eq!(
        attrs.get("ContentBasedDeduplication").map(String::as_str),
        Some("true"),
    );
}

/// Unsubscribe + delete-topic durability.
#[tokio::test]
async fn persistence_delete_topic_survives_restart() {
    let tmp = tempfile::tempdir().unwrap();
    let mut server = TestServer::start_persistent(tmp.path()).await;
    let sns = server.sns_client().await;

    let topic_arn = sns
        .create_topic()
        .name("ephemeral")
        .send()
        .await
        .unwrap()
        .topic_arn
        .unwrap();
    sns.delete_topic()
        .topic_arn(&topic_arn)
        .send()
        .await
        .unwrap();

    server.restart().await;
    let sns = server.sns_client().await;
    let topics = sns.list_topics().send().await.unwrap();
    assert!(!topics
        .topics()
        .iter()
        .any(|t| t.topic_arn() == Some(topic_arn.as_str())));
}
