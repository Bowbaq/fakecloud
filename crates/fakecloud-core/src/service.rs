use async_trait::async_trait;
use bytes::Bytes;
use http::{HeaderMap, Method, StatusCode};
use std::collections::HashMap;
use std::ops::Deref;
use std::path::PathBuf;

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
/// filesystem to the HTTP body without being materialized into RAM.
#[derive(Debug)]
pub enum ResponseBody {
    Bytes(Bytes),
    File { path: PathBuf, size: u64 },
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

/// Deref gives existing test code that does `&resp.body` a `&[u8]` view.
/// File-variant responses appear empty to this view — tests only inspect
/// in-memory bodies.
impl Deref for ResponseBody {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        match self {
            ResponseBody::Bytes(b) => b,
            ResponseBody::File { .. } => &[],
        }
    }
}

impl AsRef<[u8]> for ResponseBody {
    fn as_ref(&self) -> &[u8] {
        self
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
}
