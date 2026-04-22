use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use chrono::{DateTime, Utc};
use parking_lot::RwLock;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// Shared, cross-account singleton. `None` until `CreateOrganization`
/// runs; at most one organization exists per fakecloud process. An AWS
/// org is not per-account state (it spans accounts), so this is NOT
/// wrapped in `MultiAccountState`.
pub type SharedOrganizationsState = Arc<RwLock<Option<OrganizationState>>>;

pub const FEATURE_SET_ALL: &str = "ALL";
pub const FEATURE_SET_CONSOLIDATED_BILLING: &str = "CONSOLIDATED_BILLING";

pub const POLICY_TYPE_SCP: &str = "SERVICE_CONTROL_POLICY";

/// Stable ID of the AWS-managed FullAWSAccess SCP. Matches AWS's
/// documented identifier so SDK callers can reference it by name.
pub const FULL_AWS_ACCESS_POLICY_ID: &str = "p-FullAWSAccess";
pub const FULL_AWS_ACCESS_POLICY_NAME: &str = "FullAWSAccess";
pub const FULL_AWS_ACCESS_POLICY_DESCRIPTION: &str = "Allows access to every operation";
pub const FULL_AWS_ACCESS_POLICY_CONTENT: &str =
    r#"{"Version":"2012-10-17","Statement":[{"Effect":"Allow","Action":"*","Resource":"*"}]}"#;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganizationState {
    pub org_id: String,
    pub org_arn: String,
    pub management_account_id: String,
    pub management_account_arn: String,
    pub management_account_email: String,
    pub feature_set: String,
    pub root_id: String,
    pub root_arn: String,
    pub root_name: String,
    pub created_at: DateTime<Utc>,
    pub ous: HashMap<String, OrganizationalUnit>,
    pub accounts: HashMap<String, MemberAccount>,
    pub policies: HashMap<String, Policy>,
    /// target_id -> attached policy ids. Targets are root id, OU id, or account id.
    pub attachments: HashMap<String, HashSet<String>>,
}

impl OrganizationState {
    /// Bootstrap a new organization with `management_account_id` as the
    /// management account. Creates the root OU, seeds the AWS-managed
    /// `FullAWSAccess` SCP, and auto-attaches it to root (matching AWS's
    /// default behavior).
    pub fn bootstrap(management_account_id: &str) -> Self {
        let now = Utc::now();
        let org_id = format!("o-{}", random_id(10));
        let root_id = format!("r-{}", random_id(4));
        let org_arn = format!(
            "arn:aws:organizations::{}:organization/{}",
            management_account_id, org_id
        );
        let root_arn = format!(
            "arn:aws:organizations::{}:root/{}/{}",
            management_account_id, org_id, root_id
        );
        let mgmt_arn = format!(
            "arn:aws:organizations::{}:account/{}/{}",
            management_account_id, org_id, management_account_id
        );

        let mut policies = HashMap::new();
        policies.insert(
            FULL_AWS_ACCESS_POLICY_ID.to_string(),
            Policy {
                id: FULL_AWS_ACCESS_POLICY_ID.to_string(),
                arn: format!(
                    "arn:aws:organizations::aws:policy/service_control_policy/{}",
                    FULL_AWS_ACCESS_POLICY_ID
                ),
                name: FULL_AWS_ACCESS_POLICY_NAME.to_string(),
                description: FULL_AWS_ACCESS_POLICY_DESCRIPTION.to_string(),
                policy_type: POLICY_TYPE_SCP.to_string(),
                aws_managed: true,
                content: FULL_AWS_ACCESS_POLICY_CONTENT.to_string(),
            },
        );

        let mut attachments: HashMap<String, HashSet<String>> = HashMap::new();
        attachments
            .entry(root_id.clone())
            .or_default()
            .insert(FULL_AWS_ACCESS_POLICY_ID.to_string());

        let mut accounts = HashMap::new();
        accounts.insert(
            management_account_id.to_string(),
            MemberAccount {
                id: management_account_id.to_string(),
                arn: mgmt_arn.clone(),
                email: format!("{}@example.com", management_account_id),
                name: format!("Account {}", management_account_id),
                status: "ACTIVE".to_string(),
                joined_method: "INVITED".to_string(),
                joined_timestamp: now,
                parent_id: root_id.clone(),
            },
        );

        Self {
            org_id,
            org_arn,
            management_account_id: management_account_id.to_string(),
            management_account_arn: mgmt_arn,
            management_account_email: format!("{}@example.com", management_account_id),
            feature_set: FEATURE_SET_ALL.to_string(),
            root_id,
            root_arn,
            root_name: "Root".to_string(),
            created_at: now,
            ous: HashMap::new(),
            accounts,
            policies,
            attachments,
        }
    }

    /// Returns `true` iff `account_id` is the management account.
    pub fn is_management(&self, account_id: &str) -> bool {
        account_id == self.management_account_id
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OrganizationalUnit {
    pub id: String,
    pub arn: String,
    pub name: String,
    pub parent_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct MemberAccount {
    pub id: String,
    pub arn: String,
    pub email: String,
    pub name: String,
    pub status: String,
    pub joined_method: String,
    pub joined_timestamp: DateTime<Utc>,
    pub parent_id: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Policy {
    pub id: String,
    pub arn: String,
    pub name: String,
    pub description: String,
    pub policy_type: String,
    pub aws_managed: bool,
    pub content: String,
}

/// Generate a lowercase alphanumeric ID fragment of `len` characters.
/// Used for org/root/OU/policy IDs. Pulled from a UUID v4 so the PRNG
/// is the one already pulled in by the rest of fakecloud.
pub fn random_id(len: usize) -> String {
    let mut out = String::with_capacity(len);
    while out.len() < len {
        let u = Uuid::new_v4().simple().to_string();
        for ch in u.chars() {
            if out.len() >= len {
                break;
            }
            out.push(ch);
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bootstrap_has_root_and_full_aws_access() {
        let org = OrganizationState::bootstrap("111111111111");
        assert_eq!(org.management_account_id, "111111111111");
        assert!(org.org_id.starts_with("o-"));
        assert!(org.root_id.starts_with("r-"));
        assert_eq!(org.feature_set, FEATURE_SET_ALL);

        let full = org
            .policies
            .get(FULL_AWS_ACCESS_POLICY_ID)
            .expect("FullAWSAccess auto-seeded");
        assert!(full.aws_managed);
        assert_eq!(full.policy_type, POLICY_TYPE_SCP);

        let root_attachments = org.attachments.get(&org.root_id).expect("root attachments");
        assert!(root_attachments.contains(FULL_AWS_ACCESS_POLICY_ID));
    }

    #[test]
    fn bootstrap_enrolls_management_account_in_root() {
        let org = OrganizationState::bootstrap("222222222222");
        let mgmt = org.accounts.get("222222222222").unwrap();
        assert_eq!(mgmt.parent_id, org.root_id);
        assert_eq!(mgmt.status, "ACTIVE");
    }

    #[test]
    fn is_management_distinguishes_accounts() {
        let org = OrganizationState::bootstrap("111111111111");
        assert!(org.is_management("111111111111"));
        assert!(!org.is_management("222222222222"));
    }

    #[test]
    fn random_id_has_requested_length() {
        for len in [4, 8, 10, 16, 32] {
            let id = random_id(len);
            assert_eq!(id.len(), len);
        }
    }
}
