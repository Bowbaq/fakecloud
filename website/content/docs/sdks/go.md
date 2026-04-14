+++
title = "Go SDK"
description = "Install and use the fakecloud SDK for Go tests."
weight = 3
+++

## Install

```sh
go get github.com/faiscadev/fakecloud/sdks/go
```

Go 1.21+.

## Initialize

```go
import "github.com/faiscadev/fakecloud/sdks/go/fakecloud"

fc := fakecloud.New("http://localhost:4566")
// or with default URL:
fc := fakecloud.NewDefault() // http://localhost:4566
```

## Top-level

| Method                      | Description             |
| --------------------------- | ----------------------- |
| `Health() error`            | Server health check     |
| `Reset() error`             | Reset all service state |
| `ResetService(svc) error`   | Reset a single service  |

## `fc.Bedrock`

| Method                                       | Description                                       |
| -------------------------------------------- | ------------------------------------------------- |
| `GetInvocations()`                           | List runtime invocations (with `Error` field)     |
| `SetModelResponse(modelID, text)`            | Single canned response for a model                |
| `SetResponseRules(modelID, rules)`           | Prompt-conditional rules                          |
| `ClearResponseRules(modelID)`                | Clear rules for a model                           |
| `QueueFault(rule)`                           | Queue a fault rule                                |
| `GetFaults()`                                | List queued fault rules                           |
| `ClearFaults()`                              | Clear all queued fault rules                      |

## `fc.Lambda`

| Method                              | Description                          |
| ----------------------------------- | ------------------------------------ |
| `GetInvocations()`                  | List Lambda invocations              |
| `GetWarmContainers()`               | List warm containers                 |
| `EvictContainer(functionName)`      | Evict a warm container               |

## `fc.SES`, `fc.SNS`, `fc.SQS`, `fc.Events`, `fc.S3`, `fc.DynamoDB`, `fc.SecretsManager`, `fc.Cognito`, `fc.StepFunctions`, `fc.RDS`, `fc.APIGatewayV2`

Each mirrors the TypeScript SDK's surface with Go method naming (`GetMessages`, `TickTTL`, `FireRule`, etc.). See the [TypeScript reference](/docs/sdks/typescript/) for the full method list — the semantics are identical.

The authoritative per-method list for Go lives in the [Go SDK source README](https://github.com/faiscadev/fakecloud/blob/main/sdks/go/README.md).

## Error handling

Methods return `*FakeCloudError` on non-2xx responses:

```go
_, err := fc.Cognito.ConfirmUser(ConfirmUserRequest{
    UserPoolID: "pool-1",
    Username:   "nobody",
})
if err != nil {
    var fcErr *fakecloud.FakeCloudError
    if errors.As(err, &fcErr) {
        fmt.Println(fcErr.Status) // 404
        fmt.Println(fcErr.Body)
    }
}
```

## Example: end-to-end test

```go
package main_test

import (
    "context"
    "testing"

    "github.com/aws/aws-sdk-go-v2/aws"
    "github.com/aws/aws-sdk-go-v2/config"
    "github.com/aws/aws-sdk-go-v2/service/sqs"
    "github.com/faiscadev/fakecloud/sdks/go/fakecloud"
)

func TestAppPublishesToSQS(t *testing.T) {
    fc := fakecloud.NewDefault()
    if err := fc.Reset(); err != nil {
        t.Fatal(err)
    }

    cfg, _ := config.LoadDefaultConfig(context.TODO(),
        config.WithRegion("us-east-1"),
        config.WithCredentialsProvider(aws.AnonymousCredentials{}),
    )
    sqsClient := sqs.NewFromConfig(cfg, func(o *sqs.Options) {
        o.BaseEndpoint = aws.String("http://localhost:4566")
    })

    _, err := sqsClient.SendMessage(context.TODO(), &sqs.SendMessageInput{
        QueueUrl:    aws.String("http://localhost:4566/000000000000/my-queue"),
        MessageBody: aws.String("hello"),
    })
    if err != nil {
        t.Fatal(err)
    }

    messages, err := fc.SQS.GetMessages()
    if err != nil {
        t.Fatal(err)
    }
    if len(messages) != 1 {
        t.Fatalf("expected 1 message, got %d", len(messages))
    }
    if messages[0].Body != "hello" {
        t.Fatalf("expected body 'hello', got %q", messages[0].Body)
    }
}
```

## Source

- [`sdks/go`](https://github.com/faiscadev/fakecloud/tree/main/sdks/go)
- [Source README](https://github.com/faiscadev/fakecloud/blob/main/sdks/go/README.md) — always-current method list
