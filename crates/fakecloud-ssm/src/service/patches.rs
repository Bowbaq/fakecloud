use std::collections::HashMap;

use http::StatusCode;
use serde_json::{json, Value};

use fakecloud_core::pagination::paginate;
use fakecloud_core::service::{AwsRequest, AwsResponse, AwsServiceError};
use fakecloud_core::validation::*;

use crate::state::{PatchBaseline, PatchGroup, SsmState};

use super::{missing, SsmService};

impl SsmService {
    pub(super) fn create_patch_baseline(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let input = CreatePatchBaselineInput::from_body(&req.json_body())?;

        let mut accounts = self.state.write();
        let state = accounts.get_or_create(&req.account_id);

        // Idempotency: if a baseline with the same ClientToken already exists, return it
        if let Some(ref token) = input.client_token {
            if let Some(existing) = state
                .patch_baselines
                .values()
                .find(|pb| pb.client_token.as_deref() == Some(token))
            {
                return Ok(AwsResponse::ok_json(json!({ "BaselineId": existing.id })));
            }
        }

        let baseline_id = format!(
            "pb-{}",
            &uuid::Uuid::new_v4().to_string().replace('-', "")[..17]
        );

        let pb = PatchBaseline {
            id: baseline_id.clone(),
            name: input.name,
            operating_system: input.operating_system,
            description: input.description,
            approval_rules: input.approval_rules,
            approved_patches: input.approved_patches,
            rejected_patches: input.rejected_patches,
            tags: input.tags,
            approved_patches_compliance_level: input.approved_patches_compliance_level,
            rejected_patches_action: input.rejected_patches_action,
            global_filters: input.global_filters,
            sources: input.sources,
            approved_patches_enable_non_security: input.approved_patches_enable_non_security,
            available_security_updates_compliance_status: input
                .available_security_updates_compliance_status,
            client_token: input.client_token,
        };

        state.patch_baselines.insert(baseline_id.clone(), pb);

        Ok(AwsResponse::ok_json(json!({ "BaselineId": baseline_id })))
    }

    pub(super) fn delete_patch_baseline(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let baseline_id = body["BaselineId"]
            .as_str()
            .ok_or_else(|| missing("BaselineId"))?;
        validate_string_length("BaselineId", baseline_id, 20, 128)?;

        let mut accounts = self.state.write();
        let state = accounts.get_or_create(&req.account_id);
        state.patch_baselines.remove(baseline_id);
        // Also remove any patch group associations
        state
            .patch_groups
            .retain(|pg| pg.baseline_id != baseline_id);

        Ok(AwsResponse::ok_json(json!({ "BaselineId": baseline_id })))
    }

    pub(super) fn describe_patch_baselines(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_optional_range_i64("MaxResults", body["MaxResults"].as_i64(), 1, 100)?;
        let max_results = body["MaxResults"].as_i64().unwrap_or(50) as usize;
        let filters = body["Filters"].as_array();

        let accounts = self.state.read();
        let empty = SsmState::new(&req.account_id, &req.region);
        let state = accounts.get(&req.account_id).unwrap_or(&empty);
        let all_baselines: Vec<Value> = state
            .patch_baselines
            .values()
            .filter(|pb| {
                if let Some(filters) = filters {
                    for filter in filters {
                        let key = filter["Key"].as_str().unwrap_or("");
                        let values: Vec<&str> = filter["Values"]
                            .as_array()
                            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                            .unwrap_or_default();
                        match key {
                            "NAME_PREFIX"
                                if !values.iter().any(|v| pb.name.starts_with(v)) => {
                                    return false;
                                }
                            "OWNER"
                                // We don't track owner, but "Self" means user-created
                                if values.contains(&"AWS") => {
                                    return false;
                                }
                            "OPERATING_SYSTEM"
                                if !values.contains(&pb.operating_system.as_str()) => {
                                    return false;
                                }
                            _ => {}
                        }
                    }
                }
                true
            })
            .map(|pb| {
                let mut v = json!({
                    "BaselineId": pb.id,
                    "BaselineName": pb.name,
                    "OperatingSystem": pb.operating_system,
                    "DefaultBaseline": false,
                });
                if let Some(ref desc) = pb.description {
                    v["BaselineDescription"] = json!(desc);
                }
                v
            })
            .collect();

        let (baselines, next_token) =
            paginate(&all_baselines, body["NextToken"].as_str(), max_results);
        let mut resp = json!({ "BaselineIdentities": baselines });
        if let Some(token) = next_token {
            resp["NextToken"] = json!(token);
        }

        Ok(AwsResponse::ok_json(resp))
    }

    pub(super) fn get_patch_baseline(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let baseline_id = body["BaselineId"]
            .as_str()
            .ok_or_else(|| missing("BaselineId"))?;
        validate_string_length("BaselineId", baseline_id, 20, 128)?;

        let accounts = self.state.read();
        let empty = SsmState::new(&req.account_id, &req.region);
        let state = accounts.get(&req.account_id).unwrap_or(&empty);
        let pb = state.patch_baselines.get(baseline_id).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "DoesNotExistException",
                format!("Baseline {baseline_id} does not exist."),
            )
        })?;

        let mut resp = json!({
            "BaselineId": pb.id,
            "Name": pb.name,
            "OperatingSystem": pb.operating_system,
            "ApprovedPatches": pb.approved_patches,
            "RejectedPatches": pb.rejected_patches,
            "ApprovedPatchesComplianceLevel": pb.approved_patches_compliance_level,
            "RejectedPatchesAction": pb.rejected_patches_action,
            "ApprovedPatchesEnableNonSecurity": pb.approved_patches_enable_non_security,
            "Sources": pb.sources,
            "PatchGroups": state.patch_groups.iter()
                .filter(|pg| pg.baseline_id == baseline_id)
                .map(|pg| pg.patch_group.clone())
                .collect::<Vec<_>>(),
        });
        if let Some(ref desc) = pb.description {
            resp["Description"] = json!(desc);
        }
        if let Some(ref rules) = pb.approval_rules {
            resp["ApprovalRules"] = rules.clone();
        }
        if let Some(ref gf) = pb.global_filters {
            resp["GlobalFilters"] = gf.clone();
        }
        if let Some(ref status) = pb.available_security_updates_compliance_status {
            resp["AvailableSecurityUpdatesComplianceStatus"] = json!(status);
        }

        Ok(AwsResponse::ok_json(resp))
    }

    pub(super) fn register_patch_baseline_for_patch_group(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let baseline_id = body["BaselineId"]
            .as_str()
            .ok_or_else(|| missing("BaselineId"))?
            .to_string();
        validate_string_length("BaselineId", &baseline_id, 20, 128)?;
        let patch_group = body["PatchGroup"]
            .as_str()
            .ok_or_else(|| missing("PatchGroup"))?
            .to_string();
        validate_string_length("PatchGroup", &patch_group, 1, 256)?;

        let mut accounts = self.state.write();
        let state = accounts.get_or_create(&req.account_id);

        // Check baseline exists (AWS returns "Maintenance window" in this error, not "Patch baseline")
        if !state.patch_baselines.contains_key(&baseline_id) {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "DoesNotExistException",
                format!("Maintenance window {baseline_id} does not exist"),
            ));
        }

        // Check if this patch group is already registered to a baseline with same OS
        let os = state.patch_baselines[&baseline_id].operating_system.clone();
        if let Some(existing) = state
            .patch_groups
            .iter()
            .find(|pg| pg.patch_group == patch_group)
        {
            if let Some(existing_pb) = state.patch_baselines.get(&existing.baseline_id) {
                if existing_pb.operating_system == os {
                    return Err(AwsServiceError::aws_error(
                        StatusCode::BAD_REQUEST,
                        "AlreadyExistsException",
                        format!(
                            "Patch Group baseline already has a baseline registered for OperatingSystem {os}."
                        ),
                    ));
                }
            }
        }

        state.patch_groups.push(PatchGroup {
            baseline_id: baseline_id.clone(),
            patch_group: patch_group.clone(),
        });

        Ok(AwsResponse::ok_json(json!({
            "BaselineId": baseline_id,
            "PatchGroup": patch_group,
        })))
    }

    pub(super) fn deregister_patch_baseline_for_patch_group(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let baseline_id = body["BaselineId"]
            .as_str()
            .ok_or_else(|| missing("BaselineId"))?;
        validate_string_length("BaselineId", baseline_id, 20, 128)?;
        let patch_group = body["PatchGroup"]
            .as_str()
            .ok_or_else(|| missing("PatchGroup"))?;
        validate_string_length("PatchGroup", patch_group, 1, 256)?;

        let mut accounts = self.state.write();
        let state = accounts.get_or_create(&req.account_id);

        // Check if the association exists
        let exists = state
            .patch_groups
            .iter()
            .any(|pg| pg.baseline_id == baseline_id && pg.patch_group == patch_group);
        if exists {
            state
                .patch_groups
                .retain(|pg| !(pg.baseline_id == baseline_id && pg.patch_group == patch_group));
        } else {
            // Allow deregistering default baselines (they are implicitly registered)
            let is_default = is_default_patch_baseline(baseline_id);
            if !is_default {
                return Err(AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "DoesNotExistException",
                    "Patch Baseline to be retrieved does not exist.",
                ));
            }
        }

        Ok(AwsResponse::ok_json(json!({
            "BaselineId": baseline_id,
            "PatchGroup": patch_group,
        })))
    }

    pub(super) fn get_patch_baseline_for_patch_group(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let patch_group = body["PatchGroup"]
            .as_str()
            .ok_or_else(|| missing("PatchGroup"))?;
        validate_string_length("PatchGroup", patch_group, 1, 256)?;
        validate_optional_enum(
            "OperatingSystem",
            body["OperatingSystem"].as_str(),
            &[
                "WINDOWS",
                "AMAZON_LINUX",
                "AMAZON_LINUX_2",
                "AMAZON_LINUX_2022",
                "AMAZON_LINUX_2023",
                "UBUNTU",
                "REDHAT_ENTERPRISE_LINUX",
                "SUSE",
                "CENTOS",
                "ORACLE_LINUX",
                "DEBIAN",
                "MACOS",
                "RASPBIAN",
                "ROCKY_LINUX",
                "ALMA_LINUX",
            ],
        )?;
        let operating_system = body["OperatingSystem"].as_str().unwrap_or("WINDOWS");

        let accounts = self.state.read();
        let empty = SsmState::new(&req.account_id, &req.region);
        let state = accounts.get(&req.account_id).unwrap_or(&empty);

        // Find a patch group association matching both patch group and OS
        let found = state.patch_groups.iter().find(|pg| {
            pg.patch_group == patch_group
                && state
                    .patch_baselines
                    .get(&pg.baseline_id)
                    .is_some_and(|pb| pb.operating_system == operating_system)
        });

        if let Some(pg) = found {
            Ok(AwsResponse::ok_json(json!({
                "BaselineId": pg.baseline_id,
                "PatchGroup": pg.patch_group,
                "OperatingSystem": operating_system,
            })))
        } else {
            // Fall back to default baseline for the region/OS
            let mut resp = json!({
                "PatchGroup": patch_group,
                "OperatingSystem": operating_system,
            });
            if let Some(baseline_id) = default_patch_baseline(&req.region, operating_system) {
                resp["BaselineId"] = json!(baseline_id);
            }
            Ok(AwsResponse::ok_json(resp))
        }
    }

    pub(super) fn describe_patch_groups(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_optional_range_i64("MaxResults", body["MaxResults"].as_i64(), 1, 100)?;
        let max_results = body["MaxResults"].as_i64().unwrap_or(50) as usize;
        let filters = body["Filters"].as_array();

        let accounts = self.state.read();
        let empty = SsmState::new(&req.account_id, &req.region);
        let state = accounts.get(&req.account_id).unwrap_or(&empty);
        let all_mappings: Vec<Value> = state
            .patch_groups
            .iter()
            .filter(|pg| {
                if let Some(filters) = filters {
                    for filter in filters {
                        let key = filter["Key"].as_str().unwrap_or("");
                        let values: Vec<&str> = filter["Values"]
                            .as_array()
                            .map(|a| a.iter().filter_map(|v| v.as_str()).collect())
                            .unwrap_or_default();
                        match key {
                            "NAME_PREFIX"
                                if !values.iter().any(|v| pg.patch_group.starts_with(v)) =>
                            {
                                return false;
                            }
                            "OPERATING_SYSTEM" => {
                                if let Some(pb) = state.patch_baselines.get(&pg.baseline_id) {
                                    if !values.contains(&pb.operating_system.as_str()) {
                                        return false;
                                    }
                                }
                            }
                            _ => {}
                        }
                    }
                }
                true
            })
            .map(|pg| {
                let mut baseline_identity = json!({
                    "BaselineId": pg.baseline_id,
                    "DefaultBaseline": false,
                });
                if let Some(pb) = state.patch_baselines.get(&pg.baseline_id) {
                    baseline_identity["BaselineName"] = json!(pb.name);
                    baseline_identity["OperatingSystem"] = json!(pb.operating_system);
                    if let Some(ref desc) = pb.description {
                        baseline_identity["BaselineDescription"] = json!(desc);
                    }
                }
                json!({
                    "PatchGroup": pg.patch_group,
                    "BaselineIdentity": baseline_identity,
                })
            })
            .collect();

        let (mappings, next_token) =
            paginate(&all_mappings, body["NextToken"].as_str(), max_results);
        let mut resp = json!({ "Mappings": mappings });
        if let Some(token) = next_token {
            resp["NextToken"] = json!(token);
        }

        Ok(AwsResponse::ok_json(resp))
    }

    pub(super) fn update_patch_baseline(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let baseline_id = body["BaselineId"]
            .as_str()
            .ok_or_else(|| missing("BaselineId"))?;

        let mut accounts = self.state.write();
        let state = accounts.get_or_create(&req.account_id);
        let pb = state.patch_baselines.get_mut(baseline_id).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "DoesNotExistException",
                format!("Patch baseline {baseline_id} does not exist"),
            )
        })?;

        if let Some(name) = body["Name"].as_str() {
            pb.name = name.to_string();
        }
        if body.get("Description").is_some() {
            pb.description = body["Description"].as_str().map(|s| s.to_string());
        }
        if let Some(rules) = body.get("ApprovalRules") {
            pb.approval_rules = Some(rules.clone());
        }
        if let Some(arr) = body["ApprovedPatches"].as_array() {
            pb.approved_patches = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(arr) = body["RejectedPatches"].as_array() {
            pb.rejected_patches = arr
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(level) = body["ApprovedPatchesComplianceLevel"].as_str() {
            pb.approved_patches_compliance_level = level.to_string();
        }
        if let Some(action) = body["RejectedPatchesAction"].as_str() {
            pb.rejected_patches_action = action.to_string();
        }
        if let Some(gf) = body.get("GlobalFilters") {
            pb.global_filters = Some(gf.clone());
        }
        if let Some(arr) = body["Sources"].as_array() {
            pb.sources = arr.clone();
        }
        if let Some(enable) = body["ApprovedPatchesEnableNonSecurity"].as_bool() {
            pb.approved_patches_enable_non_security = enable;
        }

        let mut resp = json!({
            "BaselineId": pb.id,
            "Name": pb.name,
            "OperatingSystem": pb.operating_system,
            "ApprovedPatches": pb.approved_patches,
            "RejectedPatches": pb.rejected_patches,
            "ApprovedPatchesComplianceLevel": pb.approved_patches_compliance_level,
            "RejectedPatchesAction": pb.rejected_patches_action,
            "ApprovedPatchesEnableNonSecurity": pb.approved_patches_enable_non_security,
            "Sources": pb.sources,
        });
        if let Some(ref desc) = pb.description {
            resp["Description"] = json!(desc);
        }
        if let Some(ref rules) = pb.approval_rules {
            resp["ApprovalRules"] = rules.clone();
        }
        if let Some(ref gf) = pb.global_filters {
            resp["GlobalFilters"] = gf.clone();
        }

        Ok(AwsResponse::ok_json(resp))
    }

    pub(super) fn describe_instance_patch_states(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_optional_range_i64("MaxResults", body["MaxResults"].as_i64(), 10, 100)?;
        let _instance_ids = body["InstanceIds"]
            .as_array()
            .ok_or_else(|| missing("InstanceIds"))?;
        // Return empty - no real instances in emulator
        Ok(AwsResponse::ok_json(json!({ "InstancePatchStates": [] })))
    }

    pub(super) fn describe_instance_patch_states_for_patch_group(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_optional_string_length("PatchGroup", body["PatchGroup"].as_str(), 1, 256)?;
        validate_optional_range_i64("MaxResults", body["MaxResults"].as_i64(), 10, 100)?;
        let _patch_group = body["PatchGroup"]
            .as_str()
            .ok_or_else(|| missing("PatchGroup"))?;
        Ok(AwsResponse::ok_json(json!({ "InstancePatchStates": [] })))
    }

    pub(super) fn describe_instance_patches(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_optional_range_i64("MaxResults", body["MaxResults"].as_i64(), 10, 100)?;
        let _instance_id = body["InstanceId"]
            .as_str()
            .ok_or_else(|| missing("InstanceId"))?;
        Ok(AwsResponse::ok_json(json!({ "Patches": [] })))
    }

    pub(super) fn describe_effective_patches_for_patch_baseline(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_optional_string_length("BaselineId", body["BaselineId"].as_str(), 20, 128)?;
        validate_optional_range_i64("MaxResults", body["MaxResults"].as_i64(), 1, 100)?;
        let _baseline_id = body["BaselineId"]
            .as_str()
            .ok_or_else(|| missing("BaselineId"))?;
        Ok(AwsResponse::ok_json(json!({ "EffectivePatches": [] })))
    }

    pub(super) fn get_deployable_patch_snapshot_for_instance(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_optional_string_length("SnapshotId", body["SnapshotId"].as_str(), 36, 36)?;
        let instance_id = body["InstanceId"]
            .as_str()
            .ok_or_else(|| missing("InstanceId"))?;
        let snapshot_id = body["SnapshotId"]
            .as_str()
            .ok_or_else(|| missing("SnapshotId"))?;

        Ok(AwsResponse::ok_json(json!({
            "InstanceId": instance_id,
            "SnapshotId": snapshot_id,
            "Product": "{}",
            "SnapshotDownloadUrl": "",
        })))
    }

    // ── Resource Data Sync ────────────────────────────────────────

    pub(super) fn describe_patch_group_state(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_optional_string_length("PatchGroup", body["PatchGroup"].as_str(), 1, 256)?;
        let _patch_group = body["PatchGroup"]
            .as_str()
            .ok_or_else(|| missing("PatchGroup"))?;
        Ok(AwsResponse::ok_json(json!({
            "Instances": 0,
            "InstancesWithInstalledPatches": 0,
            "InstancesWithInstalledOtherPatches": 0,
            "InstancesWithInstalledRejectedPatches": 0,
            "InstancesWithInstalledPendingRebootPatches": 0,
            "InstancesWithMissingPatches": 0,
            "InstancesWithFailedPatches": 0,
            "InstancesWithNotApplicablePatches": 0,
            "InstancesWithUnreportedNotApplicablePatches": 0,
            "InstancesWithCriticalNonCompliantPatches": 0,
            "InstancesWithSecurityNonCompliantPatches": 0,
            "InstancesWithOtherNonCompliantPatches": 0,
        })))
    }

    pub(super) fn describe_patch_properties(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_required("OperatingSystem", &body["OperatingSystem"])?;
        validate_optional_enum(
            "OperatingSystem",
            body["OperatingSystem"].as_str(),
            &[
                "WINDOWS",
                "AMAZON_LINUX",
                "AMAZON_LINUX_2",
                "AMAZON_LINUX_2022",
                "UBUNTU",
                "REDHAT_ENTERPRISE_LINUX",
                "SUSE",
                "CENTOS",
                "ORACLE_LINUX",
                "DEBIAN",
                "MACOS",
                "RASPBIAN",
                "ROCKY_LINUX",
                "ALMA_LINUX",
                "AMAZON_LINUX_2023",
            ],
        )?;
        validate_required("Property", &body["Property"])?;
        validate_optional_enum(
            "Property",
            body["Property"].as_str(),
            &[
                "PRODUCT",
                "PRODUCT_FAMILY",
                "CLASSIFICATION",
                "MSRC_SEVERITY",
                "PRIORITY",
                "SEVERITY",
            ],
        )?;
        validate_optional_enum(
            "PatchSet",
            body["PatchSet"].as_str(),
            &["OS", "APPLICATION"],
        )?;
        validate_optional_range_i64("MaxResults", body["MaxResults"].as_i64(), 1, 50)?;
        Ok(AwsResponse::ok_json(json!({ "Properties": [] })))
    }

    pub(super) fn get_default_patch_baseline(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_optional_enum(
            "OperatingSystem",
            body["OperatingSystem"].as_str(),
            &[
                "WINDOWS",
                "AMAZON_LINUX",
                "AMAZON_LINUX_2",
                "AMAZON_LINUX_2022",
                "UBUNTU",
                "REDHAT_ENTERPRISE_LINUX",
                "SUSE",
                "CENTOS",
                "ORACLE_LINUX",
                "DEBIAN",
                "MACOS",
                "RASPBIAN",
                "ROCKY_LINUX",
                "ALMA_LINUX",
                "AMAZON_LINUX_2023",
            ],
        )?;
        let operating_system = body["OperatingSystem"].as_str().unwrap_or("WINDOWS");

        let accounts = self.state.read();
        let empty = SsmState::new(&req.account_id, &req.region);
        let state = accounts.get(&req.account_id).unwrap_or(&empty);

        // Check if a custom default has been registered
        if let Some(ref baseline_id) = state.default_patch_baseline_id {
            return Ok(AwsResponse::ok_json(json!({
                "BaselineId": baseline_id,
                "OperatingSystem": operating_system,
            })));
        }

        // Otherwise look up from defaults
        let baseline_id =
            default_patch_baseline(&state.region, operating_system).unwrap_or_default();
        Ok(AwsResponse::ok_json(json!({
            "BaselineId": baseline_id,
            "OperatingSystem": operating_system,
        })))
    }

    pub(super) fn register_default_patch_baseline(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        let baseline_id = body["BaselineId"]
            .as_str()
            .ok_or_else(|| missing("BaselineId"))?
            .to_string();

        let mut accounts = self.state.write();
        let state = accounts.get_or_create(&req.account_id);

        // Verify baseline exists (custom or default)
        if !state.patch_baselines.contains_key(&baseline_id)
            && !is_default_patch_baseline(&baseline_id)
        {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "DoesNotExistException",
                format!("Patch baseline {baseline_id} does not exist"),
            ));
        }

        state.default_patch_baseline_id = Some(baseline_id.clone());
        Ok(AwsResponse::ok_json(json!({
            "BaselineId": baseline_id,
        })))
    }

    pub(super) fn describe_available_patches(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body = req.json_body();
        validate_optional_range_i64("MaxResults", body["MaxResults"].as_i64(), 1, 100)?;
        Ok(AwsResponse::ok_json(json!({ "Patches": [] })))
    }
}

/// Parsed + validated inputs for `CreatePatchBaseline`.
struct CreatePatchBaselineInput {
    name: String,
    operating_system: String,
    description: Option<String>,
    approval_rules: Option<Value>,
    approved_patches: Vec<String>,
    rejected_patches: Vec<String>,
    approved_patches_compliance_level: String,
    rejected_patches_action: String,
    global_filters: Option<Value>,
    sources: Vec<Value>,
    approved_patches_enable_non_security: bool,
    available_security_updates_compliance_status: Option<String>,
    client_token: Option<String>,
    tags: HashMap<String, String>,
}

impl CreatePatchBaselineInput {
    fn from_body(body: &Value) -> Result<Self, AwsServiceError> {
        let name = body["Name"]
            .as_str()
            .ok_or_else(|| missing("Name"))?
            .to_string();
        validate_string_length("Name", &name, 3, 128)?;
        validate_optional_enum(
            "OperatingSystem",
            body["OperatingSystem"].as_str(),
            &[
                "WINDOWS",
                "AMAZON_LINUX",
                "AMAZON_LINUX_2",
                "AMAZON_LINUX_2022",
                "UBUNTU",
                "REDHAT_ENTERPRISE_LINUX",
                "SUSE",
                "CENTOS",
                "ORACLE_LINUX",
                "DEBIAN",
                "MACOS",
                "RASPBIAN",
                "ROCKY_LINUX",
                "ALMA_LINUX",
                "AMAZON_LINUX_2023",
            ],
        )?;
        validate_optional_string_length("Description", body["Description"].as_str(), 1, 1024)?;
        validate_optional_enum(
            "ApprovedPatchesComplianceLevel",
            body["ApprovedPatchesComplianceLevel"].as_str(),
            &[
                "CRITICAL",
                "HIGH",
                "MEDIUM",
                "LOW",
                "INFORMATIONAL",
                "UNSPECIFIED",
            ],
        )?;
        validate_optional_enum(
            "RejectedPatchesAction",
            body["RejectedPatchesAction"].as_str(),
            &["ALLOW_AS_DEPENDENCY", "BLOCK"],
        )?;
        validate_optional_enum(
            "AvailableSecurityUpdatesComplianceStatus",
            body["AvailableSecurityUpdatesComplianceStatus"].as_str(),
            &["COMPLIANT", "NON_COMPLIANT"],
        )?;
        validate_optional_string_length("ClientToken", body["ClientToken"].as_str(), 1, 64)?;

        let approved_patches: Vec<String> = body["ApprovedPatches"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let rejected_patches: Vec<String> = body["RejectedPatches"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_default();
        let tags: HashMap<String, String> = body["Tags"]
            .as_array()
            .map(|arr| {
                arr.iter()
                    .filter_map(|t| {
                        let k = t["Key"].as_str()?;
                        let v = t["Value"].as_str()?;
                        Some((k.to_string(), v.to_string()))
                    })
                    .collect()
            })
            .unwrap_or_default();

        Ok(Self {
            name,
            operating_system: body["OperatingSystem"]
                .as_str()
                .unwrap_or("WINDOWS")
                .to_string(),
            description: body["Description"].as_str().map(|s| s.to_string()),
            approval_rules: body.get("ApprovalRules").cloned(),
            approved_patches,
            rejected_patches,
            approved_patches_compliance_level: body["ApprovedPatchesComplianceLevel"]
                .as_str()
                .unwrap_or("UNSPECIFIED")
                .to_string(),
            rejected_patches_action: body["RejectedPatchesAction"]
                .as_str()
                .unwrap_or("ALLOW_AS_DEPENDENCY")
                .to_string(),
            global_filters: body.get("GlobalFilters").cloned(),
            sources: body["Sources"].as_array().cloned().unwrap_or_default(),
            approved_patches_enable_non_security: body["ApprovedPatchesEnableNonSecurity"]
                .as_bool()
                .unwrap_or(false),
            available_security_updates_compliance_status: body
                ["AvailableSecurityUpdatesComplianceStatus"]
                .as_str()
                .map(|s| s.to_string()),
            client_token: body["ClientToken"].as_str().map(|s| s.to_string()),
            tags,
        })
    }
}

/// Look up the default patch baseline for a given region and OS.
pub(super) fn default_patch_baseline(region: &str, operating_system: &str) -> Option<String> {
    static DEFAULT_BASELINES: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("../default_baselines.json")).unwrap_or(json!({}))
    });
    DEFAULT_BASELINES
        .get(region)
        .and_then(|r| r.get(operating_system))
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
}

/// Check if a baseline ID is a known default baseline.
pub(super) fn is_default_patch_baseline(baseline_id: &str) -> bool {
    static DEFAULT_BASELINES: std::sync::LazyLock<Value> = std::sync::LazyLock::new(|| {
        serde_json::from_str(include_str!("../default_baselines.json")).unwrap_or(json!({}))
    });
    if let Some(obj) = DEFAULT_BASELINES.as_object() {
        for region_data in obj.values() {
            if let Some(region_obj) = region_data.as_object() {
                for val in region_obj.values() {
                    if val.as_str() == Some(baseline_id) {
                        return true;
                    }
                }
            }
        }
    }
    false
}
