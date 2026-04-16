+++
title = "PHP SDK"
description = "Install and use the fakecloud SDK for PHP tests (PHPUnit, Pest, Laravel, Symfony)."
weight = 4
+++

## Install

```bash
composer require fakecloud/fakecloud
```

Requires PHP 8.1+. Uses the built-in `curl` extension and `json_decode`/`json_encode`. No external dependencies.

## Initialize

```php
use FakeCloud\FakeCloud;

$fc = new FakeCloud();                         // defaults to http://localhost:4566
$fc = new FakeCloud('http://localhost:5000');   // explicit base URL
```

## Top-level

| Method                   | Description             |
| ------------------------ | ----------------------- |
| `health()`               | Server health check     |
| `reset()`                | Reset all service state |
| `resetService($service)` | Reset a single service  |

## `$fc->bedrock()`

| Method                                | Description                                                          |
| ------------------------------------- | -------------------------------------------------------------------- |
| `getInvocations()`                    | List recorded Bedrock runtime invocations                            |
| `setModelResponse($modelId, $text)`   | Configure a single canned response for a model                       |
| `setResponseRules($modelId, $rules)`  | Replace prompt-conditional response rules for a model                |
| `clearResponseRules($modelId)`        | Clear all prompt-conditional response rules for a model              |
| `queueFault($rule)`                   | Queue a fault rule for the next N calls                              |
| `getFaults()`                         | List currently queued fault rules                                    |
| `clearFaults()`                       | Clear all queued fault rules                                         |

## `$fc->lambda()`, `$fc->ses()`, `$fc->sns()`, `$fc->sqs()`, `$fc->events()`, `$fc->s3()`, `$fc->dynamodb()`, `$fc->secretsmanager()`, `$fc->cognito()`, `$fc->stepfunctions()`, `$fc->rds()`, `$fc->elasticache()`, `$fc->apigatewayv2()`

Each sub-client mirrors the Java SDK method list 1:1. See the
[SDK README](https://github.com/faiscadev/fakecloud/blob/main/sdks/php/README.md)
for the full, always-current surface.

## Error handling

All methods throw `FakeCloudError` (a `RuntimeException`) on non-2xx responses:

```php
use FakeCloud\FakeCloudError;
use FakeCloud\ConfirmUserRequest;

try {
    $fc->cognito()->confirmUser(new ConfirmUserRequest('pool-1', 'nobody'));
} catch (FakeCloudError $err) {
    echo $err->status; // 404
    echo $err->body;   // error body from fakecloud
}
```

## Example: full test loop

```php
use FakeCloud\FakeCloud;
use FakeCloud\BedrockFaultRule;
use FakeCloud\BedrockResponseRule;

$fc = new FakeCloud();
$modelId = 'anthropic.claude-3-haiku-20240307-v1:0';

// PHPUnit setUp
$fc->reset();

// Test: classifier branches on spam vs ham
$fc->bedrock()->setResponseRules($modelId, [
    new BedrockResponseRule('buy now', '{"label":"spam"}'),
    new BedrockResponseRule(null, '{"label":"ham"}'),
]);

classify('hello friend');
classify('buy now cheap pills');

$invocations = $fc->bedrock()->getInvocations()->invocations;
$this->assertCount(2, $invocations);
$this->assertStringContainsString('ham', $invocations[0]->output);
$this->assertStringContainsString('spam', $invocations[1]->output);

// Test: retries on throttling
$fc->reset();
$fc->bedrock()->queueFault(
    new BedrockFaultRule('ThrottlingException', 'Rate exceeded', 429, 1)
);

classify('hello');

$invocations = $fc->bedrock()->getInvocations()->invocations;
$this->assertCount(2, $invocations);
$this->assertStringContainsString('ThrottlingException', $invocations[0]->error);
$this->assertNull($invocations[1]->error);
```

## Source

- [`sdks/php`](https://github.com/faiscadev/fakecloud/tree/main/sdks/php)
- [Source README](https://github.com/faiscadev/fakecloud/blob/main/sdks/php/README.md) — always-current method list
