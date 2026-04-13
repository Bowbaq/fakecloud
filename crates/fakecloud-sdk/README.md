# fakecloud-sdk

Rust client SDK for [fakecloud](https://github.com/faiscadev/fakecloud), a local AWS cloud emulator.

The crate wraps fakecloud's introspection and simulation API (`/_fakecloud/*`) so Rust tests can inspect emulator state, reset services, and trigger time-based processors without going through raw HTTP calls.

## Installation

```bash
cargo add fakecloud-sdk
```

## Quick Start

```rust
use fakecloud_sdk::FakeCloud;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let fc = FakeCloud::new("http://localhost:4566");

    let health = fc.health().await?;
    println!("{}", health.version);

    fc.reset().await?;

    let emails = fc.ses().get_emails().await?;
    println!("sent {} emails", emails.emails.len());

    Ok(())
}
```

## What It Covers

- health and reset endpoints
- SES email inspection and inbound simulation
- SNS and SQS message inspection
- EventBridge history and manual rule firing
- S3 notifications and lifecycle ticks
- DynamoDB TTL and Secrets Manager rotation ticks
- Lambda invocation and warm-container inspection
- Cognito confirmation codes, token inspection, and auth event access
- RDS instance inspection with runtime metadata
- ElastiCache cluster, replication group, and serverless cache inspection
- Step Functions execution history
- API Gateway v2 HTTP API request history
- Bedrock runtime invocation inspection, prompt-conditional response rules, and fault injection

## Testing Bedrock-calling code

```rust
use fakecloud_sdk::FakeCloud;
use fakecloud_sdk::types::{BedrockFaultRule, BedrockResponseRule};

#[tokio::test]
async fn classifier_branches_on_spam_vs_ham() {
    let fc = FakeCloud::new("http://localhost:4566");
    fc.reset().await.unwrap();

    let model_id = "anthropic.claude-3-haiku-20240307-v1:0";
    fc.bedrock()
        .set_response_rules(
            model_id,
            &[
                BedrockResponseRule {
                    prompt_contains: Some("buy now".into()),
                    response: r#"{"label":"spam"}"#.into(),
                },
                BedrockResponseRule {
                    prompt_contains: None, // catch-all
                    response: r#"{"label":"ham"}"#.into(),
                },
            ],
        )
        .await
        .unwrap();

    classify("hello friend").await;         // user code calls Bedrock
    classify("buy now cheap pills").await;

    let invs = fc.bedrock().get_invocations().await.unwrap();
    assert_eq!(invs.invocations.len(), 2);
    assert!(invs.invocations[0].output.contains("ham"));
    assert!(invs.invocations[1].output.contains("spam"));
}

#[tokio::test]
async fn retries_on_throttling() {
    let fc = FakeCloud::new("http://localhost:4566");
    fc.reset().await.unwrap();

    fc.bedrock()
        .queue_fault(&BedrockFaultRule {
            error_type: "ThrottlingException".into(),
            message: Some("Rate exceeded".into()),
            http_status: Some(429),
            count: Some(1), // only the first call faults; the retry succeeds
            ..Default::default()
        })
        .await
        .unwrap();

    classify("hello").await;

    let invs = fc.bedrock().get_invocations().await.unwrap();
    assert_eq!(invs.invocations.len(), 2);
    assert!(invs.invocations[0]
        .error
        .as_deref()
        .unwrap_or("")
        .contains("ThrottlingException"));
    assert!(invs.invocations[1].error.is_none());
}
```

## Repository

- Project: <https://github.com/faiscadev/fakecloud>
- Website: <https://fakecloud.dev>
