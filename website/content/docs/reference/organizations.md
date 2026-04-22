+++
title = "AWS Organizations"
description = "Minimal Organizations control plane: create an organization, describe it, delete it. SCP CRUD and enforcement ship in later batches."
weight = 6
+++

fakecloud ships a minimal AWS Organizations implementation. Its purpose is to let you attach Service Control Policies (SCPs) to accounts and organizational units so your tests can exercise the full IAM evaluation hierarchy — SCP ceiling, permission boundary, session policy, identity policy, resource policy — end to end.

The current surface is intentionally narrow: CreateOrganization, DescribeOrganization, and DeleteOrganization. OU ops, account membership ops, and SCP CRUD land in subsequent batches, and SCP enforcement through the IAM evaluator lands last.

## Model

- One organization per fakecloud process. `CreateOrganization` sets the caller's account as the management account and seeds a root OU.
- `FullAWSAccess` is auto-created and auto-attached to the root OU on `CreateOrganization`, matching AWS. Its content:
  ```json
  {"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"*","Resource":"*"}]}
  ```
- Only the `ALL` feature set is supported. `CONSOLIDATED_BILLING` disables SCPs in AWS and is not useful in a test tool; requesting it returns `UnsupportedAPIEndpointException`.
- Only the management account can delete the organization. Any other account calling `DeleteOrganization` gets `AccessDeniedException`.

## Supported operations

| Operation | Status | Notes |
|-----------|--------|-------|
| `CreateOrganization` | ✅ | `FeatureSet=ALL` only; caller becomes management |
| `DescribeOrganization` | ✅ | Returns `AWSOrganizationsNotInUseException` if no org exists |
| `DeleteOrganization` | ✅ | Management only; fails if any non-management members remain |

OU ops, SCP CRUD, and SCP enforcement are in active development. See the security reference for the IAM evaluation hierarchy.

## Usage

```rust
use aws_sdk_organizations::Client;

let client = Client::new(&sdk_config);
let created = client.create_organization().send().await?;
let org = created.organization().unwrap();
println!("{} managed by {}", org.id().unwrap(), org.master_account_id().unwrap());
```

## Behavior under `FAKECLOUD_IAM=off`

Organizations control plane works identically regardless of the `FAKECLOUD_IAM` setting. SCP enforcement — once it ships — is gated on IAM being enabled, so creating an organization with `FAKECLOUD_IAM=off` is allowed but has no effect on evaluation.
