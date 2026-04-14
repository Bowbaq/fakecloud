+++
title = "Python SDK"
description = "Install and use the fakecloud SDK for Python tests (sync and async)."
weight = 2
+++

## Install

```sh
pip install fakecloud
```

Works with Python 3.9+. Type hints bundled (mypy-compatible).

## Initialize (sync)

```python
from fakecloud import FakeCloud

fc = FakeCloud()  # defaults to http://localhost:4566
# or
fc = FakeCloud("http://localhost:5000")
```

## Initialize (async)

```python
from fakecloud import AsyncFakeCloud

async with AsyncFakeCloud() as fc:
    await fc.reset()
```

Both clients expose the same surface; only the call form differs (`fc.x.y()` vs `await fc.x.y()`).

## Top-level

| Method                   | Description             |
| ------------------------ | ----------------------- |
| `health()`               | Server health check     |
| `reset()`                | Reset all service state |
| `reset_service(service)` | Reset a single service  |

## `fc.bedrock`

| Method                                 | Description                                                          |
| -------------------------------------- | -------------------------------------------------------------------- |
| `get_invocations()`                    | List recorded Bedrock runtime invocations (each has `error` field)   |
| `set_model_response(model_id, text)`   | Configure a single canned response for a model                       |
| `set_response_rules(model_id, rules)`  | Replace prompt-conditional response rules                            |
| `clear_response_rules(model_id)`       | Clear all prompt-conditional response rules for a model              |
| `queue_fault(rule)`                    | Queue a fault rule for the next N matching calls                     |
| `get_faults()`                         | List currently queued fault rules                                    |
| `clear_faults()`                       | Clear all queued fault rules                                         |

## `fc.lambda_`

Note: `lambda` is a Python keyword, so the attribute is `lambda_`.

| Method                           | Description                          |
| -------------------------------- | ------------------------------------ |
| `get_invocations()`              | List recorded Lambda invocations     |
| `get_warm_containers()`          | List warm containers                 |
| `evict_container(function_name)` | Evict a warm container               |

## `fc.ses`

| Method                    | Description                               |
| ------------------------- | ----------------------------------------- |
| `get_emails()`            | List all sent emails                      |
| `simulate_inbound(req)`   | Simulate an inbound email (receipt rules) |

## `fc.sns`

| Method                         | Description                             |
| ------------------------------ | --------------------------------------- |
| `get_messages()`               | List all published messages             |
| `get_pending_confirmations()`  | List subscriptions pending confirmation |
| `confirm_subscription(req)`    | Confirm a pending subscription          |

## `fc.sqs`

| Method                   | Description                           |
| ------------------------ | ------------------------------------- |
| `get_messages()`         | List all messages across all queues   |
| `tick_expiration()`      | Tick the message expiration processor |
| `force_dlq(queue_name)`  | Force all messages to the queue's DLQ |

## `fc.events`

| Method           | Description                            |
| ---------------- | -------------------------------------- |
| `get_history()`  | Get event history                      |
| `fire_rule(req)` | Fire an EventBridge rule manually      |

## `fc.s3`

| Method                 | Description                  |
| ---------------------- | ---------------------------- |
| `get_notifications()`  | List S3 notification events  |
| `tick_lifecycle()`     | Tick the lifecycle processor |

## `fc.dynamodb`

| Method       | Description            |
| ------------ | ---------------------- |
| `tick_ttl()` | Tick the TTL processor |

## `fc.secretsmanager`

| Method            | Description                 |
| ----------------- | --------------------------- |
| `tick_rotation()` | Tick the rotation scheduler |

## `fc.cognito`

| Method                                                        | Description                     |
| ------------------------------------------------------------- | ------------------------------- |
| `get_confirmation_codes()`                                    | List all pending codes          |
| `get_confirmation_codes_for_user(pool_id, username)`          | Codes for a specific user       |
| `confirm_user(req)`                                           | Force-confirm a user            |
| `get_tokens()`                                                | List active tokens              |
| `expire_tokens(req)`                                          | Expire tokens for a pool/user   |
| `get_auth_events()`                                           | List auth events                |

## `fc.stepfunctions`

| Method             | Description             |
| ------------------ | ----------------------- |
| `get_executions()` | List all executions     |

## `fc.rds`

| Method            | Description                 |
| ----------------- | --------------------------- |
| `get_instances()` | List managed RDS instances  |

## `fc.apigatewayv2`

| Method           | Description                     |
| ---------------- | ------------------------------- |
| `get_requests()` | List recorded HTTP API requests |

## Error handling

Methods raise `FakeCloudError` on non-2xx responses:

```python
from fakecloud import FakeCloudError

try:
    fc.cognito.confirm_user({"userPoolId": "pool-1", "username": "nobody"})
except FakeCloudError as err:
    print(err.status)  # 404
    print(err.body)
```

## Example: pytest fixture

```python
import pytest
import boto3
from fakecloud import FakeCloud

@pytest.fixture
def fc():
    client = FakeCloud()
    yield client
    client.reset()

@pytest.fixture
def sqs():
    return boto3.client(
        "sqs",
        endpoint_url="http://localhost:4566",
        region_name="us-east-1",
        aws_access_key_id="test",
        aws_secret_access_key="test",
    )

def test_app_publishes_to_sqs(fc, sqs):
    sqs.send_message(
        QueueUrl="http://localhost:4566/000000000000/my-queue",
        MessageBody="hello",
    )

    messages = fc.sqs.get_messages()
    assert len(messages) == 1
    assert messages[0].body == "hello"
```

## Source

- [`sdks/python`](https://github.com/faiscadev/fakecloud/tree/main/sdks/python)
- [Source README](https://github.com/faiscadev/fakecloud/blob/main/sdks/python/README.md) — always-current method list
