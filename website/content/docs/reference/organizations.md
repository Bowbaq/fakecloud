+++
title = "AWS Organizations"
description = "Organizations control plane + SCP enforcement as the top-of-chain IAM evaluator layer."
weight = 6
+++

fakecloud ships a minimal AWS Organizations implementation. Its purpose is to let you attach Service Control Policies (SCPs) to accounts and organizational units so your tests can exercise the full IAM evaluation hierarchy — SCP ceiling, permission boundary, session policy, identity policy, resource policy — end to end.

Both the control plane and SCP enforcement are live. SCPs act as the top-of-chain allow-list ceiling: see the [security reference](/docs/reference/security#phase-6-service-control-policies-scps) for the full evaluation order and the management-account and service-linked-role exemptions.

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
| `CreatePolicy` | ✅ | `Type=SERVICE_CONTROL_POLICY` only; structural JSON validation |
| `UpdatePolicy` | ✅ | Blocks mutation of AWS-managed policies (`FullAWSAccess`) |
| `DeletePolicy` | ✅ | Blocks deletion when attached or AWS-managed |
| `DescribePolicy` | ✅ | |
| `ListPolicies` | ✅ | `Filter` required and must be `SERVICE_CONTROL_POLICY` |
| `AttachPolicy` | ✅ | Idempotent re-attach; targets = root / OU / account |
| `DetachPolicy` | ✅ | Returns `PolicyNotAttachedException` on missing attachment |
| `ListPoliciesForTarget` | ✅ | |
| `ListTargetsForPolicy` | ✅ | |

## SCP semantics

Policies are JSON documents of the same shape as identity policies. The control plane validates structural parseability; the evaluator applies the same document at request time, so there is no divergence between what the control plane accepts and what actually gates traffic.

`FullAWSAccess` (`p-FullAWSAccess`) is created and attached to the root OU on `CreateOrganization` and is immutable: `UpdatePolicy` and `DeletePolicy` return `PolicyChangesNotAllowedException` for it. You can still `DetachPolicy` it from the root — AWS permits this, and tests that want to exercise a restrictive SCP posture rely on being able to drop the AWS-managed allow-all.

SCPs are enforced through the IAM evaluator when `FAKECLOUD_IAM` is `soft` or `strict`. Off by default when no organization exists or the resolver returns a non-member / management / service-linked-role principal. See the [security reference](/docs/reference/security#phase-6-service-control-policies-scps) for the full evaluation chain.

## Usage

```rust
use aws_sdk_organizations::Client;

let client = Client::new(&sdk_config);
let created = client.create_organization().send().await?;
let org = created.organization().unwrap();
println!("{} managed by {}", org.id().unwrap(), org.master_account_id().unwrap());
```

## Behavior under `FAKECLOUD_IAM=off`

Organizations control plane works identically regardless of the `FAKECLOUD_IAM` setting. SCP enforcement is gated on IAM being enabled, so creating an organization with `FAKECLOUD_IAM=off` is allowed but has no effect on evaluation — the resolver is still wired, but the evaluator it feeds is never called.
