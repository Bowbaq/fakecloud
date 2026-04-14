+++
title = "STS"
description = "AssumeRole, session tokens, federation, caller identity."
weight = 8
+++

fakecloud implements **11 of 11** STS operations at 100% Smithy conformance.

## Supported features

- **AssumeRole** — with session name, duration, policies, external ID
- **AssumeRoleWithWebIdentity** — OIDC federation
- **AssumeRoleWithSAML** — SAML federation
- **GetSessionToken** — temporary credentials
- **GetFederationToken** — federated user credentials
- **GetCallerIdentity** — account ID, user ARN, user ID
- **DecodeAuthorizationMessage** — encoded message decoding
- **GetAccessKeyInfo** — access key metadata

## Protocol

Query protocol. Form-encoded body, `Action` parameter, XML responses.

## Gotchas

- Returned credentials are **dummy placeholders** — fakecloud doesn't validate anything against them. Your test code can use any credentials and fakecloud will accept them.
- `GetCallerIdentity` returns whatever account ID you configured via `--account-id` (default `123456789012`).

## Source

- [`crates/fakecloud-iam`](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-iam) — STS lives alongside IAM
- [AWS STS API reference](https://docs.aws.amazon.com/STS/latest/APIReference/welcome.html)
