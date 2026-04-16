+++
title = "IAM"
description = "Users, roles, policies, groups, instance profiles, OIDC/SAML providers."
weight = 7
+++

fakecloud implements **176 of 176** IAM operations at 100% Smithy conformance.

## Supported features

- **Users** — CRUD, access keys, login profiles, signing certificates, SSH public keys, service-specific credentials, MFA devices
- **Roles** — CRUD, inline policies, managed policies, trust relationships, instance profile relationships
- **Groups** — CRUD, user membership, inline and managed policies
- **Policies** — managed policies, policy versions, attachment, simulation (recorded)
- **Instance profiles** — CRUD, role attachment
- **OIDC providers** — CRUD, client IDs, thumbprints
- **SAML providers** — CRUD, metadata documents
- **Account management** — aliases, password policy, summary
- **Tags** — on users, roles, policies, and policy versions
- **Service-linked roles** — CRUD with service name validation

## Protocol

Query protocol. Form-encoded body, `Action` parameter, XML responses.

## Gotchas

- **Policies are stored and optionally evaluated.** By default fakecloud records IAM policies without evaluating them. Set `FAKECLOUD_IAM=strict` (or `soft` for log-only) to turn on policy evaluation — Allow/Deny with Deny precedence, Action/Resource wildcards, user/group/role inline and managed policies, `Condition` blocks with all 28 AWS operators and global + service-specific keys, resource-based policies for S3 bucket, SNS topic, and Lambda function policies with AWS's cross-account combining semantics, permission boundaries (`PutUserPermissionsBoundary` / `PutRolePermissionsBoundary`), and session policies passed to `AssumeRole` / `GetFederationToken`. ABAC tag conditions, KMS key policies, and `NotPrincipal` are not yet evaluated. See [SigV4 verification and IAM enforcement](@/docs/reference/security.md) for the full scope.
- **SigV4 verification is opt-in.** By default fakecloud parses signatures for routing but doesn't verify them. Set `FAKECLOUD_VERIFY_SIGV4=true` to turn on cryptographic verification with the standard ±15-minute clock skew window. The reserved `test`/`test` root-bypass convention always passes, matching LocalStack.

## Source

- [`crates/fakecloud-iam`](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-iam)
- [AWS IAM API reference](https://docs.aws.amazon.com/IAM/latest/APIReference/welcome.html)
