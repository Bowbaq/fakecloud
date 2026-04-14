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

- **Policies are stored, not evaluated.** fakecloud records IAM policies but does not enforce them at request time. Every request is treated as authorized. This is intentional — testing IAM policy evaluation belongs against real AWS (or dedicated policy simulators), and cross-cutting authz checks are out of scope for an emulator.
- **Access keys are dummy.** fakecloud accepts any credentials for any request. Access keys created via IAM are recorded but never checked against SigV4 signatures.

## Source

- [`crates/fakecloud-iam`](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-iam)
- [AWS IAM API reference](https://docs.aws.amazon.com/IAM/latest/APIReference/welcome.html)
