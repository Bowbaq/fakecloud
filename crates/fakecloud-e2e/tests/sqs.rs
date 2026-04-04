mod helpers;

use aws_sdk_sqs::types::{
    DeleteMessageBatchRequestEntry, MessageAttributeValue, QueueAttributeName,
    SendMessageBatchRequestEntry,
};
use helpers::TestServer;
use std::time::Duration;

#[tokio::test]
async fn sqs_create_list_delete_queue() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    // Create
    let resp = client
        .create_queue()
        .queue_name("test-queue")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();
    assert!(queue_url.contains("test-queue"));

    // List
    let resp = client.list_queues().send().await.unwrap();
    let urls = resp.queue_urls();
    assert_eq!(urls.len(), 1);
    assert_eq!(urls[0], queue_url);

    // Delete
    client
        .delete_queue()
        .queue_url(&queue_url)
        .send()
        .await
        .unwrap();

    let resp = client.list_queues().send().await.unwrap();
    assert_eq!(resp.queue_urls().len(), 0);
}

#[tokio::test]
async fn sqs_send_receive_delete_message() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp = client
        .create_queue()
        .queue_name("msg-queue")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();

    // Send
    client
        .send_message()
        .queue_url(&queue_url)
        .message_body("hello world")
        .send()
        .await
        .unwrap();

    // Receive
    let resp = client
        .receive_message()
        .queue_url(&queue_url)
        .max_number_of_messages(10)
        .send()
        .await
        .unwrap();

    let messages = resp.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body().unwrap(), "hello world");

    let receipt_handle = messages[0].receipt_handle().unwrap().to_string();

    // Delete message
    client
        .delete_message()
        .queue_url(&queue_url)
        .receipt_handle(&receipt_handle)
        .send()
        .await
        .unwrap();
}

#[tokio::test]
async fn sqs_multiple_messages() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp = client
        .create_queue()
        .queue_name("multi-queue")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();

    for i in 0..5 {
        client
            .send_message()
            .queue_url(&queue_url)
            .message_body(format!("message {i}"))
            .send()
            .await
            .unwrap();
    }

    let resp = client
        .receive_message()
        .queue_url(&queue_url)
        .max_number_of_messages(10)
        .send()
        .await
        .unwrap();

    assert_eq!(resp.messages().len(), 5);
}

#[tokio::test]
async fn sqs_get_queue_url() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let create_resp = client
        .create_queue()
        .queue_name("url-queue")
        .send()
        .await
        .unwrap();
    let expected_url = create_resp.queue_url().unwrap();

    let resp = client
        .get_queue_url()
        .queue_name("url-queue")
        .send()
        .await
        .unwrap();
    assert_eq!(resp.queue_url().unwrap(), expected_url);
}

#[tokio::test]
async fn sqs_purge_queue() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp = client
        .create_queue()
        .queue_name("purge-queue")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();

    client
        .send_message()
        .queue_url(&queue_url)
        .message_body("to be purged")
        .send()
        .await
        .unwrap();

    client
        .purge_queue()
        .queue_url(&queue_url)
        .send()
        .await
        .unwrap();

    let resp = client
        .receive_message()
        .queue_url(&queue_url)
        .send()
        .await
        .unwrap();
    assert_eq!(resp.messages().len(), 0);
}

#[tokio::test]
async fn sqs_create_queue_idempotent() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp1 = client
        .create_queue()
        .queue_name("idempotent-queue")
        .send()
        .await
        .unwrap();
    let resp2 = client
        .create_queue()
        .queue_name("idempotent-queue")
        .send()
        .await
        .unwrap();

    assert_eq!(resp1.queue_url().unwrap(), resp2.queue_url().unwrap());
}

#[tokio::test]
async fn sqs_cli_create_and_list() {
    let server = TestServer::start().await;

    let output = server
        .aws_cli(&["sqs", "create-queue", "--queue-name", "cli-queue"])
        .await;
    assert!(output.success(), "create failed: {}", output.stderr_text());

    let output = server.aws_cli(&["sqs", "list-queues"]).await;
    assert!(output.success(), "list failed: {}", output.stderr_text());
    let json = output.stdout_json();
    let urls = json["QueueUrls"].as_array().unwrap();
    assert_eq!(urls.len(), 1);
    assert!(urls[0].as_str().unwrap().contains("cli-queue"));
}

#[tokio::test]
async fn sqs_send_message_batch() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp = client
        .create_queue()
        .queue_name("batch-send-queue")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();

    let entries: Vec<SendMessageBatchRequestEntry> = (0..3)
        .map(|i| {
            SendMessageBatchRequestEntry::builder()
                .id(format!("msg-{i}"))
                .message_body(format!("batch message {i}"))
                .build()
                .unwrap()
        })
        .collect();

    let batch_resp = client
        .send_message_batch()
        .queue_url(&queue_url)
        .set_entries(Some(entries))
        .send()
        .await
        .unwrap();

    assert_eq!(batch_resp.successful().len(), 3);
    assert!(batch_resp.failed().is_empty());

    // Verify all messages are receivable
    let recv_resp = client
        .receive_message()
        .queue_url(&queue_url)
        .max_number_of_messages(10)
        .send()
        .await
        .unwrap();
    assert_eq!(recv_resp.messages().len(), 3);
}

#[tokio::test]
async fn sqs_delete_message_batch() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp = client
        .create_queue()
        .queue_name("batch-delete-queue")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();

    // Send 3 messages
    for i in 0..3 {
        client
            .send_message()
            .queue_url(&queue_url)
            .message_body(format!("delete me {i}"))
            .send()
            .await
            .unwrap();
    }

    // Receive all messages
    let recv_resp = client
        .receive_message()
        .queue_url(&queue_url)
        .max_number_of_messages(10)
        .send()
        .await
        .unwrap();
    let messages = recv_resp.messages();
    assert_eq!(messages.len(), 3);

    // Delete them in a batch
    let entries: Vec<DeleteMessageBatchRequestEntry> = messages
        .iter()
        .enumerate()
        .map(|(i, m)| {
            DeleteMessageBatchRequestEntry::builder()
                .id(format!("del-{i}"))
                .receipt_handle(m.receipt_handle().unwrap())
                .build()
                .unwrap()
        })
        .collect();

    let del_resp = client
        .delete_message_batch()
        .queue_url(&queue_url)
        .set_entries(Some(entries))
        .send()
        .await
        .unwrap();

    assert_eq!(del_resp.successful().len(), 3);
    assert!(del_resp.failed().is_empty());
}

#[tokio::test]
async fn sqs_set_queue_attributes() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp = client
        .create_queue()
        .queue_name("attrs-queue")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();

    // Set attributes
    client
        .set_queue_attributes()
        .queue_url(&queue_url)
        .attributes(QueueAttributeName::VisibilityTimeout, "60")
        .attributes(QueueAttributeName::DelaySeconds, "5")
        .send()
        .await
        .unwrap();

    // Get and verify
    let attrs_resp = client
        .get_queue_attributes()
        .queue_url(&queue_url)
        .attribute_names(QueueAttributeName::All)
        .send()
        .await
        .unwrap();

    let attrs = attrs_resp.attributes().unwrap();
    assert_eq!(
        attrs.get(&QueueAttributeName::VisibilityTimeout).unwrap(),
        "60"
    );
    assert_eq!(attrs.get(&QueueAttributeName::DelaySeconds).unwrap(), "5");
}

#[tokio::test]
async fn sqs_message_attributes() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp = client
        .create_queue()
        .queue_name("msg-attrs-queue")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();

    // Send message with attributes
    let attr_value = MessageAttributeValue::builder()
        .data_type("String")
        .string_value("test-value")
        .build()
        .unwrap();

    client
        .send_message()
        .queue_url(&queue_url)
        .message_body("hello with attrs")
        .message_attributes("my-attr", attr_value)
        .send()
        .await
        .unwrap();

    // Receive and verify attributes
    let recv_resp = client
        .receive_message()
        .queue_url(&queue_url)
        .message_attribute_names("All")
        .send()
        .await
        .unwrap();

    let messages = recv_resp.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body().unwrap(), "hello with attrs");

    let msg_attrs = messages[0].message_attributes().unwrap();
    let attr = msg_attrs.get("my-attr").unwrap();
    assert_eq!(attr.data_type(), "String");
    assert_eq!(attr.string_value().unwrap(), "test-value");
}

#[tokio::test]
async fn sqs_fifo_queue_ordering() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp = client
        .create_queue()
        .queue_name("ordering.fifo")
        .attributes(QueueAttributeName::FifoQueue, "true")
        .attributes(QueueAttributeName::ContentBasedDeduplication, "true")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();

    // Send messages with MessageGroupId
    for i in 0..3 {
        client
            .send_message()
            .queue_url(&queue_url)
            .message_body(format!("fifo-msg-{i}"))
            .message_group_id("group-1")
            .send()
            .await
            .unwrap();
    }

    let recv = client
        .receive_message()
        .queue_url(&queue_url)
        .max_number_of_messages(10)
        .send()
        .await
        .unwrap();

    let messages = recv.messages();
    assert_eq!(messages.len(), 3);
    assert_eq!(messages[0].body().unwrap(), "fifo-msg-0");
    assert_eq!(messages[1].body().unwrap(), "fifo-msg-1");
    assert_eq!(messages[2].body().unwrap(), "fifo-msg-2");
}

#[tokio::test]
async fn sqs_fifo_queue_missing_group_id() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp = client
        .create_queue()
        .queue_name("no-group.fifo")
        .attributes(QueueAttributeName::FifoQueue, "true")
        .attributes(QueueAttributeName::ContentBasedDeduplication, "true")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();

    // Send without MessageGroupId should fail
    let result = client
        .send_message()
        .queue_url(&queue_url)
        .message_body("should fail")
        .send()
        .await;

    assert!(
        result.is_err(),
        "Expected error when missing MessageGroupId on FIFO queue"
    );
}

#[tokio::test]
async fn sqs_long_polling_wait_time_seconds() {
    let server = TestServer::start().await;
    let client = server.sqs_client().await;

    let resp = client
        .create_queue()
        .queue_name("longpoll-queue")
        .send()
        .await
        .unwrap();
    let queue_url = resp.queue_url().unwrap().to_string();

    // Spawn a task that sends a message after a short delay
    let send_client = server.sqs_client().await;
    let send_url = queue_url.clone();
    tokio::spawn(async move {
        tokio::time::sleep(Duration::from_millis(500)).await;
        send_client
            .send_message()
            .queue_url(&send_url)
            .message_body("delayed hello")
            .send()
            .await
            .unwrap();
    });

    // Use WaitTimeSeconds to long poll - should pick up the message
    let recv = client
        .receive_message()
        .queue_url(&queue_url)
        .wait_time_seconds(5)
        .send()
        .await
        .unwrap();

    let messages = recv.messages();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body().unwrap(), "delayed hello");
}
