+++
title = "TypeScript SDK"
description = "Install and use the fakecloud SDK for TypeScript and JavaScript tests."
weight = 1
+++

## Install

```sh
npm install fakecloud
```

Works in Node.js and any environment with `fetch`. TypeScript types are bundled.

## Initialize

```typescript
import { FakeCloud } from "fakecloud";

const fc = new FakeCloud(); // defaults to http://localhost:4566
// or
const fc = new FakeCloud("http://localhost:5000");
```

## Top-level

| Method                  | Description             |
| ----------------------- | ----------------------- |
| `health()`              | Server health check     |
| `reset()`               | Reset all service state |
| `resetService(service)` | Reset a single service  |

## `fc.bedrock`

| Method                             | Description                                                          |
| ---------------------------------- | -------------------------------------------------------------------- |
| `getInvocations()`                 | List recorded Bedrock runtime invocations (each has `error` field)   |
| `setModelResponse(modelId, text)`  | Configure a single canned response for a model                       |
| `setResponseRules(modelId, rules)` | Replace prompt-conditional response rules for a model                |
| `clearResponseRules(modelId)`      | Clear all prompt-conditional response rules for a model              |
| `queueFault(rule)`                 | Queue a fault rule (e.g. `ThrottlingException`) for the next N calls |
| `getFaults()`                      | List currently queued fault rules                                    |
| `clearFaults()`                    | Clear all queued fault rules                                         |

## `fc.lambda`

| Method                         | Description                          |
| ------------------------------ | ------------------------------------ |
| `getInvocations()`             | List recorded Lambda invocations     |
| `getWarmContainers()`          | List warm (cached) Lambda containers |
| `evictContainer(functionName)` | Evict a warm container               |

## `fc.ses`

| Method                 | Description                               |
| ---------------------- | ----------------------------------------- |
| `getEmails()`          | List all sent emails                      |
| `simulateInbound(req)` | Simulate an inbound email (receipt rules) |

## `fc.sns`

| Method                      | Description                             |
| --------------------------- | --------------------------------------- |
| `getMessages()`             | List all published messages             |
| `getPendingConfirmations()` | List subscriptions pending confirmation |
| `confirmSubscription(req)`  | Confirm a pending subscription          |

## `fc.sqs`

| Method                | Description                           |
| --------------------- | ------------------------------------- |
| `getMessages()`       | List all messages across all queues   |
| `tickExpiration()`    | Tick the message expiration processor |
| `forceDlq(queueName)` | Force all messages to the queue's DLQ |

## `fc.events`

| Method          | Description                            |
| --------------- | -------------------------------------- |
| `getHistory()`  | Get event history and delivery records |
| `fireRule(req)` | Fire an EventBridge rule manually      |

## `fc.s3`

| Method               | Description                  |
| -------------------- | ---------------------------- |
| `getNotifications()` | List S3 notification events  |
| `tickLifecycle()`    | Tick the lifecycle processor |

## `fc.dynamodb`

| Method      | Description            |
| ----------- | ---------------------- |
| `tickTtl()` | Tick the TTL processor |

## `fc.secretsmanager`

| Method           | Description                 |
| ---------------- | --------------------------- |
| `tickRotation()` | Tick the rotation scheduler |

## `fc.cognito`

| Method                                                      | Description                              |
| ----------------------------------------------------------- | ---------------------------------------- |
| `getConfirmationCodes()`                                    | List all pending confirmation codes      |
| `getConfirmationCodesForUser(poolId, username)`             | Codes for a specific user                |
| `confirmUser(req)`                                          | Force-confirm a user                     |
| `getTokens()`                                               | List active tokens                       |
| `expireTokens(req)`                                         | Expire tokens for a pool/user            |
| `getAuthEvents()`                                           | List auth events                         |

## `fc.stepfunctions`

| Method           | Description             |
| ---------------- | ----------------------- |
| `getExecutions()`| List all executions     |

## `fc.rds`

| Method          | Description                       |
| --------------- | --------------------------------- |
| `getInstances()`| List managed RDS instances        |

## `fc.apigatewayv2`

| Method          | Description                       |
| --------------- | --------------------------------- |
| `getRequests()` | List recorded HTTP API requests   |

## Error handling

All methods throw `FakeCloudError` on non-2xx responses:

```typescript
import { FakeCloudError } from "fakecloud";

try {
  await fc.cognito.confirmUser({ userPoolId: "pool-1", username: "nobody" });
} catch (err) {
  if (err instanceof FakeCloudError) {
    console.log(err.status); // 404
    console.log(err.body);   // error body from fakecloud
  }
}
```

## Example: full test loop

```typescript
import { FakeCloud } from "fakecloud";
import { SQSClient, SendMessageCommand } from "@aws-sdk/client-sqs";

const fc = new FakeCloud();
const sqs = new SQSClient({
  endpoint: "http://localhost:4566",
  region: "us-east-1",
  credentials: { accessKeyId: "test", secretAccessKey: "test" },
});

beforeEach(() => fc.reset());

test("app publishes to SQS", async () => {
  await sqs.send(new SendMessageCommand({
    QueueUrl: "http://localhost:4566/000000000000/my-queue",
    MessageBody: "hello",
  }));

  const { messages } = await fc.sqs.getMessages();
  expect(messages).toHaveLength(1);
  expect(messages[0].body).toBe("hello");
});
```

## Source

- [`sdks/typescript`](https://github.com/faiscadev/fakecloud/tree/main/sdks/typescript)
- [Source README](https://github.com/faiscadev/fakecloud/blob/main/sdks/typescript/README.md) — always-current method list
