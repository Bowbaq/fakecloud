+++
title = "AWS Organizations"
description = "Minimal Organizations control plane: organization, OU tree, account membership. SCP CRUD and enforcement ship in later batches."
weight = 6
+++

fakecloud ships a minimal AWS Organizations implementation. Its purpose is to let you attach Service Control Policies (SCPs) to accounts and organizational units so your tests can exercise the full IAM evaluation hierarchy — SCP ceiling, permission boundary, session policy, identity policy, resource policy — end to end.

The current surface: organization lifecycle, OU tree, and member-account listing. SCP CRUD lands in Batch 3, and SCP enforcement through the IAM evaluator lands in Batch 4.

## Model

- One organization per fakecloud process. `CreateOrganization` sets the caller's account as the management account and seeds a root OU.
- `FullAWSAccess` is auto-created and auto-attached to the root OU on `CreateOrganization`, matching AWS. Its content:
  ```json
  {"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"*","Resource":"*"}]}
  ```
- Only the `ALL` feature set is supported. `CONSOLIDATED_BILLING` disables SCPs in AWS and is not useful in a test tool; requesting it returns `UnsupportedAPIEndpointException`.
- Only the management account can run write ops (`CreateOrganizationalUnit`, `MoveAccount`, `DeleteOrganization`, etc.). A member but non-management caller gets `AccessDeniedException`. A non-member caller gets `AWSOrganizationsNotInUseException` so org existence itself does not leak.
- Accounts auto-enroll into the root OU whenever a new admin bootstraps via `/_fakecloud/iam/create-admin` and an organization exists. This means tests can create the org first, then bootstrap admins for each member account, and the membership lands automatically.

## Supported operations

| Operation | Status | Notes |
|-----------|--------|-------|
| `CreateOrganization` | ✅ | `FeatureSet=ALL` only; caller becomes management |
| `DescribeOrganization` | ✅ | Returns `AWSOrganizationsNotInUseException` to non-members |
| `DeleteOrganization` | ✅ | Management only; fails if any non-management members remain |
| `ListRoots` | ✅ | Returns the single root |
| `CreateOrganizationalUnit` | ✅ | Management only; name must be unique under the parent |
| `UpdateOrganizationalUnit` | ✅ | Rename; duplicate-name check |
| `DeleteOrganizationalUnit` | ✅ | Fails with `OrganizationalUnitNotEmptyException` if children remain |
| `DescribeOrganizationalUnit` | ✅ | |
| `ListOrganizationalUnitsForParent` | ✅ | |
| `ListAccounts` | ✅ | Returns all members |
| `ListAccountsForParent` | ✅ | |
| `DescribeAccount` | ✅ | |
| `MoveAccount` | ✅ | Enforces exact source-parent match |

SCP CRUD and enforcement are in active development. See the security reference for the IAM evaluation hierarchy.

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
