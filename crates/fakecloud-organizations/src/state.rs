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

    /// Enroll `account_id` into the root OU as a member of the
    /// organization if not already known. No-op when the account is
    /// already enrolled anywhere in the tree. Used as the
    /// auto-enrollment hook when a new IAM admin bootstraps via
    /// `/_fakecloud/iam/create-admin` while an organization exists.
    pub fn enroll_account_if_missing(&mut self, account_id: &str) {
        if self.accounts.contains_key(account_id) {
            return;
        }
        let arn = format!(
            "arn:aws:organizations::{}:account/{}/{}",
            self.management_account_id, self.org_id, account_id
        );
        self.accounts.insert(
            account_id.to_string(),
            MemberAccount {
                id: account_id.to_string(),
                arn,
                email: format!("{}@example.com", account_id),
                name: format!("Account {}", account_id),
                status: "ACTIVE".to_string(),
                joined_method: "INVITED".to_string(),
                joined_timestamp: Utc::now(),
                parent_id: self.root_id.clone(),
            },
        );
    }

    /// Create a new OU under `parent_id` (which must be the root or
    /// another existing OU). Returns the created OU on success.
    ///
    /// Errors:
    /// - `ParentNotFoundException` — `parent_id` does not exist in
    ///   this org (neither root nor a known OU).
    /// - `DuplicateOrganizationalUnitException` — another OU with the
    ///   same name already lives directly under `parent_id`.
    pub fn create_ou(
        &mut self,
        parent_id: &str,
        name: &str,
    ) -> Result<OrganizationalUnit, OrgError> {
        if parent_id != self.root_id && !self.ous.contains_key(parent_id) {
            return Err(OrgError::ParentNotFound(parent_id.to_string()));
        }
        let dup = self
            .ous
            .values()
            .any(|ou| ou.parent_id == parent_id && ou.name == name);
        if dup {
            return Err(OrgError::DuplicateOrganizationalUnit(name.to_string()));
        }
        let root_suffix = self.root_id.strip_prefix("r-").unwrap_or(&self.root_id);
        let id = format!("ou-{}-{}", root_suffix, random_id(8));
        let arn = format!(
            "arn:aws:organizations::{}:ou/{}/{}",
            self.management_account_id, self.org_id, id
        );
        let ou = OrganizationalUnit {
            id: id.clone(),
            arn,
            name: name.to_string(),
            parent_id: parent_id.to_string(),
        };
        self.ous.insert(id, ou.clone());
        Ok(ou)
    }

    /// Rename an existing OU.
    pub fn rename_ou(
        &mut self,
        ou_id: &str,
        new_name: &str,
    ) -> Result<OrganizationalUnit, OrgError> {
        let parent_id = self
            .ous
            .get(ou_id)
            .ok_or_else(|| OrgError::OrganizationalUnitNotFound(ou_id.to_string()))?
            .parent_id
            .clone();
        let dup = self
            .ous
            .values()
            .any(|ou| ou.id != ou_id && ou.parent_id == parent_id && ou.name == new_name);
        if dup {
            return Err(OrgError::DuplicateOrganizationalUnit(new_name.to_string()));
        }
        let ou = self.ous.get_mut(ou_id).unwrap();
        ou.name = new_name.to_string();
        Ok(ou.clone())
    }

    /// Delete an OU. Fails with `OrganizationalUnitNotEmptyException`
    /// if the OU contains any child OUs or member accounts.
    pub fn delete_ou(&mut self, ou_id: &str) -> Result<(), OrgError> {
        if !self.ous.contains_key(ou_id) {
            return Err(OrgError::OrganizationalUnitNotFound(ou_id.to_string()));
        }
        let has_child_ou = self.ous.values().any(|ou| ou.parent_id == ou_id);
        let has_account = self.accounts.values().any(|a| a.parent_id == ou_id);
        if has_child_ou || has_account {
            return Err(OrgError::OrganizationalUnitNotEmpty(ou_id.to_string()));
        }
        // Detach all policies from the deleted target so stale pointers
        // don't survive.
        self.attachments.remove(ou_id);
        self.ous.remove(ou_id);
        Ok(())
    }

    /// Move an account between OUs.
    ///
    /// Errors:
    /// - `AccountNotFoundException`
    /// - `SourceParentNotFoundException` when `source_parent` is not
    ///   the account's current parent
    /// - `DestinationParentNotFoundException` when `dest_parent` is
    ///   not root or a known OU
    pub fn move_account(
        &mut self,
        account_id: &str,
        source_parent: &str,
        dest_parent: &str,
    ) -> Result<(), OrgError> {
        let account = self
            .accounts
            .get_mut(account_id)
            .ok_or_else(|| OrgError::AccountNotFound(account_id.to_string()))?;
        if account.parent_id != source_parent {
            return Err(OrgError::SourceParentNotFound(source_parent.to_string()));
        }
        let dest_exists = dest_parent == self.root_id || self.ous.contains_key(dest_parent);
        if !dest_exists {
            return Err(OrgError::DestinationParentNotFound(dest_parent.to_string()));
        }
        account.parent_id = dest_parent.to_string();
        Ok(())
    }
}

/// Typed errors used by organization state mutations so the service
/// layer can translate each into the correct AWS exception code.
#[derive(Debug)]
pub enum OrgError {
    ParentNotFound(String),
    DuplicateOrganizationalUnit(String),
    OrganizationalUnitNotFound(String),
    OrganizationalUnitNotEmpty(String),
    AccountNotFound(String),
    SourceParentNotFound(String),
    DestinationParentNotFound(String),
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

    #[test]
    fn enroll_account_if_missing_adds_to_root() {
        let mut org = OrganizationState::bootstrap("111111111111");
        org.enroll_account_if_missing("222222222222");
        let member = org.accounts.get("222222222222").expect("enrolled");
        assert_eq!(member.parent_id, org.root_id);
    }

    #[test]
    fn enroll_account_if_missing_is_idempotent() {
        let mut org = OrganizationState::bootstrap("111111111111");
        org.enroll_account_if_missing("111111111111");
        assert_eq!(org.accounts.len(), 1);
    }

    #[test]
    fn create_ou_rejects_unknown_parent() {
        let mut org = OrganizationState::bootstrap("111111111111");
        let err = org.create_ou("ou-nope", "team").unwrap_err();
        assert!(matches!(err, OrgError::ParentNotFound(_)));
    }

    #[test]
    fn create_ou_rejects_duplicate_name_under_same_parent() {
        let mut org = OrganizationState::bootstrap("111111111111");
        let root = org.root_id.clone();
        org.create_ou(&root, "engineering").unwrap();
        let err = org.create_ou(&root, "engineering").unwrap_err();
        assert!(matches!(err, OrgError::DuplicateOrganizationalUnit(_)));
    }

    #[test]
    fn create_ou_allows_same_name_under_different_parents() {
        let mut org = OrganizationState::bootstrap("111111111111");
        let root = org.root_id.clone();
        let parent = org.create_ou(&root, "top").unwrap();
        // Same leaf name under a different parent OU must succeed.
        org.create_ou(&parent.id, "engineering").unwrap();
        org.create_ou(&root, "engineering").unwrap();
    }

    #[test]
    fn delete_ou_rejects_non_empty_with_accounts() {
        let mut org = OrganizationState::bootstrap("111111111111");
        let root = org.root_id.clone();
        let ou = org.create_ou(&root, "team").unwrap();
        org.enroll_account_if_missing("222222222222");
        org.move_account("222222222222", &root, &ou.id).unwrap();
        let err = org.delete_ou(&ou.id).unwrap_err();
        assert!(matches!(err, OrgError::OrganizationalUnitNotEmpty(_)));
    }

    #[test]
    fn delete_ou_rejects_non_empty_with_child_ou() {
        let mut org = OrganizationState::bootstrap("111111111111");
        let root = org.root_id.clone();
        let parent = org.create_ou(&root, "parent").unwrap();
        org.create_ou(&parent.id, "child").unwrap();
        let err = org.delete_ou(&parent.id).unwrap_err();
        assert!(matches!(err, OrgError::OrganizationalUnitNotEmpty(_)));
    }

    #[test]
    fn delete_ou_clears_attachments() {
        let mut org = OrganizationState::bootstrap("111111111111");
        let root = org.root_id.clone();
        let ou = org.create_ou(&root, "team").unwrap();
        org.attachments
            .entry(ou.id.clone())
            .or_default()
            .insert("p-custom".to_string());
        org.delete_ou(&ou.id).unwrap();
        assert!(!org.attachments.contains_key(&ou.id));
    }

    #[test]
    fn move_account_enforces_source_parent() {
        let mut org = OrganizationState::bootstrap("111111111111");
        let root = org.root_id.clone();
        let ou = org.create_ou(&root, "team").unwrap();
        org.enroll_account_if_missing("222222222222");
        let err = org.move_account("222222222222", &ou.id, &root).unwrap_err();
        assert!(matches!(err, OrgError::SourceParentNotFound(_)));
    }

    #[test]
    fn move_account_rejects_unknown_destination() {
        let mut org = OrganizationState::bootstrap("111111111111");
        let root = org.root_id.clone();
        let err = org
            .move_account("111111111111", &root, "ou-nope")
            .unwrap_err();
        assert!(matches!(err, OrgError::DestinationParentNotFound(_)));
    }

    #[test]
    fn rename_ou_rejects_duplicate() {
        let mut org = OrganizationState::bootstrap("111111111111");
        let root = org.root_id.clone();
        let a = org.create_ou(&root, "a").unwrap();
        let b = org.create_ou(&root, "b").unwrap();
        let err = org.rename_ou(&b.id, "a").unwrap_err();
        assert!(matches!(err, OrgError::DuplicateOrganizationalUnit(_)));
        // Renaming in place is fine.
        org.rename_ou(&a.id, "a").unwrap();
    }
}
