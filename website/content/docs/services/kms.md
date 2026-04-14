+++
title = "KMS"
description = "Encryption, key management, aliases, grants, real ECDH, key import."
weight = 12
+++

fakecloud implements **53 of 53** KMS operations at 100% Smithy conformance.

## Supported features

- **Symmetric keys** — CreateKey, Encrypt, Decrypt, GenerateDataKey, ReEncrypt
- **Asymmetric keys** — Sign, Verify, GetPublicKey
- **Key management** — DescribeKey, EnableKey, DisableKey, ScheduleKeyDeletion, CancelKeyDeletion
- **Aliases** — CRUD with `alias/` prefix validation
- **Grants** — CreateGrant, RetireGrant, RevokeGrant, ListGrants
- **Key rotation** — automatic rotation flag (tracked), on-demand rotation
- **Key policies** — PutKeyPolicy, GetKeyPolicy, ListKeyPolicies
- **Tags** — on keys
- **Real ECDH** — DeriveSharedSecret performs actual Elliptic Curve Diffie-Hellman
- **Key import** — GetParametersForImport, ImportKeyMaterial with real key material handling
- **Custom key stores** — CRUD (records only)
- **Key replica** — ReplicateKey

## Protocol

JSON protocol. `X-Amz-Target` header, JSON body, JSON responses.

## Gotchas

- **Encryption is real but deterministic.** fakecloud uses a stable in-memory key derivation so encrypted values round-trip correctly across Encrypt/Decrypt calls in the same process, but the ciphertext is not compatible with real AWS KMS.

## Source

- [`crates/fakecloud-kms`](https://github.com/faiscadev/fakecloud/tree/main/crates/fakecloud-kms)
- [AWS KMS API reference](https://docs.aws.amazon.com/kms/latest/APIReference/Welcome.html)
