+++
title = "Rust SDK"
description = "Install and use the fakecloud SDK for Rust tests."
weight = 4
+++

## Install

```sh
cargo add fakecloud-sdk
```

Rust 1.75+.

## Initialize

```rust
use fakecloud_sdk::FakeCloudClient;

let fc = FakeCloudClient::new("http://localhost:4566");
// or default:
let fc = FakeCloudClient::default();
```

## Top-level

| Method                                | Description             |
| ------------------------------------- | ----------------------- |
| `health().await`                      | Server health check     |
| `reset().await`                       | Reset all service state |
| `reset_service(service).await`        | Reset a single service  |

## `fc.bedrock()`

| Method                                              | Description                                       |
| --------------------------------------------------- | ------------------------------------------------- |
| `get_invocations().await`                           | List runtime invocations                          |
| `set_model_response(model_id, text).await`          | Single canned response                            |
| `set_response_rules(model_id, rules).await`         | Prompt-conditional rules                          |
| `clear_response_rules(model_id).await`              | Clear rules for a model                           |
| `queue_fault(rule).await`                           | Queue a fault rule                                |
| `get_faults().await`                                | List queued fault rules                           |
| `clear_faults().await`                              | Clear all queued fault rules                      |

## `fc.lambda()`, `fc.ses()`, `fc.sns()`, `fc.sqs()`, `fc.events()`, `fc.s3()`, `fc.dynamodb()`, `fc.secretsmanager()`, `fc.cognito()`, `fc.stepfunctions()`, `fc.rds()`, `fc.apigatewayv2()`

Each service accessor mirrors the TypeScript SDK's surface with Rust method naming (`get_messages`, `tick_ttl`, `fire_rule`, etc.). See the [TypeScript reference](/docs/sdks/typescript/) for the full method list.

The authoritative per-method list for Rust lives in the [`fakecloud-sdk` crate source](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-sdk).

## Error handling

All methods return `Result<T, FakeCloudError>`:

```rust
use fakecloud_sdk::FakeCloudError;

match fc.cognito().confirm_user("pool-1", "nobody").await {
    Ok(_) => {}
    Err(FakeCloudError::Http { status, body }) => {
        println!("status {}, body {}", status, body);
    }
    Err(e) => eprintln!("{}", e),
}
```

## Example: end-to-end test

```rust
use aws_sdk_sqs::Client;
use aws_config::BehaviorVersion;
use fakecloud_sdk::FakeCloudClient;

#[tokio::test]
async fn app_publishes_to_sqs() {
    let fc = FakeCloudClient::default();
    fc.reset().await.unwrap();

    let config = aws_config::defaults(BehaviorVersion::latest())
        .endpoint_url("http://localhost:4566")
        .region("us-east-1")
        .load()
        .await;
    let sqs = Client::new(&config);

    sqs.send_message()
        .queue_url("http://localhost:4566/000000000000/my-queue")
        .message_body("hello")
        .send()
        .await
        .unwrap();

    let messages = fc.sqs().get_messages().await.unwrap();
    assert_eq!(messages.len(), 1);
    assert_eq!(messages[0].body, "hello");
}
```

## Source

- [`crates/fakecloud-sdk`](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-sdk)
- [Source README](https://github.com/faiscadev/fakecloud/blob/main/crates/fakecloud-sdk/README.md) — always-current method list
