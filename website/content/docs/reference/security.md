+++
title = "SigV4 verification and IAM enforcement"
description = "Opt-in security features: real SigV4 signature checking and Phase 1 IAM identity-policy evaluation."
weight = 5
+++

By default, fakecloud parses SigV4 headers for routing but doesn't verify signatures, and stores IAM policies without evaluating them. That's the right default for "tests just work" — real signature verification and policy enforcement get in the way of the happy path.

When you explicitly need them, two orthogonal flags flip them on:

```bash
FAKECLOUD_VERIFY_SIGV4=true    # or --verify-sigv4
FAKECLOUD_IAM=off|soft|strict  # or --iam off|soft|strict
```

They're independent: you can turn on SigV4 verification without touching IAM, or the other way around.

## The reserved root identity

The credential pair `test`/`test` (and any access key starting with `test`) is treated as the de-facto root bypass. It skips both SigV4 verification and IAM enforcement, matching the community convention that LocalStack and other emulators use for local development.

When either opt-in feature is enabled, fakecloud emits a one-time WARN at startup noting this bypass so you don't silently get false-positive "my policies work" results from unsigned test clients.

## `--verify-sigv4`

When on, every incoming request is cryptographically verified:

1. **Canonical request** rebuilt per the AWS SigV4 spec (double-encoded path for non-S3, single-encoded for S3; sorted, URL-encoded query string; lowercased + sorted headers; payload hash from `X-Amz-Content-Sha256` when present, otherwise `sha256(body)`).
2. **Signing key** derived via the four-step HMAC chain `AWS4 -> date -> region -> service -> aws4_request`.
3. **Constant-time comparison** against the signature the client sent.
4. **Clock skew window** of ±15 minutes, matching AWS.

Verification failures return protocol-correct AWS errors before business logic runs:

| Failure | AWS error |
| --- | --- |
| Wrong signature | `SignatureDoesNotMatch` |
| Unknown access key | `InvalidClientTokenId` |
| Clock skew > 15 min | `RequestTimeTooSkewed` |
| Malformed auth header | `IncompleteSignature` |

Header-based `Authorization: AWS4-HMAC-SHA256 ...` and query-string (presigned URL) signatures are both supported. STS temporary credentials from `AssumeRole`, `GetSessionToken`, and `GetFederationToken` are persisted per-request and verified against the secret key the client received when it called STS.

## `--iam off|soft|strict`

Three modes, in order of aggressiveness:

- **`off`** (default): policies are stored but never consulted. Zero behavior change from unconfigured fakecloud.
- **`soft`**: policies are evaluated and each deny is logged on the `fakecloud::iam::audit` tracing target, but the request is allowed through. Useful for onboarding: you can see which statements would fire without breaking your test suite.
- **`strict`**: policies are evaluated and denied requests fail with a protocol-correct `AccessDeniedException` before the service handler runs.

Filter the audit events with `RUST_LOG=fakecloud::iam::audit=warn`.

### Root is always allowed

The account's IAM root identity (`arn:aws:iam::<account>:root`) and the reserved `test*` bypass AKIDs always pass enforcement, matching AWS's own behavior where root bypasses identity-based policies.

### Enforced services

Opt-in enforcement covers the services most commonly subject to real IAM policies:

| Service | Covered actions | Resource ARN shape |
| --- | --- | --- |
| **IAM** | All 128 supported actions | `arn:aws:iam::<account>:{user,role,group,policy,instance-profile,mfa,server-certificate,saml-provider,oidc-provider}/<name>` |
| **STS** | All 8 supported actions | `*` (or `RoleArn` for `AssumeRole*`) |
| **SQS** | All 20 supported actions | `arn:aws:sqs:<region>:<account>:<queue-name>` |
| **SNS** | All 34 supported actions | Topic / subscription / platform-app / endpoint ARNs |
| **S3** | All 74 supported actions | `arn:aws:s3:::<bucket>[/<key>]` (object actions include the key; bucket actions don't) |

Other services are not enforced even with `FAKECLOUD_IAM=strict`. The startup log enumerates which services are enforced vs. skipped so you always know the current surface. If a service you need is missing, [open an issue](https://github.com/faiscadev/fakecloud/issues) — the wiring is straightforward per-service.

## Phase 1 evaluator scope

The policy evaluator implements the essentials of AWS's identity-based policy evaluation. It deliberately stops short of Phase 2 features so you don't build false mental models from a half-evaluator.

### Implemented

- `Effect: "Allow"` / `Effect: "Deny"` with **Deny precedence** (any matching deny wins).
- `Action` / `NotAction` with `*` and `?` wildcards. Service prefix match is case-insensitive; action names are case-sensitive (matches AWS).
- `Resource` / `NotResource` with `*` and `?` wildcards.
- Identity policies attached to:
  - IAM users (inline + managed + via group membership, inline and managed)
  - IAM roles (inline + managed, for assumed-role sessions)
- Empty effective policy set -> implicit deny.

### Not implemented (Phase 2)

Statements that use any of these features are **skipped during evaluation** (with a `debug!` on the audit target). They're tracked for Phase 2 and will be announced when the evaluator can handle them:

- `Condition` blocks (StringEquals, IpAddress, DateLessThan, etc.)
- Resource-based policies (S3 bucket policies, SNS topic policies, KMS key policies, Lambda resource policies)
- Permission boundaries
- Service control policies (SCPs)
- Session policies passed to `AssumeRole`
- ABAC / tag conditions
- `NotPrincipal`

If you need any of these for your test scenarios, **use real AWS**. fakecloud is a test tool, not an IAM simulator.

## Practical example

Bootstrap a user with root credentials, attach a resource-scoped policy, then hit the service with their own access key:

```bash
# Start fakecloud with enforcement on.
FAKECLOUD_VERIFY_SIGV4=true FAKECLOUD_IAM=strict ./fakecloud

# Root-bypass bootstrap.
AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test \
  aws --endpoint-url http://localhost:4566 iam create-user --user-name alice
AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test \
  aws --endpoint-url http://localhost:4566 iam create-access-key --user-name alice
# -> emits AKIA..., SECRET...

AWS_ACCESS_KEY_ID=test AWS_SECRET_ACCESS_KEY=test \
  aws --endpoint-url http://localhost:4566 iam put-user-policy \
    --user-name alice \
    --policy-name ReadSelf \
    --policy-document '{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"iam:GetUser","Resource":"arn:aws:iam::123456789012:user/alice"}]}'

# Alice can read herself...
AWS_ACCESS_KEY_ID=<alice-akid> AWS_SECRET_ACCESS_KEY=<alice-secret> \
  aws --endpoint-url http://localhost:4566 iam get-user --user-name alice
# -> success

# ...but not anyone else.
AWS_ACCESS_KEY_ID=<alice-akid> AWS_SECRET_ACCESS_KEY=<alice-secret> \
  aws --endpoint-url http://localhost:4566 iam get-user --user-name root
# -> AccessDeniedException
```

## See also

- [Limitations](@/docs/reference/limitations.md) — what fakecloud doesn't do at all
- [Configuration](@/docs/reference/configuration.md) — full flag + env var reference
- [IAM service docs](@/docs/services/iam.md) — per-action coverage
