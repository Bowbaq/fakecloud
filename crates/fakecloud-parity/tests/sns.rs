mod common;

use common::{unique_name, Backend};

#[tokio::test]
async fn sns_create_topic_publish_to_sqs() {
    let backend = Backend::from_env().await;
    let sns = backend.sns().await;
    let sqs = backend.sqs().await;

    let topic_name = unique_name("sns");
    let queue_name = unique_name("sns-queue");

    // Create a queue to receive the SNS message.
    let queue = sqs
        .create_queue()
        .queue_name(&queue_name)
        .send()
        .await
        .expect("create_queue");
    let queue_url = queue.queue_url().expect("queue_url").to_string();

    let attrs = sqs
        .get_queue_attributes()
        .queue_url(&queue_url)
        .attribute_names(aws_sdk_sqs::types::QueueAttributeName::QueueArn)
        .send()
        .await
        .expect("get_queue_attributes");
    let queue_arn = attrs
        .attributes()
        .and_then(|m| m.get(&aws_sdk_sqs::types::QueueAttributeName::QueueArn))
        .expect("queue arn")
        .clone();

    // Create a topic.
    let topic = sns
        .create_topic()
        .name(&topic_name)
        .send()
        .await
        .expect("create_topic");
    let topic_arn = topic.topic_arn().expect("topic_arn").to_string();
    assert!(
        topic_arn.starts_with("arn:aws:sns:"),
        "topic arn should start with arn:aws:sns: ; got {topic_arn}"
    );
    assert!(
        topic_arn.ends_with(&topic_name),
        "topic arn should end with topic name; got {topic_arn}"
    );

    // Subscribe the SQS queue to the topic. Real AWS delivery requires an
    // SQS queue policy allowing sns.amazonaws.com to send; set one.
    // Wildcarding the Principal is fine because the queue name itself is
    // unique-per-run.
    let policy = format!(
        r#"{{"Version":"2012-10-17","Statement":[{{"Effect":"Allow","Principal":{{"AWS":"*"}},"Action":"sqs:SendMessage","Resource":"{queue_arn}","Condition":{{"ArnEquals":{{"aws:SourceArn":"{topic_arn}"}}}}}}]}}"#
    );
    sqs.set_queue_attributes()
        .queue_url(&queue_url)
        .attributes(aws_sdk_sqs::types::QueueAttributeName::Policy, policy)
        .send()
        .await
        .expect("set_queue_attributes policy");

    let sub = sns
        .subscribe()
        .topic_arn(&topic_arn)
        .protocol("sqs")
        .endpoint(&queue_arn)
        .send()
        .await
        .expect("subscribe");
    let subscription_arn = sub
        .subscription_arn()
        .expect("subscription_arn")
        .to_string();
    assert!(
        subscription_arn.starts_with("arn:aws:sns:"),
        "subscription arn should start with arn:aws:sns: ; got {subscription_arn}"
    );

    // Publish.
    let message = "hello sns parity";
    sns.publish()
        .topic_arn(&topic_arn)
        .message(message)
        .send()
        .await
        .expect("publish");

    // Poll the queue for the SNS delivery envelope.
    let recv = sqs
        .receive_message()
        .queue_url(&queue_url)
        .max_number_of_messages(1)
        .wait_time_seconds(20)
        .send()
        .await
        .expect("receive_message");
    let messages = recv.messages();
    assert_eq!(messages.len(), 1, "expected SNS-delivered message");
    let body = messages[0].body().unwrap_or_default();
    // SNS delivers a JSON envelope containing a "Message" field.
    let envelope: serde_json::Value =
        serde_json::from_str(body).expect("sns envelope should be JSON");
    assert_eq!(
        envelope
            .get("Message")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
        message
    );
    assert_eq!(
        envelope
            .get("TopicArn")
            .and_then(|v| v.as_str())
            .unwrap_or_default(),
        topic_arn
    );

    // Teardown.
    let _ = sns
        .unsubscribe()
        .subscription_arn(subscription_arn)
        .send()
        .await;
    let _ = sns.delete_topic().topic_arn(topic_arn).send().await;
    let _ = sqs.delete_queue().queue_url(queue_url).send().await;
}

#[tokio::test]
async fn sns_subscription_filter_policy() {
    let backend = Backend::from_env().await;
    let sns = backend.sns().await;
    let sqs = backend.sqs().await;

    let topic_name = unique_name("sns-filter");
    let queue_name = unique_name("sns-filter-q");

    let queue = sqs
        .create_queue()
        .queue_name(&queue_name)
        .send()
        .await
        .expect("create_queue");
    let queue_url = queue.queue_url().expect("queue_url").to_string();

    let attrs = sqs
        .get_queue_attributes()
        .queue_url(&queue_url)
        .attribute_names(aws_sdk_sqs::types::QueueAttributeName::QueueArn)
        .send()
        .await
        .expect("get_queue_attributes");
    let queue_arn = attrs
        .attributes()
        .and_then(|m| m.get(&aws_sdk_sqs::types::QueueAttributeName::QueueArn))
        .expect("queue arn")
        .clone();

    let topic = sns
        .create_topic()
        .name(&topic_name)
        .send()
        .await
        .expect("create_topic");
    let topic_arn = topic.topic_arn().expect("topic_arn").to_string();

    let policy = format!(
        r#"{{"Version":"2012-10-17","Statement":[{{"Effect":"Allow","Principal":{{"AWS":"*"}},"Action":"sqs:SendMessage","Resource":"{queue_arn}","Condition":{{"ArnEquals":{{"aws:SourceArn":"{topic_arn}"}}}}}}]}}"#
    );
    sqs.set_queue_attributes()
        .queue_url(&queue_url)
        .attributes(aws_sdk_sqs::types::QueueAttributeName::Policy, policy)
        .send()
        .await
        .expect("set_queue_attributes policy");

    // Subscribe with a filter policy that only allows messages whose
    // `event` attribute equals `wanted`.
    let sub = sns
        .subscribe()
        .topic_arn(&topic_arn)
        .protocol("sqs")
        .endpoint(&queue_arn)
        .attributes("FilterPolicy", r#"{"event":["wanted"]}"#)
        .send()
        .await
        .expect("subscribe");
    let subscription_arn = sub
        .subscription_arn()
        .expect("subscription_arn")
        .to_string();

    let wanted_attr = aws_sdk_sns::types::MessageAttributeValue::builder()
        .data_type("String")
        .string_value("wanted")
        .build()
        .unwrap();
    let other_attr = aws_sdk_sns::types::MessageAttributeValue::builder()
        .data_type("String")
        .string_value("other")
        .build()
        .unwrap();

    sns.publish()
        .topic_arn(&topic_arn)
        .message("should arrive")
        .message_attributes("event", wanted_attr)
        .send()
        .await
        .expect("publish matching");
    sns.publish()
        .topic_arn(&topic_arn)
        .message("should be filtered")
        .message_attributes("event", other_attr)
        .send()
        .await
        .expect("publish non-matching");

    let mut bodies: Vec<String> = Vec::new();
    for _ in 0..4 {
        let recv = sqs
            .receive_message()
            .queue_url(&queue_url)
            .max_number_of_messages(10)
            .wait_time_seconds(5)
            .send()
            .await
            .expect("receive_message");
        for msg in recv.messages() {
            let body: serde_json::Value =
                serde_json::from_str(msg.body().unwrap_or_default()).expect("sns envelope JSON");
            bodies.push(
                body.get("Message")
                    .and_then(|v| v.as_str())
                    .unwrap_or_default()
                    .to_string(),
            );
            if let Some(handle) = msg.receipt_handle() {
                let _ = sqs
                    .delete_message()
                    .queue_url(&queue_url)
                    .receipt_handle(handle)
                    .send()
                    .await;
            }
        }
    }

    assert_eq!(
        bodies,
        vec!["should arrive".to_string()],
        "filter policy should have dropped the non-matching message; got {bodies:?}"
    );

    let _ = sns
        .unsubscribe()
        .subscription_arn(subscription_arn)
        .send()
        .await;
    let _ = sns.delete_topic().topic_arn(topic_arn).send().await;
    let _ = sqs.delete_queue().queue_url(queue_url).send().await;
}
