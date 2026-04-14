+++
title = "First test"
description = "Run your first AWS integration test against fakecloud."
weight = 2
+++

This page walks through a minimal end-to-end test that uses the real AWS SDK to talk to fakecloud and asserts on the result with a first-party fakecloud SDK.

## 1. Start fakecloud

```sh
fakecloud
```

It listens on `http://localhost:4566`. Leave it running in a terminal.

## 2. Use dummy credentials

fakecloud never validates credentials, but most AWS SDKs error out if none are set. Export any placeholder values:

```sh
export AWS_ACCESS_KEY_ID=test
export AWS_SECRET_ACCESS_KEY=test
export AWS_REGION=us-east-1
```

## 3. Talk to it from the AWS CLI

```sh
aws --endpoint-url http://localhost:4566 sqs create-queue --queue-name demo
aws --endpoint-url http://localhost:4566 sqs send-message \
    --queue-url http://localhost:4566/000000000000/demo \
    --message-body "hello"
```

## 4. Write an actual test

TypeScript with Vitest:

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

test("sends a message that shows up in introspection", async () => {
  await sqs.send(
    new SendMessageCommand({
      QueueUrl: "http://localhost:4566/000000000000/demo",
      MessageBody: "hello from the test",
    })
  );

  const { messages } = await fc.sqs.getMessages();
  expect(messages).toHaveLength(1);
  expect(messages[0].body).toBe("hello from the test");
});
```

The pattern: your app sends via the real AWS SDK, your test asserts via the fakecloud SDK. Two different clients, two different jobs.

## Next

Set up the first-party SDK in your language — see [SDK setup](/docs/getting-started/sdk-setup/).
