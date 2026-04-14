+++
title = "Secrets Manager"
description = "Secrets, versioning, rotation via Lambda, replication."
weight = 10
+++

fakecloud implements **23 of 23** Secrets Manager operations at 100% Smithy conformance.

## Supported features

- **Secrets** — CRUD, tags, resource-based policies
- **Versioning** — stages (AWSCURRENT, AWSPREVIOUS, AWSPENDING), version IDs, explicit version retrieval
- **Soft delete** — DeleteSecret with recovery window, RestoreSecret
- **Rotation** — RotateSecret invokes a Lambda function through all 4 steps (createSecret, setSecret, testSecret, finishSecret)
- **Automatic rotation scheduling** — via `/_fakecloud/secretsmanager/rotation-scheduler/tick`
- **Replication** — replica regions tracked in state, not actually replicated
- **Random password generation** — GetRandomPassword with full character class support

## Protocol

JSON protocol. `X-Amz-Target` header, JSON body, JSON responses.

## Introspection

- `POST /_fakecloud/secretsmanager/rotation-scheduler/tick` — trigger rotation for secrets whose schedule is due

## Cross-service delivery

- **Secrets Manager -> Lambda** — Rotation invokes the configured Lambda for all 4 rotation steps

## Source

- [`crates/fakecloud-secretsmanager`](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-secretsmanager)
- [AWS Secrets Manager API reference](https://docs.aws.amazon.com/secretsmanager/latest/apireference/Welcome.html)
