+++
title = "Persistence"
description = "How fakecloud persists state to disk across restarts."
weight = 2
+++

By default fakecloud keeps all state in memory: startup is instant, shutdown is a no-op, and tests can run in parallel without cross-contamination. That's what you want for CI and most dev workflows.

For longer-running local environments where you want state to survive restarts, pass `--storage-mode=persistent --data-path=<dir>` to mirror all service state to disk.

## Enabling persistent mode

```sh
fakecloud --storage-mode persistent --data-path /var/lib/fakecloud
```

Or via environment:

```sh
FAKECLOUD_STORAGE_MODE=persistent FAKECLOUD_DATA_PATH=/var/lib/fakecloud fakecloud
```

## What's persisted

- **S3** — buckets, objects, versions, delete markers, multipart uploads (resumable across restarts), and every bucket subresource: tags, lifecycle, CORS, policy, notification, logging, website, public access block, object lock, replication, ownership, inventory, encryption, ACL, accelerate. Written to disk on every mutation and reloaded on startup.
- **SQS** — queues, attributes, tags, in-flight and delayed messages.
- **SNS** — topics, subscriptions, attributes, tags, platform applications and endpoints, SMS settings.
- **EventBridge** — event buses, rules, targets, archives, replays, connections.
- **IAM / STS** — users, groups, roles, policies, instance profiles, access keys.
- **SSM Parameter Store** — parameters (String/SecureString/StringList), history.
- **Secrets Manager** — secrets, versions, rotation settings.
- **CloudWatch Logs** — log groups, streams, and log events.
- **KMS** — keys, aliases, key policies, grants.
- **DynamoDB** — tables, items, indexes, streams metadata.
- **Kinesis** — streams, shards, records.
- **SES** — identities, configuration sets, templates, contact lists and contacts, tags, suppression list, event destinations, identity policies, dedicated IP pools, tenants, receipt rule sets / rules / filters, account settings.
- **API Gateway v2** — HTTP APIs, routes, integrations, stages, deployments, authorizers.
- **CloudFormation** — stacks, templates, parameters, tags, resource listings, and notification ARNs.
- **Cognito** — user pools, user pool clients, users, groups, identity providers, resource servers, domains, import jobs, tags, UI customization, log delivery, risk and branding configuration, terms, WebAuthn credentials, refresh/access tokens and sessions. The `/_fakecloud/cognito/auth-events` introspection buffer resets on restart.
- **Lambda** — functions (code zips, configuration, resource policies), event source mappings. The `/_fakecloud/lambda/invocations` introspection buffer resets on restart; containers are rebuilt from the persisted code zip on first Invoke.
- **Step Functions** — state machines, definitions, executions, execution history events, tags.
- **RDS** — DB instances (configuration, credentials, tags), DB snapshots (including dump data), subnet groups, parameter groups.
- **ElastiCache** — cache clusters, replication groups, global replication groups, subnet groups, parameter groups, users, user groups, snapshots, serverless caches and snapshots, reserved cache nodes, tags.
- **Bedrock** — guardrails, guardrail versions, customization jobs, provisioned throughputs, logging config, async invocations, custom models, deployments, model import/copy/invocation jobs, evaluation jobs, inference profiles, prompt routers, resource policies, marketplace endpoints, foundation model agreements, automated reasoning policies/test cases/workflows, tags. The `/_fakecloud/bedrock/invocations` introspection buffer and simulation config (custom responses, response rules, fault rules) reset on restart.

## Version compatibility

On startup fakecloud reads `<data-path>/fakecloud.version.toml`. The file records the on-disk format version and the fakecloud version that created the directory. If the format version doesn't match the running binary, startup fails with an actionable error that points at the file.

There is no automatic migration — the intent is that you either keep using the binary that wrote the directory or start from an empty data path.

## S3 object body handling

Object bodies are streamed straight to disk in persistent mode, not held in RAM. A bounded LRU cache (`--s3-cache-size`, default 256 MiB) keeps recently read bodies available for fast re-reads. Objects larger than `cache-size / 2` bypass the cache on both read and write, so a single large upload cannot evict the entire working set.

## Introspection buffers are not persisted

The `/_fakecloud/s3/notifications` buffer — and every other `/_fakecloud/*` introspection endpoint, including `/_fakecloud/ses/emails` and `/_fakecloud/ses/inbound-emails` — is intentionally not persisted. These exist so tests can assert which events fired during the current run, not as a long-term audit log.
