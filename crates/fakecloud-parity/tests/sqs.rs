mod common;

use common::{unique_name, Backend};

#[tokio::test]
async fn sqs_create_send_receive_delete() {
    let backend = Backend::from_env().await;
    let sqs = backend.sqs().await;
    let queue_name = unique_name("sqs");

    // Create.
    let create = sqs
        .create_queue()
        .queue_name(&queue_name)
        .send()
        .await
        .expect("create_queue");
    let queue_url = create.queue_url().expect("queue_url returned").to_string();
    assert!(
        queue_url.ends_with(&queue_name),
        "queue url should end with queue name; got {queue_url}"
    );

    // GetQueueAttributes sanity: ARN is present and starts with arn:aws:sqs:.
    let attrs = sqs
        .get_queue_attributes()
        .queue_url(&queue_url)
        .attribute_names(aws_sdk_sqs::types::QueueAttributeName::QueueArn)
        .send()
        .await
        .expect("get_queue_attributes");
    let arn = attrs
        .attributes()
        .and_then(|m| m.get(&aws_sdk_sqs::types::QueueAttributeName::QueueArn))
        .expect("queue arn attribute");
    assert!(
        arn.starts_with("arn:aws:sqs:"),
        "queue arn should start with arn:aws:sqs: ; got {arn}"
    );
    assert!(
        arn.ends_with(&queue_name),
        "queue arn should end with queue name; got {arn}"
    );

    // Send -> Receive -> Delete a message.
    let body = "hello from parity";
    sqs.send_message()
        .queue_url(&queue_url)
        .message_body(body)
        .send()
        .await
        .expect("send_message");

    // Real SQS takes a short moment for the message to become visible.
    // 20-second long poll is the right primitive for this.
    let recv = sqs
        .receive_message()
        .queue_url(&queue_url)
        .max_number_of_messages(1)
        .wait_time_seconds(20)
        .send()
        .await
        .expect("receive_message");
    let messages = recv.messages();
    assert_eq!(
        messages.len(),
        1,
        "expected 1 message, got {}",
        messages.len()
    );
    let msg = &messages[0];
    assert_eq!(msg.body().unwrap_or_default(), body);

    let receipt = msg.receipt_handle().expect("receipt_handle").to_string();
    sqs.delete_message()
        .queue_url(&queue_url)
        .receipt_handle(receipt)
        .send()
        .await
        .expect("delete_message");

    // Teardown.
    sqs.delete_queue()
        .queue_url(&queue_url)
        .send()
        .await
        .expect("delete_queue");
}

#[tokio::test]
async fn sqs_get_queue_url_nonexistent_returns_expected_error() {
    let backend = Backend::from_env().await;
    let sqs = backend.sqs().await;
    let name = unique_name("sqs-missing");

    let err = sqs
        .get_queue_url()
        .queue_name(&name)
        .send()
        .await
        .expect_err("get_queue_url on nonexistent queue should fail");

    // Both fakecloud and real SQS should return
    // AWS.SimpleQueueService.NonExistentQueue (error code). The SDK maps
    // this to `QueueDoesNotExist` on the operation error. We assert the
    // raw code string so shape changes in the SDK don't break us.
    let service_err = err.into_service_error();
    let code = service_err.meta().code().unwrap_or_default();
    assert!(
        code == "AWS.SimpleQueueService.NonExistentQueue" || code == "QueueDoesNotExist",
        "expected NonExistentQueue / QueueDoesNotExist, got code={code:?}"
    );
}

#[tokio::test]
async fn sqs_fifo_dedup_and_ordering() {
    let backend = Backend::from_env().await;
    let sqs = backend.sqs().await;
    // FIFO queue names must end with `.fifo`.
    let queue_name = format!("{}.fifo", unique_name("sqs-fifo"));

    let create = sqs
        .create_queue()
        .queue_name(&queue_name)
        .attributes(aws_sdk_sqs::types::QueueAttributeName::FifoQueue, "true")
        .attributes(
            aws_sdk_sqs::types::QueueAttributeName::ContentBasedDeduplication,
            "true",
        )
        .send()
        .await
        .expect("create_queue fifo");
    let queue_url = create.queue_url().expect("queue_url").to_string();

    // Send three messages to the same group. Send "alpha" twice with the
    // same body -- content-based dedup should drop the second.
    for body in ["alpha", "alpha", "beta"] {
        sqs.send_message()
            .queue_url(&queue_url)
            .message_body(body)
            .message_group_id("grp-1")
            .send()
            .await
            .expect("send_message fifo");
    }

    // Receive both surviving messages. FIFO preserves send order within a
    // group, so alpha should arrive before beta.
    let mut received: Vec<String> = Vec::new();
    for _ in 0..5 {
        if received.len() >= 2 {
            break;
        }
        let recv = sqs
            .receive_message()
            .queue_url(&queue_url)
            .max_number_of_messages(10)
            .wait_time_seconds(5)
            .send()
            .await
            .expect("receive_message fifo");
        for msg in recv.messages() {
            received.push(msg.body().unwrap_or_default().to_string());
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
        received,
        vec!["alpha".to_string(), "beta".to_string()],
        "expected FIFO order alpha,beta after content-based dedup; got {received:?}"
    );

    sqs.delete_queue()
        .queue_url(queue_url)
        .send()
        .await
        .expect("delete_queue");
}
