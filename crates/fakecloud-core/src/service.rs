use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use std::collections::{BTreeMap, HashMap};

use crate::auth::Principal;

/// A parsed AWS request.
#[derive(Debug)]
pub struct AwsRequest {
    pub service: String,
    pub action: String,
    pub region: String,
    pub account_id: String,
    pub request_id: String,
    pub headers: HeaderMap,
    pub query_params: HashMap<String, String>,
    pub body: Bytes,
    pub path_segments: Vec<String>,
    /// The raw URI path, before splitting into segments.
    pub raw_path: String,
    /// The raw URI query string (everything after `?`), preserving repeated keys.
    pub raw_query: String,
    pub method: Method,
    /// Whether this request came via Query (form-encoded) or JSON protocol.
    pub is_query_protocol: bool,
    /// The access key ID from the SigV4 Authorization header, if present.
    pub access_key_id: Option<String>,
    /// The resolved caller identity. `None` when the credential is unknown
    /// or the caller used the reserved root-bypass credentials. Populated
    /// by dispatch via the configured [`crate::auth::CredentialResolver`]
    /// so service handlers can make identity-based decisions (e.g.
    /// `GetCallerIdentity`, IAM enforcement) without re-parsing the
    /// Authorization header.
    pub principal: Option<Principal>,
}

impl AwsRequest {
    /// Parse the request body as JSON, returning `Value::Null` on failure.
    pub fn json_body(&self) -> serde_json::Value {
        serde_json::from_slice(&self.body).unwrap_or(serde_json::Value::Null)
    }
}

/// A response body. Most handlers return [`ResponseBody::Bytes`] built from
/// an in-memory [`Bytes`] buffer; the [`File`](ResponseBody::File) variant
/// exists so large disk-backed objects can be streamed straight from the
/// filesystem to the HTTP body without being materialized into RAM. The file
/// handle is opened by the service handler while it still holds the
/// per-bucket read guard, so the reader sees a consistent inode even if a
/// concurrent PUT/DELETE renames or unlinks the path before dispatch streams
/// the body.
#[derive(Debug)]
pub enum ResponseBody {
    Bytes(Bytes),
    File { file: tokio::fs::File, size: u64 },
}

impl ResponseBody {
    pub fn len(&self) -> u64 {
        match self {
            ResponseBody::Bytes(b) => b.len() as u64,
            ResponseBody::File { size, .. } => *size,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Accessor that returns the bytes of a `Bytes` variant and panics for
    /// `File`. Used by tests and by callers that know the response was built
    /// from an in-memory buffer (JSON handlers, cross-service glue).
    pub fn expect_bytes(&self) -> &[u8] {
        match self {
            ResponseBody::Bytes(b) => b,
            ResponseBody::File { .. } => {
                panic!("expect_bytes called on ResponseBody::File")
            }
        }
    }
}

impl Default for ResponseBody {
    fn default() -> Self {
        ResponseBody::Bytes(Bytes::new())
    }
}

impl From<Bytes> for ResponseBody {
    fn from(b: Bytes) -> Self {
        ResponseBody::Bytes(b)
    }
}

impl From<Vec<u8>> for ResponseBody {
    fn from(v: Vec<u8>) -> Self {
        ResponseBody::Bytes(Bytes::from(v))
    }
}

impl From<&'static [u8]> for ResponseBody {
    fn from(s: &'static [u8]) -> Self {
        ResponseBody::Bytes(Bytes::from_static(s))
    }
}

impl From<String> for ResponseBody {
    fn from(s: String) -> Self {
        ResponseBody::Bytes(Bytes::from(s))
    }
}

impl From<&'static str> for ResponseBody {
    fn from(s: &'static str) -> Self {
        ResponseBody::Bytes(Bytes::from_static(s.as_bytes()))
    }
}

impl PartialEq<Bytes> for ResponseBody {
    fn eq(&self, other: &Bytes) -> bool {
        match self {
            ResponseBody::Bytes(b) => b == other,
            ResponseBody::File { .. } => false,
        }
    }
}

/// A response from a service handler.
pub struct AwsResponse {
    pub status: StatusCode,
    pub content_type: String,
    pub body: ResponseBody,
    pub headers: HeaderMap,
}

impl AwsResponse {
    pub fn xml(status: StatusCode, body: impl Into<Bytes>) -> Self {
        Self {
            status,
            content_type: "text/xml".to_string(),
            body: ResponseBody::Bytes(body.into()),
            headers: HeaderMap::new(),
        }
    }

    pub fn json(status: StatusCode, body: impl Into<Bytes>) -> Self {
        Self {
            status,
            content_type: "application/x-amz-json-1.1".to_string(),
            body: ResponseBody::Bytes(body.into()),
            headers: HeaderMap::new(),
        }
    }

    /// Convenience constructor for a 200 OK JSON response from a `serde_json::Value`.
    pub fn ok_json(value: serde_json::Value) -> Self {
        Self::json(StatusCode::OK, serde_json::to_vec(&value).unwrap())
    }
}

/// Error returned by service handlers.
#[derive(Debug, thiserror::Error)]
pub enum AwsServiceError {
    #[error("service not found: {service}")]
    ServiceNotFound { service: String },

    #[error("action {action} not implemented for service {service}")]
    ActionNotImplemented { service: String, action: String },

    #[error("{code}: {message}")]
    AwsError {
        status: StatusCode,
        code: String,
        message: String,
        /// Additional key-value pairs to include in the error XML (e.g., BucketName, Key, Condition).
        extra_fields: Vec<(String, String)>,
        /// Additional HTTP headers to include in the error response.
        headers: Vec<(String, String)>,
    },
}

impl AwsServiceError {
    pub fn action_not_implemented(service: &str, action: &str) -> Self {
        Self::ActionNotImplemented {
            service: service.to_string(),
            action: action.to_string(),
        }
    }

    pub fn aws_error(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
    ) -> Self {
        Self::AwsError {
            status,
            code: code.into(),
            message: message.into(),
            extra_fields: Vec::new(),
            headers: Vec::new(),
        }
    }

    pub fn aws_error_with_fields(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
        extra_fields: Vec<(String, String)>,
    ) -> Self {
        Self::AwsError {
            status,
            code: code.into(),
            message: message.into(),
            extra_fields,
            headers: Vec::new(),
        }
    }

    pub fn aws_error_with_headers(
        status: StatusCode,
        code: impl Into<String>,
        message: impl Into<String>,
        headers: Vec<(String, String)>,
    ) -> Self {
        Self::AwsError {
            status,
            code: code.into(),
            message: message.into(),
            extra_fields: Vec::new(),
            headers,
        }
    }

    pub fn extra_fields(&self) -> &[(String, String)] {
        match self {
            Self::AwsError { extra_fields, .. } => extra_fields,
            _ => &[],
        }
    }

    pub fn status(&self) -> StatusCode {
        match self {
            Self::ServiceNotFound { .. } => StatusCode::BAD_REQUEST,
            Self::ActionNotImplemented { .. } => StatusCode::NOT_IMPLEMENTED,
            Self::AwsError { status, .. } => *status,
        }
    }

    pub fn code(&self) -> &str {
        match self {
            Self::ServiceNotFound { .. } => "UnknownService",
            Self::ActionNotImplemented { .. } => "InvalidAction",
            Self::AwsError { code, .. } => code,
        }
    }

    pub fn message(&self) -> String {
        match self {
            Self::ServiceNotFound { service } => format!("service not found: {service}"),
            Self::ActionNotImplemented { service, action } => {
                format!("action {action} not implemented for service {service}")
            }
            Self::AwsError { message, .. } => message.clone(),
        }
    }

    pub fn response_headers(&self) -> &[(String, String)] {
        match self {
            Self::AwsError { headers, .. } => headers,
            _ => &[],
        }
    }
}

/// Trait that every AWS service implements.
#[async_trait]
pub trait AwsService: Send + Sync {
    /// The AWS service identifier (e.g., "sqs", "sns", "sts", "events", "ssm").
    fn service_name(&self) -> &str;

    /// Handle an incoming request.
    async fn handle(&self, request: AwsRequest) -> Result<AwsResponse, AwsServiceError>;

    /// List of actions this service supports (for introspection).
    fn supported_actions(&self) -> &[&str];

    /// Whether this service participates in opt-in IAM enforcement
    /// (`FAKECLOUD_IAM=soft|strict`).
    ///
    /// Defaults to `false`: unless a service has a full
    /// `iam_action_for` implementation covering every operation it
    /// supports plus resource-ARN extractors, it's silently skipped when
    /// IAM enforcement is on. The startup log enumerates which services
    /// are enforced and which are not so users always know the current
    /// enforcement surface.
    ///
    /// Phase 1 contract: a service that returns `true` here MUST also
    /// provide a fully populated [`AwsService::iam_action_for`]
    /// implementation covering every action it advertises. Returning
    /// `true` without the action mapping is a programming bug.
    fn iam_enforceable(&self) -> bool {
        false
    }

    /// Derive the IAM action + resource ARN for an incoming request.
    ///
    /// Only called when [`AwsService::iam_enforceable`] returns `true`
    /// and IAM enforcement is enabled. Services must map every action
    /// they implement; returning `None` for a covered action causes the
    /// evaluator to skip the request and flag it via the
    /// `fakecloud::iam::audit` tracing target so gaps are visible in
    /// soft mode.
    ///
    /// The `IamAction.resource` is built from `request.principal`'s
    /// account id (not global config) so multi-account isolation
    /// (#381) works once per-account state partitioning lands.
    fn iam_action_for(&self, _request: &AwsRequest) -> Option<crate::auth::IamAction> {
        None
    }

    /// Derive service-specific IAM condition keys for an incoming request.
    ///
    /// Called right after [`AwsService::iam_action_for`] when IAM
    /// enforcement is enabled. The returned map is merged into the
    /// [`crate::auth::ConditionContext::service_keys`] before the
    /// evaluator runs, so policies can reference keys like `s3:prefix`
    /// or `sns:Protocol` the same way they reference global keys.
    ///
    /// Keys MUST be in the full `"service:key"` form, lowercased
    /// (e.g. `"s3:prefix"`), matching the case-insensitive lookup in
    /// [`crate::auth::ConditionContext::lookup`]. Extractors should
    /// only emit keys they can populate with confidence; anything
    /// ambiguous or unimplemented should be skipped with a
    /// `tracing::debug!(target: "fakecloud::iam::audit", ...)` so
    /// condition evaluation safe-fails to "doesn't apply" rather than
    /// "matches".
    ///
    /// Default impl returns an empty map: services that haven't been
    /// plumbed yet behave exactly as before.
    fn iam_condition_keys_for(
        &self,
        _request: &AwsRequest,
        _action: &crate::auth::IamAction,
    ) -> BTreeMap<String, Vec<String>> {
        BTreeMap::new()
    }

    /// Return the tags on the resource identified by `resource_arn`.
    ///
    /// Called at dispatch time when IAM enforcement is enabled, right
    /// after [`AwsService::iam_action_for`]. The returned map populates
    /// `aws:ResourceTag/<key>` condition keys so policies can gate
    /// access based on the target resource's tags.
    ///
    /// Return `None` to signal that this service does not (yet) support
    /// resource-tag ABAC — dispatch will emit a debug audit log and
    /// skip `aws:ResourceTag/*` evaluation. Return `Some(empty map)`
    /// when the resource exists but has no tags.
    fn resource_tags_for(
        &self,
        _resource_arn: &str,
    ) -> Option<std::collections::HashMap<String, String>> {
        None
    }

    /// Extract tags being sent in the request (e.g. on CreateQueue,
    /// PutObject with `x-amz-tagging`, TagResource).
    ///
    /// The returned map populates `aws:RequestTag/<key>` and
    /// `aws:TagKeys` condition keys. Return `None` when the service
    /// does not (yet) support request-tag extraction — dispatch skips
    /// `aws:RequestTag/*` / `aws:TagKeys` evaluation with a debug log.
    /// Return `Some(empty map)` when the request legitimately carries
    /// no tags.
    fn request_tags_from(
        &self,
        _request: &AwsRequest,
        _action: &str,
    ) -> Option<std::collections::HashMap<String, String>> {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::auth::IamAction;
    use async_trait::async_trait;

    struct DefaultService;

    #[async_trait]
    impl AwsService for DefaultService {
        fn service_name(&self) -> &str {
            "default"
        }
        async fn handle(&self, _request: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
            unreachable!()
        }
        fn supported_actions(&self) -> &[&str] {
            &[]
        }
    }

    struct PopulatedService;

    #[async_trait]
    impl AwsService for PopulatedService {
        fn service_name(&self) -> &str {
            "populated"
        }
        async fn handle(&self, _request: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
            unreachable!()
        }
        fn supported_actions(&self) -> &[&str] {
            &[]
        }
        fn iam_condition_keys_for(
            &self,
            _request: &AwsRequest,
            _action: &IamAction,
        ) -> BTreeMap<String, Vec<String>> {
            let mut m = BTreeMap::new();
            m.insert("s3:prefix".to_string(), vec!["logs/".to_string()]);
            m
        }
    }

    fn sample_request() -> AwsRequest {
        AwsRequest {
            service: "default".into(),
            action: "Noop".into(),
            region: "us-east-1".into(),
            account_id: "123456789012".into(),
            request_id: "req-1".into(),
            headers: HeaderMap::new(),
            query_params: HashMap::new(),
            body: Bytes::new(),
            path_segments: vec![],
            raw_path: "/".into(),
            raw_query: String::new(),
            method: Method::GET,
            is_query_protocol: false,
            access_key_id: None,
            principal: None,
        }
    }

    fn sample_action() -> IamAction {
        IamAction {
            service: "s3",
            action: "ListBucket",
            resource: "arn:aws:s3:::my-bucket".to_string(),
        }
    }

    #[test]
    fn iam_condition_keys_for_default_is_empty() {
        let svc = DefaultService;
        let keys = svc.iam_condition_keys_for(&sample_request(), &sample_action());
        assert!(keys.is_empty());
    }

    #[test]
    fn iam_condition_keys_for_override_returns_map() {
        let svc = PopulatedService;
        let keys = svc.iam_condition_keys_for(&sample_request(), &sample_action());
        assert_eq!(keys.get("s3:prefix"), Some(&vec!["logs/".to_string()]));
    }
}
