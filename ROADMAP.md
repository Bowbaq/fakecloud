# Roadmap

fakecloud's goal is to be the best free AWS emulator for integration testing and local development. This roadmap outlines what's coming next.

For every service we implement, the standard is the same: full API coverage, real behavior (not stubs), conformance testing against AWS Smithy models, and cross-service integrations where applicable.

## Currently shipping

**Cognito** — User Pools and Identity Pools. Authentication and authorization testing without real AWS credentials.

## Up next

### Kinesis

Kinesis Data Streams and Kinesis Data Firehose. This also unlocks DynamoDB Streams, which depends on a Kinesis-compatible streaming backend.

### RDS

Full RDS API with real database engines. The approach: implement the complete AWS API surface (CreateDBInstance, ModifyDBInstance, snapshots, parameter groups, etc.) and run actual PostgreSQL and MySQL instances via Docker — the same pattern fakecloud uses for Lambda execution. Your tests talk to real databases, managed through the standard RDS API.

### ECR + ECS

Container registry and container orchestration. ECR provides image storage and lifecycle management. ECS provides clusters, services, task definitions, and task execution — backed by real Docker containers.

### ElastiCache

Full ElastiCache API backed by real Redis instances via Docker. Create, modify, and delete cache clusters through the standard API, with actual Redis available for your application to connect to.

### Elastic Load Balancing

Application Load Balancers, target groups, listeners, and routing rules. Configuration management and basic request routing.

### CloudFront

Distribution configuration, cache behaviors, origins, and invalidations.

### API Gateway v2

HTTP APIs and WebSocket APIs. REST API v1 is available in LocalStack Community; HTTP API v2 is not. Integrates with Lambda (already supported).

### Step Functions

Amazon States Language interpreter with full execution semantics. Task, Choice, Parallel, Map, Wait, and all other state types. Integrates with Lambda and other fakecloud services.

### CloudWatch Metrics

Metric storage, alarms, dashboards, and math expressions. Completes the CloudWatch story alongside our existing CloudWatch Logs implementation (113 operations).

## Testing APIs

fakecloud is built for testing. Beyond emulating the AWS API, fakecloud exposes its own `/_fakecloud/*` endpoints that give you capabilities AWS doesn't — inspecting internal state, simulating events, and setting up test scenarios.

### Introspection

Read internal state that AWS doesn't expose. Useful for test assertions.

- **`GET /_fakecloud/ses/emails`** — Every email sent through SES, with full headers and body. *(shipped)*
- **`GET /_fakecloud/lambda/invocations`** — Every Lambda invocation with request payload and response.  *(shipped)*
- **SNS**: Messages published to each topic and what was delivered to each subscription.
- **SQS**: Message delivery history, dead-letter queue activity, delivery counts.
- **EventBridge**: Events that matched rules and which targets were invoked.
- **S3**: Notification events that fired on object operations.

### Simulation

Trigger things that normally come from AWS infrastructure or external systems.

- **`POST /_fakecloud/ses/inbound`** — Simulate receiving an email. Evaluates receipt rules and executes S3/SNS/Lambda actions. *(shipped)*
- **EventBridge**: Advance time to trigger scheduled rules without waiting.
- **SQS**: Force dead-letter queue delivery without waiting for visibility timeouts.

### State setup

Set up test scenarios faster than calling multiple AWS APIs.

- **`POST /_fakecloud/reset`** — Reset all state across all services. *(shipped)*
- **Bulk seed**: Load data into DynamoDB tables, S3 buckets, or SQS queues in a single call.
- **Pre-configure**: Set up IAM roles with policies, verify SES identities, or create full CloudFormation-style resource graphs without running templates.

### SDKs

Once the APIs are stable, client libraries for TypeScript, Python, Go, Rust, and Java will wrap the `/_fakecloud/*` endpoints for cleaner test code.

## Design principles

**Smart proxy pattern** — For services that wrap stateful software (RDS, ElastiCache, ECS), fakecloud implements the full AWS API and delegates execution to real software via Docker. This gives you API compatibility and real behavior in one package.

**No stubs** — Every operation either does what AWS does or returns an explicit error. We don't return fake success responses for things we haven't implemented.

**Conformance testing** — Every service is tested against AWS Smithy models with thousands of auto-generated test variants covering boundary values, optional field permutations, and negative cases.

## Suggesting a service

Open an issue on [GitHub](https://github.com/faiscadev/fakecloud/issues) with the service name and your use case. Real-world demand drives prioritization.
