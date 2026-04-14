use axum::body::Body;
use axum::extract::{Extension, Query};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use std::collections::HashMap;
use std::sync::Arc;

use crate::auth::{is_root_bypass, CredentialResolver, IamMode};
use crate::protocol::{self, AwsProtocol};
use crate::registry::ServiceRegistry;
use crate::service::{AwsRequest, ResponseBody};

/// The main dispatch handler. All HTTP requests come through here.
pub async fn dispatch(
    Extension(registry): Extension<Arc<ServiceRegistry>>,
    Extension(config): Extension<Arc<DispatchConfig>>,
    Query(query_params): Query<HashMap<String, String>>,
    request: Request<Body>,
) -> Response<Body> {
    let request_id = uuid::Uuid::new_v4().to_string();

    let (parts, body) = request.into_parts();
    // TODO: plumb streaming request bodies end-to-end to remove the cap.
    // 128 MiB comfortably covers every legitimate single-PutObject (AWS
    // recommends multipart above ~100 MiB) and each multipart part is
    // dispatched through here separately, so a 20 GiB upload stays under this
    // limit per-request.
    const MAX_BODY_BYTES: usize = 128 * 1024 * 1024;
    let body_bytes = match axum::body::to_bytes(body, MAX_BODY_BYTES).await {
        Ok(b) => b,
        Err(_) => {
            return build_error_response(
                StatusCode::PAYLOAD_TOO_LARGE,
                "RequestEntityTooLarge",
                "Request body too large",
                &request_id,
                AwsProtocol::Query,
            );
        }
    };

    // Detect service and action
    let detected = match protocol::detect_service(&parts.headers, &query_params, &body_bytes) {
        Some(d) => d,
        None => {
            // OPTIONS requests (CORS preflight) don't carry Authorization headers.
            // Route them to S3 since S3 is the only REST service that handles CORS.
            // Note: API Gateway CORS preflight is not fully supported in this emulator
            // because we can't distinguish between S3 and API Gateway OPTIONS requests
            // without additional context (in real AWS, they have different domains).
            if parts.method == http::Method::OPTIONS {
                protocol::DetectedRequest {
                    service: "s3".to_string(),
                    action: String::new(),
                    protocol: AwsProtocol::Rest,
                }
            } else if !parts.uri.path().starts_with("/_") {
                // Requests without AWS auth that don't match any service might be
                // API Gateway execute API calls (plain HTTP without signatures).
                // Route them to apigateway service which will validate if a matching
                // API/stage exists. Skip special FakeCloud endpoints (/_*).
                protocol::DetectedRequest {
                    service: "apigateway".to_string(),
                    action: String::new(),
                    protocol: AwsProtocol::RestJson,
                }
            } else {
                return build_error_response(
                    StatusCode::BAD_REQUEST,
                    "MissingAction",
                    "Could not determine target service or action from request",
                    &request_id,
                    AwsProtocol::Query,
                );
            }
        }
    };

    // Look up service
    let service = match registry.get(&detected.service) {
        Some(s) => s,
        None => {
            return build_error_response(
                detected.protocol.error_status(),
                "UnknownService",
                &format!("Service '{}' is not available", detected.service),
                &request_id,
                detected.protocol,
            );
        }
    };

    // Extract region and access key from auth header (or presigned query).
    let auth_header = parts
        .headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    let header_info = fakecloud_aws::sigv4::parse_sigv4(auth_header);
    let presigned_info = if header_info.is_none() {
        // Presigned URL: credentials live in the query string.
        fakecloud_aws::sigv4::parse_sigv4_presigned(&query_params).map(|p| p.as_info())
    } else {
        None
    };
    let sigv4_info = header_info.or(presigned_info);
    let access_key_id = sigv4_info.as_ref().map(|info| info.access_key.clone());
    let region = sigv4_info
        .map(|info| info.region)
        .or_else(|| extract_region_from_user_agent(&parts.headers))
        .unwrap_or_else(|| config.region.clone());

    // Opt-in SigV4 cryptographic verification. Runs before the service
    // handler so a failing signature never reaches business logic. The
    // reserved `test*` root identity short-circuits verification to keep
    // local-dev workflows frictionless.
    if config.verify_sigv4 {
        let caller_akid = access_key_id.as_deref().unwrap_or("");
        if !is_root_bypass(caller_akid) {
            if let Some(resolver) = config.credential_resolver.as_ref() {
                let amz_date = parts
                    .headers
                    .get("x-amz-date")
                    .and_then(|v| v.to_str().ok());
                let parsed = fakecloud_aws::sigv4::parse_sigv4_header(auth_header, amz_date)
                    .or_else(|| fakecloud_aws::sigv4::parse_sigv4_presigned(&query_params));
                let parsed = match parsed {
                    Some(p) => p,
                    None => {
                        return build_error_response(
                            StatusCode::FORBIDDEN,
                            "IncompleteSignature",
                            "Request is missing or has a malformed AWS Signature",
                            &request_id,
                            detected.protocol,
                        );
                    }
                };
                let resolved = match resolver.resolve(&parsed.access_key) {
                    Some(r) => r,
                    None => {
                        return build_error_response(
                            StatusCode::FORBIDDEN,
                            "InvalidClientTokenId",
                            "The security token included in the request is invalid",
                            &request_id,
                            detected.protocol,
                        );
                    }
                };
                let headers_vec = fakecloud_aws::sigv4::headers_from_http(&parts.headers);
                let raw_query_for_verify = parts.uri.query().unwrap_or("").to_string();
                let verify_req = fakecloud_aws::sigv4::VerifyRequest {
                    method: parts.method.as_str(),
                    path: parts.uri.path(),
                    query: &raw_query_for_verify,
                    headers: &headers_vec,
                    body: &body_bytes,
                };
                match fakecloud_aws::sigv4::verify(
                    &parsed,
                    &verify_req,
                    &resolved.secret_access_key,
                    chrono::Utc::now(),
                ) {
                    Ok(()) => {}
                    Err(fakecloud_aws::sigv4::SigV4Error::RequestTimeTooSkewed { .. }) => {
                        return build_error_response(
                            StatusCode::FORBIDDEN,
                            "RequestTimeTooSkewed",
                            "The difference between the request time and the current time is too large",
                            &request_id,
                            detected.protocol,
                        );
                    }
                    Err(fakecloud_aws::sigv4::SigV4Error::InvalidDate(msg)) => {
                        return build_error_response(
                            StatusCode::FORBIDDEN,
                            "IncompleteSignature",
                            &format!("Invalid x-amz-date: {msg}"),
                            &request_id,
                            detected.protocol,
                        );
                    }
                    Err(fakecloud_aws::sigv4::SigV4Error::Malformed(msg)) => {
                        return build_error_response(
                            StatusCode::FORBIDDEN,
                            "IncompleteSignature",
                            &format!("Malformed SigV4 signature: {msg}"),
                            &request_id,
                            detected.protocol,
                        );
                    }
                    Err(fakecloud_aws::sigv4::SigV4Error::SignatureMismatch) => {
                        return build_error_response(
                            StatusCode::FORBIDDEN,
                            "SignatureDoesNotMatch",
                            "The request signature we calculated does not match the signature you provided",
                            &request_id,
                            detected.protocol,
                        );
                    }
                }
            }
        }
    }

    // Build path segments
    let path = parts.uri.path().to_string();
    let raw_query = parts.uri.query().unwrap_or("").to_string();
    let path_segments: Vec<String> = path
        .split('/')
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .collect();

    // For JSON protocol, validate that non-empty bodies are valid JSON
    if detected.protocol == AwsProtocol::Json
        && !body_bytes.is_empty()
        && serde_json::from_slice::<serde_json::Value>(&body_bytes).is_err()
    {
        return build_error_response(
            StatusCode::BAD_REQUEST,
            "SerializationException",
            "Start of structure or map found where not expected",
            &request_id,
            AwsProtocol::Json,
        );
    }

    // Merge query params with form body params for Query protocol
    let mut all_params = query_params;
    if detected.protocol == AwsProtocol::Query {
        let body_params = protocol::parse_query_body(&body_bytes);
        for (k, v) in body_params {
            all_params.entry(k).or_insert(v);
        }
    }

    let aws_request = AwsRequest {
        service: detected.service.clone(),
        action: detected.action.clone(),
        region,
        account_id: config.account_id.clone(),
        request_id: request_id.clone(),
        headers: parts.headers,
        query_params: all_params,
        body: body_bytes,
        path_segments,
        raw_path: path,
        raw_query,
        method: parts.method,
        is_query_protocol: detected.protocol == AwsProtocol::Query,
        access_key_id,
    };

    tracing::info!(
        service = %aws_request.service,
        action = %aws_request.action,
        request_id = %aws_request.request_id,
        "handling request"
    );

    match service.handle(aws_request).await {
        Ok(resp) => {
            let mut builder = Response::builder()
                .status(resp.status)
                .header("x-amzn-requestid", &request_id)
                .header("x-amz-request-id", &request_id);

            if !resp.content_type.is_empty() {
                builder = builder.header("content-type", &resp.content_type);
            }

            let has_content_length = resp
                .headers
                .iter()
                .any(|(k, _)| k.as_str().eq_ignore_ascii_case("content-length"));

            for (k, v) in &resp.headers {
                builder = builder.header(k, v);
            }

            match resp.body {
                ResponseBody::Bytes(b) => builder.body(Body::from(b)).unwrap(),
                ResponseBody::File { file, size } => {
                    let stream = tokio_util::io::ReaderStream::new(file);
                    let body = Body::from_stream(stream);
                    if !has_content_length {
                        builder = builder.header("content-length", size.to_string());
                    }
                    builder.body(body).unwrap()
                }
            }
        }
        Err(err) => {
            tracing::warn!(
                service = %detected.service,
                action = %detected.action,
                error = %err,
                "request failed"
            );
            let error_headers = err.response_headers().to_vec();
            let mut resp = build_error_response_with_fields(
                err.status(),
                err.code(),
                &err.message(),
                &request_id,
                detected.protocol,
                err.extra_fields(),
            );
            for (k, v) in &error_headers {
                if let (Ok(name), Ok(val)) = (
                    k.parse::<http::header::HeaderName>(),
                    v.parse::<http::header::HeaderValue>(),
                ) {
                    resp.headers_mut().insert(name, val);
                }
            }
            resp
        }
    }
}

/// Configuration passed to the dispatch handler.
#[derive(Clone)]
pub struct DispatchConfig {
    pub region: String,
    pub account_id: String,
    /// Whether to cryptographically verify SigV4 signatures on incoming
    /// requests. Wired through from `--verify-sigv4` /
    /// `FAKECLOUD_VERIFY_SIGV4`. Off by default.
    pub verify_sigv4: bool,
    /// IAM policy evaluation mode. Wired through from `--iam` /
    /// `FAKECLOUD_IAM`. Defaults to [`IamMode::Off`]. Actual evaluation is
    /// added in a later batch; today this field is plumbed but never
    /// consulted.
    pub iam_mode: IamMode,
    /// Resolves access key IDs to their secrets and owning principals.
    /// Required when `verify_sigv4` or `iam_mode != Off`. When `None`, both
    /// features gracefully degrade to off-by-default behavior.
    pub credential_resolver: Option<Arc<dyn CredentialResolver>>,
}

impl std::fmt::Debug for DispatchConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DispatchConfig")
            .field("region", &self.region)
            .field("account_id", &self.account_id)
            .field("verify_sigv4", &self.verify_sigv4)
            .field("iam_mode", &self.iam_mode)
            .field(
                "credential_resolver",
                &self
                    .credential_resolver
                    .as_ref()
                    .map(|_| "<CredentialResolver>"),
            )
            .finish()
    }
}

impl DispatchConfig {
    /// Minimal constructor for tests and call sites that don't care about the
    /// opt-in security features.
    pub fn new(region: impl Into<String>, account_id: impl Into<String>) -> Self {
        Self {
            region: region.into(),
            account_id: account_id.into(),
            verify_sigv4: false,
            iam_mode: IamMode::Off,
            credential_resolver: None,
        }
    }
}

/// Extract region from User-Agent header suffix `region/<region>`.
fn extract_region_from_user_agent(headers: &http::HeaderMap) -> Option<String> {
    let ua = headers.get("user-agent")?.to_str().ok()?;
    for part in ua.split_whitespace() {
        if let Some(region) = part.strip_prefix("region/") {
            if !region.is_empty() {
                return Some(region.to_string());
            }
        }
    }
    None
}

fn build_error_response(
    status: StatusCode,
    code: &str,
    message: &str,
    request_id: &str,
    protocol: AwsProtocol,
) -> Response<Body> {
    build_error_response_with_fields(status, code, message, request_id, protocol, &[])
}

fn build_error_response_with_fields(
    status: StatusCode,
    code: &str,
    message: &str,
    request_id: &str,
    protocol: AwsProtocol,
    extra_fields: &[(String, String)],
) -> Response<Body> {
    let (status, content_type, body) = match protocol {
        AwsProtocol::Query => {
            fakecloud_aws::error::xml_error_response(status, code, message, request_id)
        }
        AwsProtocol::Rest => fakecloud_aws::error::s3_xml_error_response_with_fields(
            status,
            code,
            message,
            request_id,
            extra_fields,
        ),
        AwsProtocol::Json | AwsProtocol::RestJson => {
            fakecloud_aws::error::json_error_response(status, code, message)
        }
    };

    Response::builder()
        .status(status)
        .header("content-type", content_type)
        .header("x-amzn-requestid", request_id)
        .header("x-amz-request-id", request_id)
        .body(Body::from(body))
        .unwrap()
}

trait ProtocolExt {
    fn error_status(&self) -> StatusCode;
}

impl ProtocolExt for AwsProtocol {
    fn error_status(&self) -> StatusCode {
        StatusCode::BAD_REQUEST
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dispatch_config_new_defaults_to_off() {
        let cfg = DispatchConfig::new("us-east-1", "123456789012");
        assert_eq!(cfg.region, "us-east-1");
        assert_eq!(cfg.account_id, "123456789012");
        assert!(!cfg.verify_sigv4);
        assert_eq!(cfg.iam_mode, IamMode::Off);
    }

    #[test]
    fn dispatch_config_carries_opt_in_flags() {
        let cfg = DispatchConfig {
            region: "eu-west-1".to_string(),
            account_id: "000000000000".to_string(),
            verify_sigv4: true,
            iam_mode: IamMode::Strict,
            credential_resolver: None,
        };
        assert!(cfg.verify_sigv4);
        assert!(cfg.iam_mode.is_strict());
    }
}
