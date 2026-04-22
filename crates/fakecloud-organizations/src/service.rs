use std::sync::Arc;

use async_trait::async_trait;
use http::StatusCode;
use serde_json::{json, Value};

use fakecloud_core::service::{AwsRequest, AwsResponse, AwsService, AwsServiceError};

use crate::state::{
    MemberAccount, OrgError, OrganizationState, OrganizationalUnit, SharedOrganizationsState,
    FEATURE_SET_ALL,
};

/// Single source of truth for supported Organizations actions. Expanded
/// across subsequent batches (SCP CRUD + attachment ops).
pub static ORGANIZATIONS_ACTIONS: &[&str] = &[
    "CreateOrganization",
    "DescribeOrganization",
    "DeleteOrganization",
    "ListRoots",
    "CreateOrganizationalUnit",
    "UpdateOrganizationalUnit",
    "DeleteOrganizationalUnit",
    "DescribeOrganizationalUnit",
    "ListOrganizationalUnitsForParent",
    "ListAccounts",
    "ListAccountsForParent",
    "DescribeAccount",
    "MoveAccount",
];

pub struct OrganizationsService {
    state: SharedOrganizationsState,
}

impl OrganizationsService {
    pub fn new(state: SharedOrganizationsState) -> Self {
        Self { state }
    }

    pub fn shared() -> (Arc<Self>, SharedOrganizationsState) {
        let state: SharedOrganizationsState = Arc::new(parking_lot::RwLock::new(None));
        (Arc::new(Self::new(state.clone())), state)
    }

    fn create_organization(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let feature_set = body
            .get("FeatureSet")
            .and_then(|v| v.as_str())
            .unwrap_or(FEATURE_SET_ALL);
        if feature_set != FEATURE_SET_ALL {
            // fakecloud ships SCP enforcement which requires the ALL
            // feature set. CONSOLIDATED_BILLING disables SCPs in AWS,
            // and we don't simulate that distinction — reject up front
            // rather than silently lie about which feature set is on.
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "UnsupportedAPIEndpointException",
                "fakecloud only supports the ALL feature set for organizations",
            ));
        }

        let mut guard = self.state.write();
        if guard.is_some() {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "AlreadyInOrganizationException",
                "The AWS account is already a member of an organization.",
            ));
        }
        let org = OrganizationState::bootstrap(&req.account_id);
        let resp_value = organization_payload(&org);
        *guard = Some(org);
        Ok(AwsResponse::ok_json(json!({ "Organization": resp_value })))
    }

    fn describe_organization(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let guard = self.state.read();
        let org = guard.as_ref().ok_or_else(organizations_not_in_use)?;
        // AWS scopes DescribeOrganization to members of the organization.
        // Non-members must not learn that an org exists at all — return
        // the same `AWSOrganizationsNotInUseException` the no-org path
        // returns so org metadata doesn't leak across account boundaries.
        if !org.accounts.contains_key(&req.account_id) {
            return Err(organizations_not_in_use());
        }
        Ok(AwsResponse::ok_json(
            json!({ "Organization": organization_payload(org) }),
        ))
    }

    fn delete_organization(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let mut guard = self.state.write();
        let org = guard.as_ref().ok_or_else(organizations_not_in_use)?;
        // Non-members get the same "not in use" error as callers in a
        // process with no org at all — they should not be able to tell
        // the difference.
        if !org.accounts.contains_key(&req.account_id) {
            return Err(organizations_not_in_use());
        }
        if !org.is_management(&req.account_id) {
            return Err(AwsServiceError::aws_error(
                StatusCode::FORBIDDEN,
                "AccessDeniedException",
                "Only the management account can delete the organization.",
            ));
        }
        // Match AWS: delete fails if any member accounts besides the
        // management account remain. In Batch 1 only the management is
        // enrolled, so this check is a no-op; Batch 2 starts populating
        // real member accounts.
        let non_mgmt = org
            .accounts
            .keys()
            .filter(|id| id.as_str() != org.management_account_id)
            .count();
        if non_mgmt > 0 {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "OrganizationNotEmptyException",
                "The organization still has member accounts. Remove them first.",
            ));
        }
        *guard = None;
        Ok(AwsResponse::ok_json(Value::Null))
    }

    fn list_roots(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let guard = self.state.read();
        let org = self.require_member(&guard, &req.account_id)?;
        let root = json!({
            "Id": org.root_id,
            "Arn": org.root_arn,
            "Name": org.root_name,
            "PolicyTypes": [
                {"Type": "SERVICE_CONTROL_POLICY", "Status": "ENABLED"}
            ],
        });
        Ok(AwsResponse::ok_json(json!({ "Roots": [root] })))
    }

    fn create_organizational_unit(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let parent_id = required_str(&body, "ParentId")?;
        let name = required_str(&body, "Name")?;
        let mut guard = self.state.write();
        self.require_member_management(&guard, &req.account_id)?;
        let org = guard.as_mut().unwrap();
        let ou = org.create_ou(parent_id, name).map_err(org_error_to_aws)?;
        Ok(AwsResponse::ok_json(
            json!({ "OrganizationalUnit": ou_payload(&ou) }),
        ))
    }

    fn update_organizational_unit(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let ou_id = required_str(&body, "OrganizationalUnitId")?;
        let new_name = required_str(&body, "Name")?;
        let mut guard = self.state.write();
        self.require_member_management(&guard, &req.account_id)?;
        let org = guard.as_mut().unwrap();
        let ou = org.rename_ou(ou_id, new_name).map_err(org_error_to_aws)?;
        Ok(AwsResponse::ok_json(
            json!({ "OrganizationalUnit": ou_payload(&ou) }),
        ))
    }

    fn delete_organizational_unit(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let ou_id = required_str(&body, "OrganizationalUnitId")?;
        let mut guard = self.state.write();
        self.require_member_management(&guard, &req.account_id)?;
        let org = guard.as_mut().unwrap();
        org.delete_ou(ou_id).map_err(org_error_to_aws)?;
        Ok(AwsResponse::ok_json(Value::Null))
    }

    fn describe_organizational_unit(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let ou_id = required_str(&body, "OrganizationalUnitId")?;
        let guard = self.state.read();
        let org = self.require_member(&guard, &req.account_id)?;
        let ou = org.ous.get(ou_id).ok_or_else(|| {
            org_error_to_aws(OrgError::OrganizationalUnitNotFound(ou_id.to_string()))
        })?;
        Ok(AwsResponse::ok_json(
            json!({ "OrganizationalUnit": ou_payload(ou) }),
        ))
    }

    fn list_organizational_units_for_parent(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let parent_id = required_str(&body, "ParentId")?;
        let guard = self.state.read();
        let org = self.require_member(&guard, &req.account_id)?;
        if parent_id != org.root_id && !org.ous.contains_key(parent_id) {
            return Err(org_error_to_aws(OrgError::ParentNotFound(
                parent_id.to_string(),
            )));
        }
        let children: Vec<Value> = org
            .ous
            .values()
            .filter(|ou| ou.parent_id == parent_id)
            .map(ou_payload)
            .collect();
        Ok(AwsResponse::ok_json(
            json!({ "OrganizationalUnits": children }),
        ))
    }

    fn list_accounts(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let guard = self.state.read();
        let org = self.require_member(&guard, &req.account_id)?;
        let accounts: Vec<Value> = org.accounts.values().map(account_payload).collect();
        Ok(AwsResponse::ok_json(json!({ "Accounts": accounts })))
    }

    fn list_accounts_for_parent(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let parent_id = required_str(&body, "ParentId")?;
        let guard = self.state.read();
        let org = self.require_member(&guard, &req.account_id)?;
        if parent_id != org.root_id && !org.ous.contains_key(parent_id) {
            return Err(org_error_to_aws(OrgError::ParentNotFound(
                parent_id.to_string(),
            )));
        }
        let accounts: Vec<Value> = org
            .accounts
            .values()
            .filter(|a| a.parent_id == parent_id)
            .map(account_payload)
            .collect();
        Ok(AwsResponse::ok_json(json!({ "Accounts": accounts })))
    }

    fn describe_account(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let account_id = required_str(&body, "AccountId")?;
        let guard = self.state.read();
        let org = self.require_member(&guard, &req.account_id)?;
        let account = org
            .accounts
            .get(account_id)
            .ok_or_else(|| org_error_to_aws(OrgError::AccountNotFound(account_id.to_string())))?;
        Ok(AwsResponse::ok_json(
            json!({ "Account": account_payload(account) }),
        ))
    }

    fn move_account(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let account_id = required_str(&body, "AccountId")?;
        let source = required_str(&body, "SourceParentId")?;
        let dest = required_str(&body, "DestinationParentId")?;
        let mut guard = self.state.write();
        self.require_member_management(&guard, &req.account_id)?;
        let org = guard.as_mut().unwrap();
        org.move_account(account_id, source, dest)
            .map_err(org_error_to_aws)?;
        Ok(AwsResponse::ok_json(Value::Null))
    }

    /// Read-side helper: enforce that an org exists and the caller is a
    /// member. Returns the borrowed org on success.
    fn require_member<'a>(
        &self,
        guard: &'a parking_lot::RwLockReadGuard<'_, Option<OrganizationState>>,
        account_id: &str,
    ) -> Result<&'a OrganizationState, AwsServiceError> {
        let org = guard.as_ref().ok_or_else(organizations_not_in_use)?;
        if !org.accounts.contains_key(account_id) {
            return Err(organizations_not_in_use());
        }
        Ok(org)
    }

    /// Write-side helper for mutating ops: caller must be the
    /// management account of an existing organization. Returns the
    /// management-only error rather than an Option, so the caller can
    /// unwrap the guard safely right after.
    fn require_member_management(
        &self,
        guard: &parking_lot::RwLockWriteGuard<'_, Option<OrganizationState>>,
        account_id: &str,
    ) -> Result<(), AwsServiceError> {
        let org = guard.as_ref().ok_or_else(organizations_not_in_use)?;
        if !org.accounts.contains_key(account_id) {
            return Err(organizations_not_in_use());
        }
        if !org.is_management(account_id) {
            return Err(AwsServiceError::aws_error(
                StatusCode::FORBIDDEN,
                "AccessDeniedException",
                "This operation can be called only from the organization's management account.",
            ));
        }
        Ok(())
    }
}

#[async_trait]
impl AwsService for OrganizationsService {
    fn service_name(&self) -> &str {
        "organizations"
    }

    async fn handle(&self, req: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        match req.action.as_str() {
            "CreateOrganization" => self.create_organization(&req),
            "DescribeOrganization" => self.describe_organization(&req),
            "DeleteOrganization" => self.delete_organization(&req),
            "ListRoots" => self.list_roots(&req),
            "CreateOrganizationalUnit" => self.create_organizational_unit(&req),
            "UpdateOrganizationalUnit" => self.update_organizational_unit(&req),
            "DeleteOrganizationalUnit" => self.delete_organizational_unit(&req),
            "DescribeOrganizationalUnit" => self.describe_organizational_unit(&req),
            "ListOrganizationalUnitsForParent" => self.list_organizational_units_for_parent(&req),
            "ListAccounts" => self.list_accounts(&req),
            "ListAccountsForParent" => self.list_accounts_for_parent(&req),
            "DescribeAccount" => self.describe_account(&req),
            "MoveAccount" => self.move_account(&req),
            _ => Err(AwsServiceError::action_not_implemented(
                "organizations",
                &req.action,
            )),
        }
    }

    fn supported_actions(&self) -> &[&str] {
        ORGANIZATIONS_ACTIONS
    }
}

fn organizations_not_in_use() -> AwsServiceError {
    AwsServiceError::aws_error(
        StatusCode::BAD_REQUEST,
        "AWSOrganizationsNotInUseException",
        "Your account is not a member of an organization.",
    )
}

fn ou_payload(ou: &OrganizationalUnit) -> Value {
    json!({
        "Id": ou.id,
        "Arn": ou.arn,
        "Name": ou.name,
    })
}

fn account_payload(account: &MemberAccount) -> Value {
    json!({
        "Id": account.id,
        "Arn": account.arn,
        "Email": account.email,
        "Name": account.name,
        "Status": account.status,
        "JoinedMethod": account.joined_method,
        "JoinedTimestamp": account.joined_timestamp.timestamp() as f64,
    })
}

fn required_str<'a>(body: &'a Value, key: &str) -> Result<&'a str, AwsServiceError> {
    body.get(key).and_then(|v| v.as_str()).ok_or_else(|| {
        AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "InvalidInputException",
            format!("Missing required parameter: {key}"),
        )
    })
}

fn org_error_to_aws(err: OrgError) -> AwsServiceError {
    match err {
        OrgError::ParentNotFound(id) => AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "ParentNotFoundException",
            format!("The parent with id {id} was not found."),
        ),
        OrgError::DuplicateOrganizationalUnit(name) => AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "DuplicateOrganizationalUnitException",
            format!("An organizational unit named {name} already exists under this parent."),
        ),
        OrgError::OrganizationalUnitNotFound(id) => AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "OrganizationalUnitNotFoundException",
            format!("The organizational unit with id {id} was not found."),
        ),
        OrgError::OrganizationalUnitNotEmpty(id) => AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "OrganizationalUnitNotEmptyException",
            format!("The organizational unit {id} still contains accounts or child OUs."),
        ),
        OrgError::AccountNotFound(id) => AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "AccountNotFoundException",
            format!("The account with id {id} was not found."),
        ),
        OrgError::SourceParentNotFound(id) => AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "SourceParentNotFoundException",
            format!("The source parent {id} does not contain this account."),
        ),
        OrgError::DestinationParentNotFound(id) => AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "DestinationParentNotFoundException",
            format!("The destination parent {id} does not exist."),
        ),
    }
}

fn organization_payload(org: &OrganizationState) -> Value {
    json!({
        "Id": org.org_id,
        "Arn": org.org_arn,
        "FeatureSet": org.feature_set,
        "MasterAccountArn": org.management_account_arn,
        "MasterAccountId": org.management_account_id,
        "MasterAccountEmail": org.management_account_email,
        "AvailablePolicyTypes": [
            {"Type": "SERVICE_CONTROL_POLICY", "Status": "ENABLED"}
        ],
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use bytes::Bytes;
    use http::{HeaderMap, Method};
    use std::collections::HashMap;

    fn req_with(account: &str, action: &str, body: Value) -> AwsRequest {
        AwsRequest {
            service: "organizations".to_string(),
            action: action.to_string(),
            region: "us-east-1".to_string(),
            account_id: account.to_string(),
            request_id: "test".to_string(),
            headers: HeaderMap::new(),
            query_params: HashMap::new(),
            body: Bytes::from(serde_json::to_vec(&body).unwrap()),
            path_segments: vec![],
            raw_path: String::new(),
            raw_query: String::new(),
            method: Method::POST,
            is_query_protocol: false,
            access_key_id: None,
            principal: None,
        }
    }

    fn body_json(resp: &AwsResponse) -> Value {
        serde_json::from_slice(resp.body.expect_bytes()).unwrap()
    }

    fn expect_err(r: Result<AwsResponse, AwsServiceError>) -> AwsServiceError {
        match r {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        }
    }

    #[tokio::test]
    async fn create_organization_succeeds_once() {
        let (svc, state) = OrganizationsService::shared();
        let resp = svc
            .handle(req_with("111111111111", "CreateOrganization", json!({})))
            .await
            .unwrap();
        assert_eq!(resp.status, StatusCode::OK);
        let v = body_json(&resp);
        assert_eq!(v["Organization"]["MasterAccountId"], "111111111111");
        assert!(state.read().is_some());
    }

    #[tokio::test]
    async fn create_organization_twice_errors() {
        let (svc, _state) = OrganizationsService::shared();
        svc.handle(req_with("111111111111", "CreateOrganization", json!({})))
            .await
            .unwrap();
        let err = expect_err(
            svc.handle(req_with("222222222222", "CreateOrganization", json!({})))
                .await,
        );
        assert_eq!(err.code(), "AlreadyInOrganizationException");
    }

    #[tokio::test]
    async fn describe_without_org_errors() {
        let (svc, _state) = OrganizationsService::shared();
        let err = expect_err(
            svc.handle(req_with("111111111111", "DescribeOrganization", json!({})))
                .await,
        );
        assert_eq!(err.code(), "AWSOrganizationsNotInUseException");
    }

    #[tokio::test]
    async fn describe_round_trips_create() {
        let (svc, _state) = OrganizationsService::shared();
        svc.handle(req_with("111111111111", "CreateOrganization", json!({})))
            .await
            .unwrap();
        let resp = svc
            .handle(req_with("111111111111", "DescribeOrganization", json!({})))
            .await
            .unwrap();
        let v = body_json(&resp);
        assert_eq!(v["Organization"]["MasterAccountId"], "111111111111");
        assert_eq!(v["Organization"]["FeatureSet"], "ALL");
    }

    #[tokio::test]
    async fn non_member_describe_returns_not_in_use() {
        let (svc, _state) = OrganizationsService::shared();
        svc.handle(req_with("111111111111", "CreateOrganization", json!({})))
            .await
            .unwrap();
        let err = expect_err(
            svc.handle(req_with("222222222222", "DescribeOrganization", json!({})))
                .await,
        );
        assert_eq!(err.code(), "AWSOrganizationsNotInUseException");
    }

    #[tokio::test]
    async fn non_member_delete_returns_not_in_use() {
        let (svc, _state) = OrganizationsService::shared();
        svc.handle(req_with("111111111111", "CreateOrganization", json!({})))
            .await
            .unwrap();
        let err = expect_err(
            svc.handle(req_with("222222222222", "DeleteOrganization", json!({})))
                .await,
        );
        assert_eq!(err.code(), "AWSOrganizationsNotInUseException");
    }

    #[tokio::test]
    async fn member_non_management_delete_returns_access_denied() {
        let (svc, state) = OrganizationsService::shared();
        svc.handle(req_with("111111111111", "CreateOrganization", json!({})))
            .await
            .unwrap();
        // Simulate Batch 2 membership by enrolling a second account
        // directly in state (auto-enrollment lands in Batch 2).
        {
            let mut guard = state.write();
            let org = guard.as_mut().unwrap();
            let account_id = "222222222222".to_string();
            let parent_id = org.root_id.clone();
            let org_id = org.org_id.clone();
            let arn = format!(
                "arn:aws:organizations::111111111111:account/{}/{}",
                org_id, &account_id
            );
            org.accounts.insert(
                account_id.clone(),
                crate::state::MemberAccount {
                    id: account_id.clone(),
                    arn,
                    email: "member@example.com".to_string(),
                    name: "member".to_string(),
                    status: "ACTIVE".to_string(),
                    joined_method: "INVITED".to_string(),
                    joined_timestamp: chrono::Utc::now(),
                    parent_id,
                },
            );
        }
        let err = expect_err(
            svc.handle(req_with("222222222222", "DeleteOrganization", json!({})))
                .await,
        );
        assert_eq!(err.code(), "AccessDeniedException");
    }

    #[tokio::test]
    async fn delete_clears_state() {
        let (svc, state) = OrganizationsService::shared();
        svc.handle(req_with("111111111111", "CreateOrganization", json!({})))
            .await
            .unwrap();
        svc.handle(req_with("111111111111", "DeleteOrganization", json!({})))
            .await
            .unwrap();
        assert!(state.read().is_none());
    }

    #[tokio::test]
    async fn create_with_consolidated_billing_rejected() {
        let (svc, _state) = OrganizationsService::shared();
        let err = expect_err(
            svc.handle(req_with(
                "111111111111",
                "CreateOrganization",
                json!({"FeatureSet": "CONSOLIDATED_BILLING"}),
            ))
            .await,
        );
        assert_eq!(err.code(), "UnsupportedAPIEndpointException");
    }
}
