use std::collections::HashMap;
use std::sync::Arc;

use async_trait::async_trait;
use chrono::Utc;
use http::{Method, StatusCode};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use fakecloud_core::service::{AwsRequest, AwsResponse, AwsService, AwsServiceError};

use crate::runtime::ContainerRuntime;
use crate::state::{EventSourceMapping, LambdaFunction, SharedLambdaState};

/// All fields of a `CreateFunction` request, already parsed and
/// defaulted. The code zip (if any) is eagerly base64-decoded so the
/// caller can hash it without doing the decode again.
struct CreateFunctionInput {
    function_name: String,
    runtime: String,
    role: String,
    handler: String,
    description: String,
    timeout: i64,
    memory_size: i64,
    package_type: String,
    tags: HashMap<String, String>,
    environment: HashMap<String, String>,
    architectures: Vec<String>,
    code_zip: Option<Vec<u8>>,
    code_fallback: Vec<u8>,
}

impl CreateFunctionInput {
    fn from_body(body: &Value) -> Result<Self, AwsServiceError> {
        let function_name = body["FunctionName"]
            .as_str()
            .ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "InvalidParameterValueException",
                    "FunctionName is required",
                )
            })?
            .to_string();

        let tags: HashMap<String, String> = body["Tags"]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let environment: HashMap<String, String> = body["Environment"]["Variables"]
            .as_object()
            .map(|m| {
                m.iter()
                    .filter_map(|(k, v)| v.as_str().map(|s| (k.clone(), s.to_string())))
                    .collect()
            })
            .unwrap_or_default();

        let architectures = body["Architectures"]
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
            .unwrap_or_else(|| vec!["x86_64".to_string()]);

        let code_zip: Option<Vec<u8>> = match body["Code"]["ZipFile"].as_str() {
            Some(b64) => Some(
                base64::Engine::decode(&base64::engine::general_purpose::STANDARD, b64).map_err(
                    |_| {
                        AwsServiceError::aws_error(
                            StatusCode::BAD_REQUEST,
                            "InvalidParameterValueException",
                            "Could not decode Code.ZipFile: invalid base64",
                        )
                    },
                )?,
            ),
            None => None,
        };

        let code_fallback = serde_json::to_vec(&body["Code"]).unwrap_or_default();

        Ok(Self {
            function_name,
            runtime: body["Runtime"].as_str().unwrap_or("python3.12").to_string(),
            role: body["Role"].as_str().unwrap_or("").to_string(),
            handler: body["Handler"]
                .as_str()
                .unwrap_or("index.handler")
                .to_string(),
            description: body["Description"].as_str().unwrap_or("").to_string(),
            timeout: body["Timeout"].as_i64().unwrap_or(3),
            memory_size: body["MemorySize"].as_i64().unwrap_or(128),
            package_type: body["PackageType"].as_str().unwrap_or("Zip").to_string(),
            tags,
            environment,
            architectures,
            code_zip,
            code_fallback,
        })
    }
}

pub struct LambdaService {
    state: SharedLambdaState,
    runtime: Option<Arc<ContainerRuntime>>,
}

impl LambdaService {
    pub fn new(state: SharedLambdaState) -> Self {
        Self {
            state,
            runtime: None,
        }
    }

    pub fn with_runtime(mut self, runtime: Arc<ContainerRuntime>) -> Self {
        self.runtime = Some(runtime);
        self
    }

    /// Determine the action from the HTTP method and path segments.
    /// Lambda uses REST-style routing:
    ///   POST   /2015-03-31/functions                         -> CreateFunction
    ///   GET    /2015-03-31/functions                         -> ListFunctions
    ///   GET    /2015-03-31/functions/{name}                  -> GetFunction
    ///   DELETE /2015-03-31/functions/{name}                  -> DeleteFunction
    ///   POST   /2015-03-31/functions/{name}/invocations      -> Invoke
    ///   POST   /2015-03-31/functions/{name}/versions         -> PublishVersion
    ///   POST   /2015-03-31/event-source-mappings             -> CreateEventSourceMapping
    ///   GET    /2015-03-31/event-source-mappings             -> ListEventSourceMappings
    ///   GET    /2015-03-31/event-source-mappings/{uuid}      -> GetEventSourceMapping
    ///   DELETE /2015-03-31/event-source-mappings/{uuid}      -> DeleteEventSourceMapping
    fn resolve_action(req: &AwsRequest) -> Option<(&'static str, Option<String>)> {
        let segs = &req.path_segments;
        if segs.is_empty() || segs[0] != "2015-03-31" {
            return None;
        }

        // Second segment is the collection (`functions` /
        // `event-source-mappings`); third is the resource name when
        // present. Bind the resource name once so the match arms don't
        // each repeat `segs[2].clone()`.
        let collection = segs.get(1).map(|s| s.as_str());
        let resource = segs.get(2).map(|s| s.to_string());

        let action = match (
            &req.method,
            segs.len(),
            collection,
            segs.get(3).map(|s| s.as_str()),
        ) {
            // /2015-03-31/functions
            (&Method::POST, 2, Some("functions"), _) => "CreateFunction",
            (&Method::GET, 2, Some("functions"), _) => "ListFunctions",
            // /2015-03-31/functions/{name}
            (&Method::GET, 3, Some("functions"), _) => "GetFunction",
            (&Method::DELETE, 3, Some("functions"), _) => "DeleteFunction",
            // /2015-03-31/functions/{name}/invocations
            (&Method::POST, 4, Some("functions"), Some("invocations")) => "Invoke",
            // /2015-03-31/functions/{name}/versions
            (&Method::POST, 4, Some("functions"), Some("versions")) => "PublishVersion",
            // /2015-03-31/functions/{name}/policy
            (&Method::POST, 4, Some("functions"), Some("policy")) => "AddPermission",
            (&Method::GET, 4, Some("functions"), Some("policy")) => "GetPolicy",
            // /2015-03-31/functions/{name}/policy/{statement-id}
            (&Method::DELETE, 5, Some("functions"), Some("policy")) => "RemovePermission",
            // /2015-03-31/event-source-mappings
            (&Method::POST, 2, Some("event-source-mappings"), _) => "CreateEventSourceMapping",
            (&Method::GET, 2, Some("event-source-mappings"), _) => "ListEventSourceMappings",
            // /2015-03-31/event-source-mappings/{uuid}
            (&Method::GET, 3, Some("event-source-mappings"), _) => "GetEventSourceMapping",
            (&Method::DELETE, 3, Some("event-source-mappings"), _) => "DeleteEventSourceMapping",
            _ => return None,
        };

        Some((action, resource))
    }

    fn create_function(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body: Value = serde_json::from_slice(&req.body).unwrap_or_default();
        let input = CreateFunctionInput::from_body(&body)?;

        let mut state = self.state.write();

        if state.functions.contains_key(&input.function_name) {
            return Err(AwsServiceError::aws_error(
                StatusCode::CONFLICT,
                "ResourceConflictException",
                format!("Function already exist: {}", input.function_name),
            ));
        }

        // Hash the actual ZIP bytes when available, falling back to the
        // raw Code JSON so image-based functions still get a stable id.
        let code_bytes = input.code_zip.as_deref().unwrap_or(&input.code_fallback);
        let mut hasher = Sha256::new();
        hasher.update(code_bytes);
        let hash = hasher.finalize();
        let code_sha256 = base64::Engine::encode(&base64::engine::general_purpose::STANDARD, hash);
        let code_size = code_bytes.len() as i64;

        let function_arn = format!(
            "arn:aws:lambda:{}:{}:function:{}",
            state.region, state.account_id, input.function_name
        );
        let now = Utc::now();

        let func = LambdaFunction {
            function_name: input.function_name.clone(),
            function_arn,
            runtime: input.runtime,
            role: input.role,
            handler: input.handler,
            description: input.description,
            timeout: input.timeout,
            memory_size: input.memory_size,
            code_sha256,
            code_size,
            version: "$LATEST".to_string(),
            last_modified: now,
            tags: input.tags,
            environment: input.environment,
            architectures: input.architectures,
            package_type: input.package_type,
            code_zip: input.code_zip,
            policy: None,
        };

        let response = self.function_config_json(&func);

        state.functions.insert(input.function_name, func);

        Ok(AwsResponse::json(StatusCode::CREATED, response.to_string()))
    }

    fn get_function(&self, function_name: &str) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let func = state.functions.get(function_name).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!(
                    "Function not found: arn:aws:lambda:{}:{}:function:{}",
                    state.region, state.account_id, function_name
                ),
            )
        })?;

        let config = self.function_config_json(func);
        let response = json!({
            "Code": {
                "Location": format!("https://awslambda-{}-tasks.s3.{}.amazonaws.com/stub",
                    func.function_arn.split(':').nth(3).unwrap_or("us-east-1"),
                    func.function_arn.split(':').nth(3).unwrap_or("us-east-1")),
                "RepositoryType": "S3"
            },
            "Configuration": config,
            "Tags": func.tags,
        });

        Ok(AwsResponse::json(StatusCode::OK, response.to_string()))
    }

    fn delete_function(&self, function_name: &str) -> Result<AwsResponse, AwsServiceError> {
        let mut state = self.state.write();
        let region = state.region.clone();
        let account_id = state.account_id.clone();
        if state.functions.remove(function_name).is_none() {
            return Err(AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!(
                    "Function not found: arn:aws:lambda:{}:{}:function:{}",
                    region, account_id, function_name
                ),
            ));
        }

        // Clean up any running container for this function
        if let Some(ref runtime) = self.runtime {
            let rt = runtime.clone();
            let name = function_name.to_string();
            tokio::spawn(async move { rt.stop_container(&name).await });
        }

        Ok(AwsResponse::json(StatusCode::NO_CONTENT, ""))
    }

    fn list_functions(&self) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let functions: Vec<Value> = state
            .functions
            .values()
            .map(|f| self.function_config_json(f))
            .collect();

        let response = json!({
            "Functions": functions,
        });

        Ok(AwsResponse::json(StatusCode::OK, response.to_string()))
    }

    async fn invoke(
        &self,
        function_name: &str,
        payload: &[u8],
    ) -> Result<AwsResponse, AwsServiceError> {
        let func = {
            let state = self.state.read();
            state.functions.get(function_name).cloned().ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::NOT_FOUND,
                    "ResourceNotFoundException",
                    format!(
                        "Function not found: arn:aws:lambda:{}:{}:function:{}",
                        state.region, state.account_id, function_name
                    ),
                )
            })?
        };

        let runtime = self.runtime.as_ref().ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "ServiceException",
                "Docker/Podman is required for Lambda execution but is not available",
            )
        })?;

        if func.code_zip.is_none() {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "InvalidParameterValueException",
                "Function has no deployment package",
            ));
        }

        match runtime.invoke(&func, payload).await {
            Ok(response_bytes) => {
                let mut resp = AwsResponse::json(StatusCode::OK, response_bytes);
                resp.headers.insert(
                    http::header::HeaderName::from_static("x-amz-executed-version"),
                    http::header::HeaderValue::from_static("$LATEST"),
                );
                Ok(resp)
            }
            Err(e) => {
                tracing::error!(function = %function_name, error = %e, "Lambda invocation failed");
                Err(AwsServiceError::aws_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "ServiceException",
                    format!("Lambda execution failed: {e}"),
                ))
            }
        }
    }

    fn publish_version(&self, function_name: &str) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let func = state.functions.get(function_name).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!(
                    "Function not found: arn:aws:lambda:{}:{}:function:{}",
                    state.region, state.account_id, function_name
                ),
            )
        })?;

        let mut config = self.function_config_json(func);
        // Stub: always return version "1"
        config["Version"] = json!("1");
        config["FunctionArn"] = json!(format!("{}:1", func.function_arn));

        Ok(AwsResponse::json(StatusCode::CREATED, config.to_string()))
    }

    fn create_event_source_mapping(
        &self,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body: Value = serde_json::from_slice(&req.body).unwrap_or_default();
        let event_source_arn = body["EventSourceArn"]
            .as_str()
            .ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "InvalidParameterValueException",
                    "EventSourceArn is required",
                )
            })?
            .to_string();

        let function_name = body["FunctionName"]
            .as_str()
            .ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "InvalidParameterValueException",
                    "FunctionName is required",
                )
            })?
            .to_string();

        let mut state = self.state.write();

        // Resolve function name to ARN
        let function_arn = if function_name.starts_with("arn:") {
            function_name.clone()
        } else {
            let func = state.functions.get(&function_name).ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::NOT_FOUND,
                    "ResourceNotFoundException",
                    format!(
                        "Function not found: arn:aws:lambda:{}:{}:function:{}",
                        state.region, state.account_id, function_name
                    ),
                )
            })?;
            func.function_arn.clone()
        };

        let batch_size = body["BatchSize"].as_i64().unwrap_or(10);
        let enabled = body["Enabled"].as_bool().unwrap_or(true);
        let mapping_uuid = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();

        let mapping = EventSourceMapping {
            uuid: mapping_uuid.clone(),
            function_arn: function_arn.clone(),
            event_source_arn: event_source_arn.clone(),
            batch_size,
            enabled,
            state: if enabled {
                "Enabled".to_string()
            } else {
                "Disabled".to_string()
            },
            last_modified: now,
        };

        let response = self.event_source_mapping_json(&mapping);
        state.event_source_mappings.insert(mapping_uuid, mapping);

        Ok(AwsResponse::json(
            StatusCode::ACCEPTED,
            response.to_string(),
        ))
    }

    fn list_event_source_mappings(&self) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let mappings: Vec<Value> = state
            .event_source_mappings
            .values()
            .map(|m| self.event_source_mapping_json(m))
            .collect();

        let response = json!({
            "EventSourceMappings": mappings,
        });

        Ok(AwsResponse::json(StatusCode::OK, response.to_string()))
    }

    fn get_event_source_mapping(&self, uuid: &str) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let mapping = state.event_source_mappings.get(uuid).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("The resource you requested does not exist. (Service: Lambda, Status Code: 404, Request ID: {uuid})"),
            )
        })?;

        let response = self.event_source_mapping_json(mapping);
        Ok(AwsResponse::json(StatusCode::OK, response.to_string()))
    }

    fn delete_event_source_mapping(&self, uuid: &str) -> Result<AwsResponse, AwsServiceError> {
        let mut state = self.state.write();
        let mapping = state.event_source_mappings.remove(uuid).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("The resource you requested does not exist. (Service: Lambda, Status Code: 404, Request ID: {uuid})"),
            )
        })?;

        let mut response = self.event_source_mapping_json(&mapping);
        response["State"] = json!("Deleting");
        Ok(AwsResponse::json(
            StatusCode::ACCEPTED,
            response.to_string(),
        ))
    }

    fn function_config_json(&self, func: &LambdaFunction) -> Value {
        let mut env_vars = json!({});
        if !func.environment.is_empty() {
            env_vars = json!({ "Variables": func.environment });
        }

        json!({
            "FunctionName": func.function_name,
            "FunctionArn": func.function_arn,
            "Runtime": func.runtime,
            "Role": func.role,
            "Handler": func.handler,
            "Description": func.description,
            "Timeout": func.timeout,
            "MemorySize": func.memory_size,
            "CodeSha256": func.code_sha256,
            "CodeSize": func.code_size,
            "Version": func.version,
            "LastModified": func.last_modified.format("%Y-%m-%dT%H:%M:%S%.3f+0000").to_string(),
            "PackageType": func.package_type,
            "Architectures": func.architectures,
            "Environment": env_vars,
            "State": "Active",
            "LastUpdateStatus": "Successful",
            "TracingConfig": { "Mode": "PassThrough" },
            "RevisionId": uuid::Uuid::new_v4().to_string(),
        })
    }

    fn event_source_mapping_json(&self, mapping: &EventSourceMapping) -> Value {
        json!({
            "UUID": mapping.uuid,
            "FunctionArn": mapping.function_arn,
            "EventSourceArn": mapping.event_source_arn,
            "BatchSize": mapping.batch_size,
            "State": mapping.state,
            "LastModified": mapping.last_modified.timestamp_millis() as f64 / 1000.0,
        })
    }

    /// Grant a permission on a Lambda function by appending a
    /// statement to its resource-based policy.
    ///
    /// Mirrors AWS: the caller passes `(StatementId, Action,
    /// Principal, SourceArn?, SourceAccount?)` and the service
    /// composes a canonical policy document so that the existing
    /// evaluator can read it without a Lambda-specific fork. Per the
    /// S3 rollout's #427 evaluator, `SourceArn` becomes an `ArnLike`
    /// Condition and `SourceAccount` becomes a `StringEquals`
    /// Condition — both are already supported by the Phase 2 operator
    /// set, so the permission gate behaves end-to-end without any new
    /// evaluator code.
    fn add_permission(
        &self,
        function_name: &str,
        req: &AwsRequest,
    ) -> Result<AwsResponse, AwsServiceError> {
        let body: Value = serde_json::from_slice(&req.body).unwrap_or_default();
        let statement_id = body
            .get("StatementId")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "InvalidParameterValueException",
                    "StatementId is required",
                )
            })?
            .to_string();
        let action = body
            .get("Action")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "InvalidParameterValueException",
                    "Action is required",
                )
            })?
            .to_string();
        let principal_raw = body
            .get("Principal")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "InvalidParameterValueException",
                    "Principal is required",
                )
            })?
            .to_string();
        let source_arn = body
            .get("SourceArn")
            .and_then(|v| v.as_str())
            .map(str::to_string);
        let source_account = body
            .get("SourceAccount")
            .and_then(|v| v.as_str())
            .map(str::to_string);

        let mut state = self.state.write();
        let func = state.functions.get_mut(function_name).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("Function not found: {function_name}"),
            )
        })?;

        // Load current policy or seed a fresh canonical doc. Any
        // stored blob that doesn't parse as a JSON object is treated
        // as corrupt and replaced — `AddPermission` is the only
        // mutation path for this field and it always writes valid
        // JSON, so seeing a non-object here means something else
        // wrote garbage, and silently propagating it would make
        // later reads harder to debug.
        let mut doc: Value = func
            .policy
            .as_deref()
            .and_then(|s| serde_json::from_str::<Value>(s).ok())
            .filter(|v| v.is_object())
            .unwrap_or_else(|| json!({"Version": "2012-10-17", "Statement": []}));

        // Ensure Statement is an array so we can push into it.
        if !doc.get("Statement").map(|s| s.is_array()).unwrap_or(false) {
            doc["Statement"] = json!([]);
        }
        let statements = doc["Statement"].as_array_mut().unwrap();

        // Reject duplicate StatementId — matches AWS's
        // ResourceConflictException.
        if statements
            .iter()
            .any(|s| s.get("Sid").and_then(|v| v.as_str()) == Some(statement_id.as_str()))
        {
            return Err(AwsServiceError::aws_error(
                StatusCode::CONFLICT,
                "ResourceConflictException",
                format!("The statement id ({statement_id}) provided already exists"),
            ));
        }

        // Canonicalize Principal: a service host string becomes
        // `{"Service": "<host>"}`, an account-id or ARN becomes
        // `{"AWS": "<raw>"}`. AWS accepts both shapes on the wire;
        // storing the object form uniformly means the existing
        // evaluator path handles everything without reading back the
        // raw input.
        let principal_value =
            if principal_raw.ends_with(".amazonaws.com") || principal_raw.contains(".amazon") {
                json!({ "Service": principal_raw })
            } else {
                json!({ "AWS": principal_raw })
            };

        // Emit SourceArn / SourceAccount as Condition keys so the
        // existing Phase 2 ArnLike / StringEquals operators gate the
        // grant without new evaluator code.
        let mut condition = serde_json::Map::new();
        if let Some(arn) = source_arn.as_ref() {
            condition.insert("ArnLike".to_string(), json!({ "aws:SourceArn": arn }));
        }
        if let Some(acct) = source_account.as_ref() {
            condition.insert(
                "StringEquals".to_string(),
                json!({ "aws:SourceAccount": acct }),
            );
        }

        let mut new_statement = serde_json::Map::new();
        new_statement.insert("Sid".to_string(), json!(statement_id));
        new_statement.insert("Effect".to_string(), json!("Allow"));
        new_statement.insert("Principal".to_string(), principal_value);
        new_statement.insert("Action".to_string(), json!(format!("lambda:{action}")));
        new_statement.insert("Resource".to_string(), json!(func.function_arn));
        if !condition.is_empty() {
            new_statement.insert("Condition".to_string(), Value::Object(condition));
        }
        let statement_json = Value::Object(new_statement);
        statements.push(statement_json.clone());

        func.policy = Some(serde_json::to_string(&doc).unwrap());

        Ok(AwsResponse::json(
            StatusCode::CREATED,
            json!({ "Statement": serde_json::to_string(&statement_json).unwrap() }).to_string(),
        ))
    }

    fn remove_permission(
        &self,
        function_name: &str,
        statement_id: &str,
    ) -> Result<AwsResponse, AwsServiceError> {
        let mut state = self.state.write();
        let func = state.functions.get_mut(function_name).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("Function not found: {function_name}"),
            )
        })?;
        let policy_str = func.policy.as_deref().ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("No policy is associated with function {function_name}"),
            )
        })?;
        let mut doc: Value = serde_json::from_str(policy_str).map_err(|_| {
            AwsServiceError::aws_error(
                StatusCode::INTERNAL_SERVER_ERROR,
                "InternalError",
                "stored resource policy is not valid JSON",
            )
        })?;
        let statements = doc
            .get_mut("Statement")
            .and_then(|s| s.as_array_mut())
            .ok_or_else(|| {
                AwsServiceError::aws_error(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "InternalError",
                    "stored resource policy has no Statement array",
                )
            })?;
        let before = statements.len();
        statements.retain(|s| s.get("Sid").and_then(|v| v.as_str()) != Some(statement_id));
        if statements.len() == before {
            return Err(AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("Statement {statement_id} is not found in resource policy"),
            ));
        }
        // Leave an empty {"Statement":[]} behind rather than clearing
        // the field to None — AWS's GetPolicy keeps returning the
        // (empty) doc until the function itself is deleted.
        func.policy = Some(serde_json::to_string(&doc).unwrap());
        Ok(AwsResponse::json(StatusCode::NO_CONTENT, String::new()))
    }

    fn get_policy(&self, function_name: &str) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let func = state.functions.get(function_name).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("Function not found: {function_name}"),
            )
        })?;
        let policy = func.policy.as_deref().ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("No policy is associated with function {function_name}"),
            )
        })?;
        Ok(AwsResponse::json(
            StatusCode::OK,
            json!({
                "Policy": policy,
                "RevisionId": uuid::Uuid::new_v4().to_string(),
            })
            .to_string(),
        ))
    }
}

#[async_trait]
impl AwsService for LambdaService {
    fn service_name(&self) -> &str {
        "lambda"
    }

    async fn handle(&self, req: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let (action, resource_name) = Self::resolve_action(&req).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "UnknownOperationException",
                format!("Unknown operation: {} {}", req.method, req.raw_path),
            )
        })?;

        match action {
            "CreateFunction" => self.create_function(&req),
            "ListFunctions" => self.list_functions(),
            "GetFunction" => self.get_function(resource_name.as_deref().unwrap_or("")),
            "DeleteFunction" => self.delete_function(resource_name.as_deref().unwrap_or("")),
            "Invoke" => {
                self.invoke(resource_name.as_deref().unwrap_or(""), &req.body)
                    .await
            }
            "PublishVersion" => self.publish_version(resource_name.as_deref().unwrap_or("")),
            "AddPermission" => self.add_permission(resource_name.as_deref().unwrap_or(""), &req),
            "GetPolicy" => self.get_policy(resource_name.as_deref().unwrap_or("")),
            "RemovePermission" => {
                // Path: /2015-03-31/functions/{name}/policy/{sid}
                let sid = req.path_segments.get(4).cloned().unwrap_or_default();
                self.remove_permission(resource_name.as_deref().unwrap_or(""), &sid)
            }
            "CreateEventSourceMapping" => self.create_event_source_mapping(&req),
            "ListEventSourceMappings" => self.list_event_source_mappings(),
            "GetEventSourceMapping" => {
                self.get_event_source_mapping(resource_name.as_deref().unwrap_or(""))
            }
            "DeleteEventSourceMapping" => {
                self.delete_event_source_mapping(resource_name.as_deref().unwrap_or(""))
            }
            _ => Err(AwsServiceError::action_not_implemented("lambda", action)),
        }
    }

    fn supported_actions(&self) -> &[&str] {
        &[
            "CreateFunction",
            "GetFunction",
            "DeleteFunction",
            "ListFunctions",
            "Invoke",
            "PublishVersion",
            "AddPermission",
            "RemovePermission",
            "GetPolicy",
            "CreateEventSourceMapping",
            "ListEventSourceMappings",
            "GetEventSourceMapping",
            "DeleteEventSourceMapping",
        ]
    }

    fn iam_enforceable(&self) -> bool {
        true
    }

    /// Lambda resources are function ARNs. Function-scoped ops
    /// resolve the target ARN from the path; list ops target `*`
    /// (the whole service), matching how AWS models them.
    fn iam_action_for(&self, request: &AwsRequest) -> Option<fakecloud_core::auth::IamAction> {
        // REST-JSON services don't have `request.action` populated at
        // dispatch time — it's derived from method+path inside
        // `handle()`. Reuse the same resolver so the two can never
        // drift.
        let (action_str, resource_name) = Self::resolve_action(request)?;
        let action: &'static str = match action_str {
            "CreateFunction" => "CreateFunction",
            "ListFunctions" => "ListFunctions",
            "GetFunction" => "GetFunction",
            "DeleteFunction" => "DeleteFunction",
            "Invoke" => "InvokeFunction",
            "PublishVersion" => "PublishVersion",
            "AddPermission" => "AddPermission",
            "RemovePermission" => "RemovePermission",
            "GetPolicy" => "GetPolicy",
            "CreateEventSourceMapping" => "CreateEventSourceMapping",
            "ListEventSourceMappings" => "ListEventSourceMappings",
            "GetEventSourceMapping" => "GetEventSourceMapping",
            "DeleteEventSourceMapping" => "DeleteEventSourceMapping",
            _ => return None,
        };
        let state = self.state.read();
        let resource = match action {
            "GetFunction" | "DeleteFunction" | "InvokeFunction" | "PublishVersion"
            | "AddPermission" | "RemovePermission" | "GetPolicy" => {
                let name = resource_name.unwrap_or_default();
                if name.is_empty() {
                    "*".to_string()
                } else {
                    format!(
                        "arn:aws:lambda:{}:{}:function:{}",
                        state.region, state.account_id, name
                    )
                }
            }
            "CreateFunction" => {
                // Best-effort: parse the FunctionName from the body so
                // CreateFunction can be resource-scoped against the
                // to-be-created ARN. Falls back to `*` when the body
                // isn't JSON yet (e.g. soft-mode observability).
                serde_json::from_slice::<Value>(&request.body)
                    .ok()
                    .and_then(|v| {
                        v.get("FunctionName").and_then(|f| f.as_str()).map(|n| {
                            format!(
                                "arn:aws:lambda:{}:{}:function:{}",
                                state.region, state.account_id, n
                            )
                        })
                    })
                    .unwrap_or_else(|| "*".to_string())
            }
            _ => "*".to_string(),
        };
        Some(fakecloud_core::auth::IamAction {
            service: "lambda",
            action,
            resource,
        })
    }

    fn iam_condition_keys_for(
        &self,
        request: &AwsRequest,
        action: &fakecloud_core::auth::IamAction,
    ) -> std::collections::BTreeMap<String, Vec<String>> {
        let mut out = std::collections::BTreeMap::new();
        if action.action == "AddPermission" {
            if action.resource != "*" {
                out.insert(
                    "lambda:functionarn".to_string(),
                    vec![action.resource.clone()],
                );
            }
            if let Ok(body) = serde_json::from_slice::<Value>(&request.body) {
                if let Some(principal) = body.get("Principal").and_then(|p| p.as_str()) {
                    out.insert("lambda:principal".to_string(), vec![principal.to_string()]);
                }
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::LambdaState;
    use bytes::Bytes;
    use http::{HeaderMap, Method};
    use parking_lot::RwLock;
    use std::collections::HashMap;
    use std::sync::Arc;

    fn make_state() -> SharedLambdaState {
        Arc::new(RwLock::new(LambdaState::new("123456789012", "us-east-1")))
    }

    fn make_request(method: Method, path: &str, body: &str) -> AwsRequest {
        let path_segments: Vec<String> = path
            .split('/')
            .filter(|s| !s.is_empty())
            .map(|s| s.to_string())
            .collect();
        AwsRequest {
            service: "lambda".to_string(),
            action: String::new(),
            region: "us-east-1".to_string(),
            account_id: "123456789012".to_string(),
            request_id: "test-request-id".to_string(),
            headers: HeaderMap::new(),
            query_params: HashMap::new(),
            body: Bytes::from(body.to_string()),
            path_segments,
            raw_path: path.to_string(),
            raw_query: String::new(),
            method,
            is_query_protocol: false,
            access_key_id: None,
            principal: None,
        }
    }

    #[test]
    fn iam_condition_keys_for_add_permission_populates_arn_and_principal() {
        let svc = LambdaService::new(make_state());
        let body = json!({
            "StatementId": "stmt",
            "Action": "lambda:InvokeFunction",
            "Principal": "s3.amazonaws.com",
        })
        .to_string();
        let req = make_request(Method::POST, "/2015-03-31/functions/my-func/policy", &body);
        let action = fakecloud_core::auth::IamAction {
            service: "lambda",
            action: "AddPermission",
            resource: "arn:aws:lambda:us-east-1:123456789012:function:my-func".to_string(),
        };
        let keys = svc.iam_condition_keys_for(&req, &action);
        assert_eq!(
            keys.get("lambda:functionarn"),
            Some(&vec![
                "arn:aws:lambda:us-east-1:123456789012:function:my-func".to_string()
            ])
        );
        assert_eq!(
            keys.get("lambda:principal"),
            Some(&vec!["s3.amazonaws.com".to_string()])
        );
    }

    #[test]
    fn iam_condition_keys_for_add_permission_omits_missing_principal() {
        let svc = LambdaService::new(make_state());
        let body = json!({"StatementId": "stmt", "Action": "lambda:InvokeFunction"}).to_string();
        let req = make_request(Method::POST, "/2015-03-31/functions/my-func/policy", &body);
        let action = fakecloud_core::auth::IamAction {
            service: "lambda",
            action: "AddPermission",
            resource: "arn:aws:lambda:us-east-1:123456789012:function:my-func".to_string(),
        };
        let keys = svc.iam_condition_keys_for(&req, &action);
        assert!(!keys.contains_key("lambda:principal"));
        assert!(keys.contains_key("lambda:functionarn"));
    }

    #[test]
    fn iam_condition_keys_for_non_add_permission_is_empty() {
        let svc = LambdaService::new(make_state());
        let req = make_request(Method::GET, "/2015-03-31/functions/my-func", "");
        let action = fakecloud_core::auth::IamAction {
            service: "lambda",
            action: "GetFunction",
            resource: "arn:aws:lambda:us-east-1:123456789012:function:my-func".to_string(),
        };
        assert!(svc.iam_condition_keys_for(&req, &action).is_empty());
    }

    #[tokio::test]
    async fn test_create_and_get_function() {
        let state = make_state();
        let svc = LambdaService::new(state);

        let create_body = json!({
            "FunctionName": "my-func",
            "Runtime": "python3.12",
            "Role": "arn:aws:iam::123456789012:role/test-role",
            "Handler": "index.handler",
            "Code": { "ZipFile": "UEsFBgAAAAAAAAAAAAAAAAAAAAA=" }
        });

        let req = make_request(
            Method::POST,
            "/2015-03-31/functions",
            &create_body.to_string(),
        );
        let resp = svc.handle(req).await.unwrap();
        assert_eq!(resp.status, StatusCode::CREATED);

        let body: Value = serde_json::from_slice(resp.body.expect_bytes()).unwrap();
        assert_eq!(body["FunctionName"], "my-func");
        assert_eq!(body["Runtime"], "python3.12");

        // Get
        let req = make_request(Method::GET, "/2015-03-31/functions/my-func", "");
        let resp = svc.handle(req).await.unwrap();
        assert_eq!(resp.status, StatusCode::OK);
        let body: Value = serde_json::from_slice(resp.body.expect_bytes()).unwrap();
        assert_eq!(body["Configuration"]["FunctionName"], "my-func");
    }

    #[tokio::test]
    async fn test_delete_function() {
        let state = make_state();
        let svc = LambdaService::new(state);

        let create_body = json!({
            "FunctionName": "to-delete",
            "Runtime": "nodejs20.x",
            "Role": "arn:aws:iam::123456789012:role/test",
            "Handler": "index.handler",
            "Code": {}
        });

        let req = make_request(
            Method::POST,
            "/2015-03-31/functions",
            &create_body.to_string(),
        );
        svc.handle(req).await.unwrap();

        let req = make_request(Method::DELETE, "/2015-03-31/functions/to-delete", "");
        let resp = svc.handle(req).await.unwrap();
        assert_eq!(resp.status, StatusCode::NO_CONTENT);

        // Verify deleted
        let req = make_request(Method::GET, "/2015-03-31/functions/to-delete", "");
        let resp = svc.handle(req).await;
        assert!(resp.is_err());
    }

    #[tokio::test]
    async fn test_invoke_without_runtime_returns_error() {
        let state = make_state();
        let svc = LambdaService::new(state);

        let create_body = json!({
            "FunctionName": "invoke-me",
            "Runtime": "python3.12",
            "Role": "arn:aws:iam::123456789012:role/test",
            "Handler": "index.handler",
            "Code": {}
        });

        let req = make_request(
            Method::POST,
            "/2015-03-31/functions",
            &create_body.to_string(),
        );
        svc.handle(req).await.unwrap();

        let req = make_request(
            Method::POST,
            "/2015-03-31/functions/invoke-me/invocations",
            r#"{"key": "value"}"#,
        );
        let resp = svc.handle(req).await;
        assert!(resp.is_err());
    }

    #[tokio::test]
    async fn test_invoke_nonexistent_function() {
        let state = make_state();
        let svc = LambdaService::new(state);

        let req = make_request(
            Method::POST,
            "/2015-03-31/functions/does-not-exist/invocations",
            "{}",
        );
        let resp = svc.handle(req).await;
        assert!(resp.is_err());
    }

    #[tokio::test]
    async fn test_list_functions() {
        let state = make_state();
        let svc = LambdaService::new(state);

        for name in &["func-a", "func-b"] {
            let create_body = json!({
                "FunctionName": name,
                "Runtime": "python3.12",
                "Role": "arn:aws:iam::123456789012:role/test",
                "Handler": "index.handler",
                "Code": {}
            });
            let req = make_request(
                Method::POST,
                "/2015-03-31/functions",
                &create_body.to_string(),
            );
            svc.handle(req).await.unwrap();
        }

        let req = make_request(Method::GET, "/2015-03-31/functions", "");
        let resp = svc.handle(req).await.unwrap();
        let body: Value = serde_json::from_slice(resp.body.expect_bytes()).unwrap();
        assert_eq!(body["Functions"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn test_event_source_mapping() {
        let state = make_state();
        let svc = LambdaService::new(state);

        // Create function first
        let create_body = json!({
            "FunctionName": "esm-func",
            "Runtime": "python3.12",
            "Role": "arn:aws:iam::123456789012:role/test",
            "Handler": "index.handler",
            "Code": {}
        });
        let req = make_request(
            Method::POST,
            "/2015-03-31/functions",
            &create_body.to_string(),
        );
        svc.handle(req).await.unwrap();

        // Create mapping
        let mapping_body = json!({
            "FunctionName": "esm-func",
            "EventSourceArn": "arn:aws:sqs:us-east-1:123456789012:my-queue",
            "BatchSize": 5
        });
        let req = make_request(
            Method::POST,
            "/2015-03-31/event-source-mappings",
            &mapping_body.to_string(),
        );
        let resp = svc.handle(req).await.unwrap();
        assert_eq!(resp.status, StatusCode::ACCEPTED);
        let body: Value = serde_json::from_slice(resp.body.expect_bytes()).unwrap();
        let uuid = body["UUID"].as_str().unwrap().to_string();

        // List mappings
        let req = make_request(Method::GET, "/2015-03-31/event-source-mappings", "");
        let resp = svc.handle(req).await.unwrap();
        let body: Value = serde_json::from_slice(resp.body.expect_bytes()).unwrap();
        assert_eq!(body["EventSourceMappings"].as_array().unwrap().len(), 1);

        // Delete mapping
        let req = make_request(
            Method::DELETE,
            &format!("/2015-03-31/event-source-mappings/{uuid}"),
            "",
        );
        let resp = svc.handle(req).await.unwrap();
        assert_eq!(resp.status, StatusCode::ACCEPTED);
    }

    async fn seed_function(svc: &LambdaService, name: &str) {
        let body = json!({
            "FunctionName": name,
            "Runtime": "python3.12",
            "Role": "arn:aws:iam::123456789012:role/r",
            "Handler": "index.handler",
            "Code": {}
        });
        let req = make_request(Method::POST, "/2015-03-31/functions", &body.to_string());
        svc.handle(req).await.unwrap();
    }

    #[tokio::test]
    async fn add_permission_builds_canonical_statement() {
        let svc = LambdaService::new(make_state());
        seed_function(&svc, "f").await;

        let body = json!({
            "StatementId": "s3-invoke",
            "Action": "InvokeFunction",
            "Principal": "s3.amazonaws.com",
            "SourceArn": "arn:aws:s3:::my-bucket",
            "SourceAccount": "123456789012",
        });
        let req = make_request(
            Method::POST,
            "/2015-03-31/functions/f/policy",
            &body.to_string(),
        );
        let resp = svc.handle(req).await.unwrap();
        assert_eq!(resp.status, StatusCode::CREATED);

        let out: Value = serde_json::from_slice(resp.body.expect_bytes()).unwrap();
        let statement: Value = serde_json::from_str(out["Statement"].as_str().unwrap()).unwrap();
        assert_eq!(statement["Sid"], "s3-invoke");
        assert_eq!(statement["Effect"], "Allow");
        assert_eq!(statement["Principal"]["Service"], "s3.amazonaws.com");
        assert_eq!(statement["Action"], "lambda:InvokeFunction");
        assert_eq!(
            statement["Resource"],
            "arn:aws:lambda:us-east-1:123456789012:function:f"
        );
        assert_eq!(
            statement["Condition"]["ArnLike"]["aws:SourceArn"],
            "arn:aws:s3:::my-bucket"
        );
        assert_eq!(
            statement["Condition"]["StringEquals"]["aws:SourceAccount"],
            "123456789012"
        );
    }

    #[tokio::test]
    async fn add_permission_aws_principal_emits_aws_key() {
        let svc = LambdaService::new(make_state());
        seed_function(&svc, "f").await;

        let body = json!({
            "StatementId": "user-invoke",
            "Action": "InvokeFunction",
            "Principal": "arn:aws:iam::123456789012:user/alice",
        });
        let req = make_request(
            Method::POST,
            "/2015-03-31/functions/f/policy",
            &body.to_string(),
        );
        svc.handle(req).await.unwrap();

        // Fetch via GetPolicy and inspect the stored doc.
        let req = make_request(Method::GET, "/2015-03-31/functions/f/policy", "");
        let resp = svc.handle(req).await.unwrap();
        let body: Value = serde_json::from_slice(resp.body.expect_bytes()).unwrap();
        let doc: Value = serde_json::from_str(body["Policy"].as_str().unwrap()).unwrap();
        let statements = doc["Statement"].as_array().unwrap();
        assert_eq!(statements.len(), 1);
        assert_eq!(
            statements[0]["Principal"]["AWS"],
            "arn:aws:iam::123456789012:user/alice"
        );
        assert!(statements[0].get("Condition").is_none());
    }

    #[tokio::test]
    async fn add_permission_rejects_duplicate_statement_id() {
        let svc = LambdaService::new(make_state());
        seed_function(&svc, "f").await;

        let body = json!({
            "StatementId": "dup",
            "Action": "InvokeFunction",
            "Principal": "arn:aws:iam::123456789012:user/a",
        });
        let req = make_request(
            Method::POST,
            "/2015-03-31/functions/f/policy",
            &body.to_string(),
        );
        svc.handle(req).await.unwrap();

        let req = make_request(
            Method::POST,
            "/2015-03-31/functions/f/policy",
            &body.to_string(),
        );
        let err = match svc.handle(req).await {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert_eq!(err.status(), StatusCode::CONFLICT);
    }

    #[tokio::test]
    async fn get_policy_returns_404_when_no_policy_attached() {
        let svc = LambdaService::new(make_state());
        seed_function(&svc, "f").await;

        let req = make_request(Method::GET, "/2015-03-31/functions/f/policy", "");
        let err = match svc.handle(req).await {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn remove_permission_strips_matching_sid_and_leaves_empty_doc() {
        let svc = LambdaService::new(make_state());
        seed_function(&svc, "f").await;

        for sid in ["a", "b"] {
            let body = json!({
                "StatementId": sid,
                "Action": "InvokeFunction",
                "Principal": "arn:aws:iam::123456789012:user/u",
            });
            let req = make_request(
                Method::POST,
                "/2015-03-31/functions/f/policy",
                &body.to_string(),
            );
            svc.handle(req).await.unwrap();
        }

        // Remove "a"
        let req = make_request(Method::DELETE, "/2015-03-31/functions/f/policy/a", "");
        let resp = svc.handle(req).await.unwrap();
        assert_eq!(resp.status, StatusCode::NO_CONTENT);

        // GetPolicy still returns the doc with just "b".
        let req = make_request(Method::GET, "/2015-03-31/functions/f/policy", "");
        let resp = svc.handle(req).await.unwrap();
        let body: Value = serde_json::from_slice(resp.body.expect_bytes()).unwrap();
        let doc: Value = serde_json::from_str(body["Policy"].as_str().unwrap()).unwrap();
        let stmts = doc["Statement"].as_array().unwrap();
        assert_eq!(stmts.len(), 1);
        assert_eq!(stmts[0]["Sid"], "b");

        // Remove the last one — doc stays (empty Statement array).
        let req = make_request(Method::DELETE, "/2015-03-31/functions/f/policy/b", "");
        svc.handle(req).await.unwrap();

        let req = make_request(Method::GET, "/2015-03-31/functions/f/policy", "");
        let resp = svc.handle(req).await.unwrap();
        let body: Value = serde_json::from_slice(resp.body.expect_bytes()).unwrap();
        let doc: Value = serde_json::from_str(body["Policy"].as_str().unwrap()).unwrap();
        assert_eq!(doc["Statement"].as_array().unwrap().len(), 0);
    }

    #[tokio::test]
    async fn remove_permission_unknown_sid_is_404() {
        let svc = LambdaService::new(make_state());
        seed_function(&svc, "f").await;

        let body = json!({
            "StatementId": "known",
            "Action": "InvokeFunction",
            "Principal": "arn:aws:iam::123456789012:user/u",
        });
        let req = make_request(
            Method::POST,
            "/2015-03-31/functions/f/policy",
            &body.to_string(),
        );
        svc.handle(req).await.unwrap();

        let req = make_request(Method::DELETE, "/2015-03-31/functions/f/policy/other", "");
        let err = match svc.handle(req).await {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn add_permission_on_missing_function_is_404() {
        let svc = LambdaService::new(make_state());
        let body = json!({
            "StatementId": "s",
            "Action": "InvokeFunction",
            "Principal": "arn:aws:iam::123456789012:user/u",
        });
        let req = make_request(
            Method::POST,
            "/2015-03-31/functions/missing/policy",
            &body.to_string(),
        );
        let err = match svc.handle(req).await {
            Err(e) => e,
            Ok(_) => panic!("expected error"),
        };
        assert_eq!(err.status(), StatusCode::NOT_FOUND);
    }

    #[test]
    fn iam_action_for_maps_invoke_to_function_arn() {
        let svc = LambdaService::new(make_state());
        let req = make_request(Method::POST, "/2015-03-31/functions/f/invocations", "");
        let action = svc.iam_action_for(&req).unwrap();
        assert_eq!(action.service, "lambda");
        assert_eq!(action.action, "InvokeFunction");
        assert_eq!(
            action.resource,
            "arn:aws:lambda:us-east-1:123456789012:function:f"
        );
    }

    #[test]
    fn iam_action_for_maps_list_to_star() {
        let svc = LambdaService::new(make_state());
        let req = make_request(Method::GET, "/2015-03-31/functions", "");
        let action = svc.iam_action_for(&req).unwrap();
        assert_eq!(action.action, "ListFunctions");
        assert_eq!(action.resource, "*");
    }

    #[test]
    fn iam_action_for_create_reads_function_name_from_body() {
        let svc = LambdaService::new(make_state());
        let body = json!({
            "FunctionName": "newfn",
            "Runtime": "python3.12",
            "Role": "arn:aws:iam::123456789012:role/r",
            "Handler": "index.handler",
            "Code": {}
        });
        let req = make_request(Method::POST, "/2015-03-31/functions", &body.to_string());
        let action = svc.iam_action_for(&req).unwrap();
        assert_eq!(action.action, "CreateFunction");
        assert_eq!(
            action.resource,
            "arn:aws:lambda:us-east-1:123456789012:function:newfn"
        );
    }
}
