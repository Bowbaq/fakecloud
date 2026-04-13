# Roadmap

fakecloud's goal is to be the best free AWS emulator for integration testing and local development. This roadmap outlines what's coming next.

For every service we implement, the standard is the same: full API coverage, real behavior (not stubs), conformance testing against AWS Smithy models, and cross-service integrations where applicable.

## Recently shipped

### S3 persistence

Opt-in disk persistence for S3 via `--storage-mode=persistent --data-path=<dir>`.
Buckets, objects, versions, delete markers, multipart uploads (resumable across
restarts), and every bucket subresource are written to disk on every mutation
and reloaded on startup. Object bodies stream straight to disk with a bounded
LRU cache (`--s3-cache-size`, default 256 MiB) so a single large upload can
never pull the entire working set into RAM. Memory mode stays the default and
is unchanged. Follow-up: extend persistence to DynamoDB, SQS, and the other
stateful services.

### API Gateway v2

HTTP APIs with Lambda proxy integration, route-based request handling, path parameters, wildcards, CORS, JWT and Lambda authorizers. 28 operations, full Lambda proxy integration v2.0 format, HTTP proxy and Mock integrations. Completes the serverless stack: API Gateway → Lambda → Step Functions.

### Step Functions

Complete AWS Step Functions implementation with full Amazon States Language (ASL) interpreter. All state types: Task, Pass, Choice, Wait, Parallel, Map, Succeed, Fail. Error handling with Retry and Catch. Cross-service task integrations: Lambda, SQS, SNS, EventBridge, DynamoDB. 14 operations, 60 E2E tests, 100% conformance.

### Kinesis

Kinesis Data Streams with full streaming API support. Put records, consume via shard iterators, manage stream retention and scaling. DynamoDB Streams → Lambda event source mappings and DynamoDB → Kinesis streaming are also shipped.

### RDS

Full RDS API with real database engines (PostgreSQL, MySQL, MariaDB). Complete AWS API surface including CreateDBInstance, ModifyDBInstance, snapshots, parameter groups, read replicas, and more. Runs actual database instances via Docker using the same pattern as Lambda execution. Your tests talk to real databases, managed through the standard RDS API.

### ElastiCache

Complete ElastiCache implementation covering cache clusters, replication groups, global replication groups, serverless caches and snapshots, subnet groups, users/user groups, failover operations, and tagging. Docker-backed Redis provides real caching behavior through the AWS API.

## Up next

Priorities based on real-world usage research and user demand signals.

### Bedrock

AWS AI/ML service for foundation model invocation, guardrails, knowledge bases, and agents. Testing AI features locally is painful without mock service support.

### Lambda Containers

Extend existing Lambda implementation to support container image deployments (not just ZIP). Larger deployment packages, custom runtimes, modern Lambda patterns. More practical than full ECS for most testing workflows.

### CloudFront

CDN configuration, cache behaviors, origins, and invalidations. LocalStack paid-only feature.

### Athena

SQL analytics on S3 with query execution, result handling, and workgroup management. Complements existing S3 implementation.

### AppConfig

Feature flags and configuration management. Modern pattern for gradual rollouts and A/B testing.

### CloudWatch Metrics

Metric storage, alarms, dashboards, and math expressions. Completes the CloudWatch story alongside our existing CloudWatch Logs implementation (113 operations).

### ECR

Container registry for image storage and lifecycle management. Primarily needed as dependency for Lambda Containers.

### ECS

Container orchestration with clusters, services, task definitions, and task execution (backed by real Docker containers). Deferred until clearer demand signal — Lambda Containers addresses most container testing needs without full orchestration complexity.

## Testing APIs

fakecloud is built for testing. Beyond emulating the AWS API, fakecloud exposes its own `/_fakecloud/*` endpoints that give you capabilities AWS doesn't — inspecting internal state, simulating events, and setting up test scenarios.

### Introspection *(shipped)*

Read internal state that AWS doesn't expose. Useful for test assertions.

- **`GET /_fakecloud/ses/emails`** — Every email sent through SES, with full headers and body.
- **`GET /_fakecloud/lambda/invocations`** — Every Lambda invocation with request payload and response.
- **`GET /_fakecloud/stepfunctions/executions`** — All Step Functions executions with status, input, output, and timestamps.
- **`GET /_fakecloud/apigatewayv2/requests`** — All HTTP API requests with method, path, headers, and response.
- **`GET /_fakecloud/sns/messages`** — All messages published to SNS topics.
- **`GET /_fakecloud/sqs/messages`** — All messages across all SQS queues with receive counts.
- **`GET /_fakecloud/events/history`** — All EventBridge events and target deliveries.
- **`GET /_fakecloud/s3/notifications`** — All S3 notification events that fired.
- **`GET /_fakecloud/sns/pending-confirmations`** — SNS subscriptions awaiting confirmation.
- **`GET /_fakecloud/lambda/warm-containers`** — Lambda containers currently warm.

### Simulation *(shipped)*

Trigger things that normally come from AWS infrastructure or external systems.

- **`POST /_fakecloud/ses/inbound`** — Simulate receiving an email. Evaluates receipt rules and executes S3/SNS/Lambda actions.
- **`POST /_fakecloud/events/fire-rule`** — Fire a specific EventBridge rule immediately, regardless of its schedule.
- **`POST /_fakecloud/dynamodb/ttl-processor/tick`** — Expire DynamoDB items whose TTL attribute is in the past.
- **`POST /_fakecloud/secretsmanager/rotation-scheduler/tick`** — Rotate secrets whose rotation schedule is due.
- **`POST /_fakecloud/sqs/expiration-processor/tick`** — Remove expired messages from all SQS queues.
- **`POST /_fakecloud/sqs/{queue-name}/force-dlq`** — Force messages to dead-letter queue without waiting for more receives.
- **`POST /_fakecloud/s3/lifecycle-processor/tick`** — Run S3 lifecycle rules (expiration, transitions) immediately.
- **`POST /_fakecloud/sns/confirm-subscription`** — Force-confirm a pending SNS subscription.
- **`POST /_fakecloud/lambda/{function-name}/evict-container`** — Force cold start by evicting warm container.

### State setup *(shipped)*

- **`POST /_fakecloud/reset`** — Reset all state across all services.
- **`POST /_fakecloud/reset/{service}`** — Reset only a specific service's state.

### SDKs

TypeScript, Python, and Go SDKs now wrap the `/_fakecloud/*` endpoints for
cleaner test code. Future SDK work, if we need it, will likely focus on Rust and
Java.

## Design principles

**Smart proxy pattern** — For services that wrap stateful software (RDS, ElastiCache, Lambda execution), fakecloud implements the full AWS API and delegates execution to real software via Docker. This gives you API compatibility and real behavior in one package.

**No stubs** — Every operation either does what AWS does or returns an explicit error. We don't return fake success responses for things we haven't implemented.

**Conformance testing** — Every service is tested against AWS Smithy models with thousands of auto-generated test variants covering boundary values, optional field permutations, and negative cases.

**Demand-driven** — Priorities come from real-world usage patterns, not just AWS service catalog breadth. We analyze LocalStack usage, GitHub code searches, and direct user requests.

## Suggesting a service

Open an issue on [GitHub](https://github.com/faiscadev/fakecloud/issues) with the service name and your use case. Real-world demand drives prioritization.
