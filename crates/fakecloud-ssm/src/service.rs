use std::collections::HashMap;

use async_trait::async_trait;
use chrono::Utc;
use http::StatusCode;
use serde_json::{json, Value};

use fakecloud_core::service::{AwsRequest, AwsResponse, AwsService, AwsServiceError};

use crate::state::{SharedSsmState, SsmParameter, SsmParameterVersion};

pub struct SsmService {
    state: SharedSsmState,
}

impl SsmService {
    pub fn new(state: SharedSsmState) -> Self {
        Self { state }
    }
}

#[async_trait]
impl AwsService for SsmService {
    fn service_name(&self) -> &str {
        "ssm"
    }

    async fn handle(&self, req: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        match req.action.as_str() {
            "PutParameter" => self.put_parameter(&req),
            "GetParameter" => self.get_parameter(&req),
            "GetParameters" => self.get_parameters(&req),
            "GetParametersByPath" => self.get_parameters_by_path(&req),
            "DeleteParameter" => self.delete_parameter(&req),
            "DeleteParameters" => self.delete_parameters(&req),
            "DescribeParameters" => self.describe_parameters(&req),
            "GetParameterHistory" => self.get_parameter_history(&req),
            "AddTagsToResource" => self.add_tags_to_resource(&req),
            "RemoveTagsFromResource" => self.remove_tags_from_resource(&req),
            "ListTagsForResource" => self.list_tags_for_resource(&req),
            "LabelParameterVersion" => self.label_parameter_version(&req),
            _ => Err(AwsServiceError::action_not_implemented("ssm", &req.action)),
        }
    }

    fn supported_actions(&self) -> &[&str] {
        &[
            "PutParameter",
            "GetParameter",
            "GetParameters",
            "GetParametersByPath",
            "DeleteParameter",
            "DeleteParameters",
            "DescribeParameters",
            "GetParameterHistory",
            "AddTagsToResource",
            "RemoveTagsFromResource",
            "ListTagsForResource",
            "LabelParameterVersion",
        ]
    }
}

fn parse_body(req: &AwsRequest) -> Value {
    serde_json::from_slice(&req.body).unwrap_or(Value::Object(Default::default()))
}

fn json_resp(body: Value) -> AwsResponse {
    AwsResponse::json(StatusCode::OK, serde_json::to_string(&body).unwrap())
}

fn param_to_json(p: &SsmParameter, with_value: bool, with_decryption: bool) -> Value {
    let mut v = json!({
        "Name": p.name,
        "Type": p.param_type,
        "Version": p.version,
        "ARN": p.arn,
        "LastModifiedDate": p.last_modified.timestamp() as f64,
        "DataType": "text",
    });
    if with_value {
        if p.param_type == "SecureString" && !with_decryption {
            v["Value"] = json!("****");
        } else {
            v["Value"] = json!(p.value);
        }
    }
    v
}

impl SsmService {
    fn put_parameter(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"]
            .as_str()
            .ok_or_else(|| missing("Name"))?
            .to_string();
        let value = body["Value"]
            .as_str()
            .ok_or_else(|| missing("Value"))?
            .to_string();
        let param_type = body["Type"].as_str().unwrap_or("String").to_string();
        let overwrite = body["Overwrite"].as_bool().unwrap_or(false);

        let mut state = self.state.write();

        if let Some(existing) = state.parameters.get_mut(&name) {
            if !overwrite {
                return Err(AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "ParameterAlreadyExists",
                    format!("The parameter {name} already exists."),
                ));
            }
            let now = Utc::now();
            existing.history.push(SsmParameterVersion {
                value: existing.value.clone(),
                version: existing.version,
                last_modified: existing.last_modified,
            });
            existing.version += 1;
            existing.value = value;
            existing.param_type = param_type;
            existing.last_modified = now;

            return Ok(json_resp(json!({
                "Version": existing.version,
                "Tier": "Standard",
            })));
        }

        let now = Utc::now();
        let arn = format!(
            "arn:aws:ssm:{}:{}:parameter{}",
            state.region, state.account_id, name
        );

        let param = SsmParameter {
            name: name.clone(),
            value,
            param_type,
            version: 1,
            arn,
            last_modified: now,
            history: Vec::new(),
            labels: HashMap::new(),
            tags: HashMap::new(),
        };

        state.parameters.insert(name, param);
        Ok(json_resp(json!({
            "Version": 1,
            "Tier": "Standard",
        })))
    }

    fn get_parameter(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"].as_str().ok_or_else(|| missing("Name"))?;
        let with_decryption = body["WithDecryption"].as_bool().unwrap_or(false);

        let state = self.state.read();
        let param = state
            .parameters
            .get(name)
            .ok_or_else(|| param_not_found(name))?;

        Ok(json_resp(json!({
            "Parameter": param_to_json(param, true, with_decryption),
        })))
    }

    fn get_parameters(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let names = body["Names"].as_array().ok_or_else(|| missing("Names"))?;
        let with_decryption = body["WithDecryption"].as_bool().unwrap_or(false);

        let state = self.state.read();
        let mut parameters = Vec::new();
        let mut invalid = Vec::new();

        for name_val in names {
            if let Some(name) = name_val.as_str() {
                if let Some(param) = state.parameters.get(name) {
                    parameters.push(param_to_json(param, true, with_decryption));
                } else {
                    invalid.push(name.to_string());
                }
            }
        }

        Ok(json_resp(json!({
            "Parameters": parameters,
            "InvalidParameters": invalid,
        })))
    }

    fn get_parameters_by_path(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let path = body["Path"].as_str().ok_or_else(|| missing("Path"))?;
        let recursive = body["Recursive"].as_bool().unwrap_or(false);
        let with_decryption = body["WithDecryption"].as_bool().unwrap_or(false);
        let filters = body["ParameterFilters"].as_array().cloned();
        let max_results = body["MaxResults"].as_i64().unwrap_or(10) as usize;
        let next_token_offset: usize = body["NextToken"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let state = self.state.read();
        let prefix = if path.ends_with('/') {
            path.to_string()
        } else {
            format!("{path}/")
        };

        let all_params: Vec<&SsmParameter> = state
            .parameters
            .range(prefix.clone()..)
            .take_while(|(k, _)| k.starts_with(&prefix))
            .filter(|(k, _)| {
                if recursive {
                    true
                } else {
                    !k[prefix.len()..].contains('/')
                }
            })
            .filter(|(_, p)| apply_parameter_filters(p, filters.as_ref()))
            .map(|(_, p)| p)
            .collect();

        let page = &all_params[next_token_offset..];
        let has_more = page.len() > max_results;
        let parameters: Vec<Value> = page
            .iter()
            .take(max_results)
            .map(|p| param_to_json(p, true, with_decryption))
            .collect();

        let mut resp = json!({ "Parameters": parameters });
        if has_more {
            resp["NextToken"] = json!((next_token_offset + max_results).to_string());
        }

        Ok(json_resp(resp))
    }

    fn delete_parameter(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"].as_str().ok_or_else(|| missing("Name"))?;

        let mut state = self.state.write();
        if state.parameters.remove(name).is_none() {
            return Err(param_not_found(name));
        }

        Ok(json_resp(json!({})))
    }

    fn delete_parameters(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let names = body["Names"].as_array().ok_or_else(|| missing("Names"))?;

        let mut state = self.state.write();
        let mut deleted = Vec::new();
        let mut invalid = Vec::new();

        for name_val in names {
            if let Some(name) = name_val.as_str() {
                if state.parameters.remove(name).is_some() {
                    deleted.push(name.to_string());
                } else {
                    invalid.push(name.to_string());
                }
            }
        }

        Ok(json_resp(json!({
            "DeletedParameters": deleted,
            "InvalidParameters": invalid,
        })))
    }

    fn describe_parameters(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let filters = body["ParameterFilters"].as_array().cloned();
        let max_results = body["MaxResults"].as_i64().unwrap_or(10) as usize;
        let next_token_offset: usize = body["NextToken"]
            .as_str()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0);

        let state = self.state.read();
        let all_params: Vec<&SsmParameter> = state
            .parameters
            .values()
            .filter(|p| apply_parameter_filters(p, filters.as_ref()))
            .collect();

        let page = &all_params[next_token_offset..];
        let has_more = page.len() > max_results;
        let parameters: Vec<Value> = page
            .iter()
            .take(max_results)
            .map(|p| param_to_json(p, false, false))
            .collect();

        let mut resp = json!({ "Parameters": parameters });
        if has_more {
            resp["NextToken"] = json!((next_token_offset + max_results).to_string());
        }

        Ok(json_resp(resp))
    }

    fn get_parameter_history(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"].as_str().ok_or_else(|| missing("Name"))?;

        let state = self.state.read();
        let param = state
            .parameters
            .get(name)
            .ok_or_else(|| param_not_found(name))?;

        let mut history: Vec<Value> = param
            .history
            .iter()
            .map(|h| {
                let mut entry = json!({
                    "Name": param.name,
                    "Value": h.value,
                    "Version": h.version,
                    "LastModifiedDate": h.last_modified.timestamp() as f64,
                    "Type": param.param_type,
                });
                if let Some(labels) = param.labels.get(&h.version) {
                    entry["Labels"] = json!(labels);
                }
                entry
            })
            .collect();

        // Include current version
        let mut current = json!({
            "Name": param.name,
            "Value": param.value,
            "Version": param.version,
            "LastModifiedDate": param.last_modified.timestamp() as f64,
            "Type": param.param_type,
        });
        if let Some(labels) = param.labels.get(&param.version) {
            current["Labels"] = json!(labels);
        }
        history.push(current);

        Ok(json_resp(json!({ "Parameters": history })))
    }
    fn add_tags_to_resource(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let resource_id = body["ResourceId"]
            .as_str()
            .ok_or_else(|| missing("ResourceId"))?;
        let tags = body["Tags"].as_array().ok_or_else(|| missing("Tags"))?;

        let mut state = self.state.write();
        let param = state
            .parameters
            .get_mut(resource_id)
            .ok_or_else(|| param_not_found(resource_id))?;

        for tag in tags {
            if let (Some(key), Some(val)) = (tag["Key"].as_str(), tag["Value"].as_str()) {
                param.tags.insert(key.to_string(), val.to_string());
            }
        }

        Ok(json_resp(json!({})))
    }

    fn remove_tags_from_resource(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let resource_id = body["ResourceId"]
            .as_str()
            .ok_or_else(|| missing("ResourceId"))?;
        let tag_keys = body["TagKeys"]
            .as_array()
            .ok_or_else(|| missing("TagKeys"))?;

        let mut state = self.state.write();
        let param = state
            .parameters
            .get_mut(resource_id)
            .ok_or_else(|| param_not_found(resource_id))?;

        for key in tag_keys {
            if let Some(k) = key.as_str() {
                param.tags.remove(k);
            }
        }

        Ok(json_resp(json!({})))
    }

    fn list_tags_for_resource(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let resource_id = body["ResourceId"]
            .as_str()
            .ok_or_else(|| missing("ResourceId"))?;

        let state = self.state.read();
        let param = state
            .parameters
            .get(resource_id)
            .ok_or_else(|| param_not_found(resource_id))?;

        let tags: Vec<Value> = param
            .tags
            .iter()
            .map(|(k, v)| json!({"Key": k, "Value": v}))
            .collect();

        Ok(json_resp(json!({ "TagList": tags })))
    }

    fn label_parameter_version(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"].as_str().ok_or_else(|| missing("Name"))?;
        let labels = body["Labels"].as_array().ok_or_else(|| missing("Labels"))?;
        let version = body["ParameterVersion"].as_i64();

        let mut state = self.state.write();
        let param = state
            .parameters
            .get_mut(name)
            .ok_or_else(|| param_not_found(name))?;

        let target_version = version.unwrap_or(param.version);

        // Validate version exists
        let version_exists = param.version == target_version
            || param.history.iter().any(|h| h.version == target_version);
        if !version_exists {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "ParameterVersionNotFound",
                format!("Version {target_version} of parameter {name} not found."),
            ));
        }

        let label_strings: Vec<String> = labels
            .iter()
            .filter_map(|l| l.as_str().map(|s| s.to_string()))
            .collect();

        // Remove these labels from any other version (labels are unique across versions)
        for existing_labels in param.labels.values_mut() {
            existing_labels.retain(|l| !label_strings.contains(l));
        }
        // Remove empty entries
        param.labels.retain(|_, v| !v.is_empty());

        // Add labels to target version
        let entry = param.labels.entry(target_version).or_default();
        for label in &label_strings {
            if !entry.contains(label) {
                entry.push(label.clone());
            }
        }

        Ok(json_resp(json!({
            "InvalidLabels": [],
            "ParameterVersion": target_version,
        })))
    }
}

/// Apply ParameterFilters to a parameter. Returns true if the parameter passes all filters.
fn apply_parameter_filters(param: &SsmParameter, filters: Option<&Vec<Value>>) -> bool {
    let filters = match filters {
        Some(f) => f,
        None => return true,
    };

    for filter in filters {
        let key = match filter["Key"].as_str() {
            Some(k) => k,
            None => continue,
        };
        let option = filter["Option"].as_str().unwrap_or("Equals");
        let values: Vec<&str> = filter["Values"]
            .as_array()
            .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
            .unwrap_or_default();

        let matches = match key {
            "Name" => match option {
                "BeginsWith" => values.iter().any(|v| param.name.starts_with(v)),
                "Contains" => values.iter().any(|v| param.name.contains(v)),
                "Equals" => values.iter().any(|v| param.name == *v),
                _ => true,
            },
            "Type" => values.iter().any(|v| param.param_type == *v),
            "KeyId" => {
                // KeyId filter is for SecureString parameters with specific KMS keys.
                // In our emulator we don't track KMS keys, so only match if type is SecureString.
                param.param_type == "SecureString"
            }
            _ => true,
        };

        if !matches {
            return false;
        }
    }

    true
}

fn missing(name: &str) -> AwsServiceError {
    AwsServiceError::aws_error(
        StatusCode::BAD_REQUEST,
        "ValidationException",
        format!("The request must contain the parameter {name}"),
    )
}

fn param_not_found(name: &str) -> AwsServiceError {
    AwsServiceError::aws_error(
        StatusCode::BAD_REQUEST,
        "ParameterNotFound",
        format!("Parameter {name} not found."),
    )
}
