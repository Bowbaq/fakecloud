use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::sync::Arc;

pub type SharedApiGatewayV2State =
    Arc<parking_lot::RwLock<fakecloud_core::multi_account::MultiAccountState<ApiGatewayV2State>>>;

impl fakecloud_core::multi_account::AccountState for ApiGatewayV2State {
    fn new_for_account(account_id: &str, region: &str, _endpoint: &str) -> Self {
        Self::new(account_id, region)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiGatewayV2State {
    pub account_id: String,
    pub region: String,
    #[serde(default)]
    pub apis: HashMap<String, HttpApi>,
    #[serde(default)]
    pub routes: HashMap<String, HashMap<String, Route>>,
    #[serde(default)]
    pub integrations: HashMap<String, HashMap<String, Integration>>,
    #[serde(default)]
    pub stages: HashMap<String, HashMap<String, Stage>>,
    #[serde(default)]
    pub deployments: HashMap<String, HashMap<String, Deployment>>,
    #[serde(default)]
    pub authorizers: HashMap<String, HashMap<String, Authorizer>>,
    /// Introspection-only buffer backing `/_fakecloud/apigatewayv2/requests`.
    /// Intentionally not persisted across restarts.
    #[serde(default, skip_serializing)]
    pub request_history: Vec<ApiRequest>,
}

pub const APIGATEWAYV2_SNAPSHOT_SCHEMA_VERSION: u32 = 2;

#[derive(Debug, Serialize, Deserialize)]
pub struct ApiGatewayV2Snapshot {
    pub schema_version: u32,
    #[serde(default)]
    pub accounts: Option<fakecloud_core::multi_account::MultiAccountState<ApiGatewayV2State>>,
    #[serde(default)]
    pub state: Option<ApiGatewayV2State>,
}

impl ApiGatewayV2State {
    pub fn new(account_id: &str, region: &str) -> Self {
        Self {
            account_id: account_id.to_string(),
            region: region.to_string(),
            apis: HashMap::new(),
            routes: HashMap::new(),
            integrations: HashMap::new(),
            stages: HashMap::new(),
            deployments: HashMap::new(),
            authorizers: HashMap::new(),
            request_history: Vec::new(),
        }
    }

    pub fn reset(&mut self) {
        self.apis.clear();
        self.routes.clear();
        self.integrations.clear();
        self.stages.clear();
        self.deployments.clear();
        self.authorizers.clear();
        self.request_history.clear();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpApi {
    pub api_id: String,
    pub name: String,
    pub protocol_type: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cors_configuration: Option<CorsConfiguration>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tags: Option<HashMap<String, String>>,
    pub created_date: DateTime<Utc>,
    pub api_endpoint: String,
    /// Real AWS API Gateway v2 always returns this on GetApi, defaulting
    /// to `$request.header.x-api-key` for HTTP APIs. The Terraform
    /// `aws_apigatewayv2_api` provider asserts on the value.
    pub api_key_selection_expression: String,
    /// Real AWS always returns this on GetApi, defaulting to
    /// `$request.method $request.path` for HTTP APIs. Same Terraform
    /// assertion pattern as the api_key_selection_expression above.
    pub route_selection_expression: String,
    /// Disabled by default; honoured at create time if the caller sets it.
    pub disable_execute_api_endpoint: bool,
    /// `ipv4` (default) or `dualstack`. Real AWS always returns this on
    /// GetApi and Terraform's provider asserts on it.
    pub ip_address_type: String,
}

impl HttpApi {
    pub fn new(
        api_id: String,
        name: String,
        description: Option<String>,
        tags: Option<HashMap<String, String>>,
        region: &str,
    ) -> Self {
        let created_date = Utc::now();
        let api_endpoint = format!("https://{}.execute-api.{}.amazonaws.com", api_id, region);

        Self {
            api_id,
            name,
            protocol_type: "HTTP".to_string(),
            description,
            cors_configuration: None,
            tags,
            created_date,
            api_endpoint,
            api_key_selection_expression: "$request.header.x-api-key".to_string(),
            route_selection_expression: "$request.method $request.path".to_string(),
            disable_execute_api_endpoint: false,
            ip_address_type: "ipv4".to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CorsConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_credentials: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_headers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_methods: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub allow_origins: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expose_headers: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_age: Option<i32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Route {
    pub route_id: String,
    pub route_key: String, // "GET /pets/{id}"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target: Option<String>, // "integrations/{integration-id}"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorization_type: Option<String>, // "NONE", "JWT", "CUSTOM"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorizer_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Integration {
    pub integration_id: String,
    pub integration_type: String, // "AWS_PROXY", "HTTP_PROXY", "MOCK"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub integration_uri: Option<String>, // Lambda ARN or HTTP endpoint
    #[serde(skip_serializing_if = "Option::is_none")]
    pub payload_format_version: Option<String>, // "2.0"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub timeout_in_millis: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stage {
    pub stage_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub deployment_id: Option<String>,
    pub auto_deploy: bool,
    pub created_date: DateTime<Utc>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated_date: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Deployment {
    pub deployment_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub created_date: DateTime<Utc>,
    pub auto_deployed: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Authorizer {
    pub authorizer_id: String,
    pub name: String,
    pub authorizer_type: String, // "JWT" or "REQUEST"
    #[serde(skip_serializing_if = "Option::is_none")]
    pub authorizer_uri: Option<String>, // Lambda ARN for REQUEST type
    #[serde(skip_serializing_if = "Option::is_none")]
    pub identity_source: Option<Vec<String>>, // e.g., ["$request.header.Authorization"]
    #[serde(skip_serializing_if = "Option::is_none")]
    pub jwt_configuration: Option<JwtConfiguration>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JwtConfiguration {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub audience: Option<Vec<String>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub issuer: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ApiRequest {
    pub request_id: String,
    pub api_id: String,
    pub stage: String,
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub query_params: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
    pub timestamp: DateTime<Utc>,
    pub status_code: u16,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_state_empty() {
        let state = ApiGatewayV2State::new("123456789012", "us-east-1");
        assert_eq!(state.account_id, "123456789012");
        assert_eq!(state.region, "us-east-1");
        assert!(state.apis.is_empty());
        assert!(state.routes.is_empty());
        assert!(state.request_history.is_empty());
    }

    #[test]
    fn new_http_api_defaults() {
        let api = HttpApi::new(
            "abc123".to_string(),
            "my-api".to_string(),
            Some("desc".to_string()),
            None,
            "us-east-1",
        );
        assert_eq!(api.api_id, "abc123");
        assert_eq!(api.name, "my-api");
        assert_eq!(api.protocol_type, "HTTP");
        assert_eq!(
            api.api_key_selection_expression,
            "$request.header.x-api-key"
        );
        assert_eq!(
            api.route_selection_expression,
            "$request.method $request.path"
        );
        assert!(api.api_endpoint.contains("abc123"));
        assert!(api.api_endpoint.contains("us-east-1"));
        assert!(!api.disable_execute_api_endpoint);
        assert_eq!(api.ip_address_type, "ipv4");
    }
}
