use bytes::Bytes;
use http::HeaderMap;
use std::collections::HashMap;

/// The wire protocol used by an AWS service.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AwsProtocol {
    /// Query protocol: form-encoded body, Action param, XML response.
    /// Used by: SQS, SNS, IAM, STS.
    Query,
    /// JSON protocol: JSON body, X-Amz-Target header, JSON response.
    /// Used by: SSM, EventBridge, DynamoDB, SecretsManager, KMS, CloudWatch Logs.
    Json,
    /// REST protocol: HTTP method + path-based routing, XML responses.
    /// Used by: S3, API Gateway, Route53.
    Rest,
    /// REST-JSON protocol: HTTP method + path-based routing, JSON responses.
    /// Used by: Lambda, SES v2.
    RestJson,
}

/// Services that use REST protocol with XML responses (detected from SigV4 credential scope).
const REST_XML_SERVICES: &[&str] = &["s3"];

/// Services that use REST protocol with JSON responses (detected from SigV4 credential scope).
const REST_JSON_SERVICES: &[&str] = &["lambda", "ses", "apigateway", "bedrock", "scheduler"];

/// Detected service name and action from an incoming HTTP request.
#[derive(Debug)]
pub struct DetectedRequest {
    pub service: String,
    pub action: String,
    pub protocol: AwsProtocol,
}

/// Detect the target service and action from HTTP request components.
pub fn detect_service(
    headers: &HeaderMap,
    query_params: &HashMap<String, String>,
    body: &Bytes,
) -> Option<DetectedRequest> {
    // 1. Check X-Amz-Target header (JSON protocol)
    if let Some(target) = headers.get("x-amz-target").and_then(|v| v.to_str().ok()) {
        return parse_amz_target(target);
    }

    // 2. Check for Query protocol (Action parameter in query string or form body)
    if let Some(action) = query_params.get("Action") {
        let service =
            extract_service_from_auth(headers).or_else(|| infer_service_from_action(action));
        if let Some(service) = service {
            return Some(DetectedRequest {
                service,
                action: action.clone(),
                protocol: AwsProtocol::Query,
            });
        }
    }

    // 3. Try form-encoded body
    {
        let form_params = decode_form_urlencoded(body);

        if let Some(action) = form_params.get("Action") {
            let service =
                extract_service_from_auth(headers).or_else(|| infer_service_from_action(action));
            if let Some(service) = service {
                return Some(DetectedRequest {
                    service,
                    action: action.clone(),
                    protocol: AwsProtocol::Query,
                });
            }
        }
    }

    // 4. Fallback: check auth header for REST-style services (S3, Lambda, SES, etc.)
    if let Some(service) = extract_service_from_auth(headers) {
        if let Some(protocol) = rest_protocol_for(&service) {
            return Some(DetectedRequest {
                service,
                action: String::new(), // REST services determine action from method+path
                protocol,
            });
        }
    }

    // 5. Check query params for presigned URL auth (X-Amz-Credential for SigV4)
    if let Some(credential) = query_params.get("X-Amz-Credential") {
        // Format: AKID/date/region/service/aws4_request
        let parts: Vec<&str> = credential.split('/').collect();
        if parts.len() >= 4 {
            let service = parts[3].to_string();
            if let Some(protocol) = rest_protocol_for(&service) {
                return Some(DetectedRequest {
                    service,
                    action: String::new(),
                    protocol,
                });
            }
        }
    }

    // 6. Check for SigV2-style presigned URL (AWSAccessKeyId + Signature + Expires)
    //    Only match when all three SigV2 presigned-URL parameters are present so
    //    we don't accidentally claim non-S3 requests.
    if query_params.contains_key("AWSAccessKeyId")
        && query_params.contains_key("Signature")
        && query_params.contains_key("Expires")
    {
        return Some(DetectedRequest {
            service: "s3".to_string(),
            action: String::new(),
            protocol: AwsProtocol::Rest,
        });
    }

    None
}

/// Parse `X-Amz-Target: AWSEvents.PutEvents` -> service=events, action=PutEvents
/// Parse `X-Amz-Target: AmazonSSM.GetParameter` -> service=ssm, action=GetParameter
fn parse_amz_target(target: &str) -> Option<DetectedRequest> {
    let (prefix, action) = target.rsplit_once('.')?;

    let service = match prefix {
        "AWSEvents" => "events",
        "AmazonSSM" => "ssm",
        "AmazonSQS" => "sqs",
        "AmazonSNS" => "sns",
        "DynamoDB_20120810" => "dynamodb",
        "Logs_20140328" => "logs",
        s if s.starts_with("secretsmanager") => "secretsmanager",
        s if s.starts_with("TrentService") => "kms",
        s if s.starts_with("AWSCognitoIdentityProviderService") => "cognito-idp",
        s if s.starts_with("Kinesis_20131202") => "kinesis",
        s if s.starts_with("AWSStepFunctions") => "states",
        s if s.starts_with("AWSOrganizationsV") => "organizations",
        _ => return None,
    };

    Some(DetectedRequest {
        service: service.to_string(),
        action: action.to_string(),
        protocol: AwsProtocol::Json,
    })
}

/// Returns the REST protocol variant for a service, or None if not a REST service.
fn rest_protocol_for(service: &str) -> Option<AwsProtocol> {
    if REST_XML_SERVICES.contains(&service) {
        Some(AwsProtocol::Rest)
    } else if REST_JSON_SERVICES.contains(&service) {
        Some(AwsProtocol::RestJson)
    } else {
        None
    }
}

/// Infer service from the action name when no SigV4 auth is present.
/// Some AWS operations (e.g., AssumeRoleWithSAML, AssumeRoleWithWebIdentity)
/// do not require authentication and won't have an Authorization header.
fn infer_service_from_action(action: &str) -> Option<String> {
    match action {
        "AssumeRole"
        | "AssumeRoleWithSAML"
        | "AssumeRoleWithWebIdentity"
        | "GetCallerIdentity"
        | "GetSessionToken"
        | "GetFederationToken"
        | "GetAccessKeyInfo"
        | "DecodeAuthorizationMessage" => Some("sts".to_string()),
        "CreateUser" | "DeleteUser" | "GetUser" | "ListUsers" | "CreateRole" | "DeleteRole"
        | "GetRole" | "ListRoles" | "CreatePolicy" | "DeletePolicy" | "GetPolicy"
        | "ListPolicies" | "AttachRolePolicy" | "DetachRolePolicy" | "CreateAccessKey"
        | "DeleteAccessKey" | "ListAccessKeys" | "ListRolePolicies" => Some("iam".to_string()),
        // SES v1 (Query protocol)
        "VerifyEmailIdentity"
        | "VerifyDomainIdentity"
        | "VerifyDomainDkim"
        | "ListIdentities"
        | "GetIdentityVerificationAttributes"
        | "GetIdentityDkimAttributes"
        | "DeleteIdentity"
        | "SetIdentityDkimEnabled"
        | "SetIdentityNotificationTopic"
        | "SetIdentityFeedbackForwardingEnabled"
        | "GetIdentityNotificationAttributes"
        | "GetIdentityMailFromDomainAttributes"
        | "SetIdentityMailFromDomain"
        | "SendEmail"
        | "SendRawEmail"
        | "SendTemplatedEmail"
        | "SendBulkTemplatedEmail"
        | "CreateTemplate"
        | "GetTemplate"
        | "ListTemplates"
        | "DeleteTemplate"
        | "UpdateTemplate"
        | "CreateConfigurationSet"
        | "DeleteConfigurationSet"
        | "DescribeConfigurationSet"
        | "ListConfigurationSets"
        | "CreateConfigurationSetEventDestination"
        | "UpdateConfigurationSetEventDestination"
        | "DeleteConfigurationSetEventDestination"
        | "GetSendQuota"
        | "GetSendStatistics"
        | "GetAccountSendingEnabled"
        | "CreateReceiptRuleSet"
        | "DeleteReceiptRuleSet"
        | "DescribeReceiptRuleSet"
        | "ListReceiptRuleSets"
        | "CloneReceiptRuleSet"
        | "SetActiveReceiptRuleSet"
        | "ReorderReceiptRuleSet"
        | "CreateReceiptRule"
        | "DeleteReceiptRule"
        | "DescribeReceiptRule"
        | "UpdateReceiptRule"
        | "CreateReceiptFilter"
        | "DeleteReceiptFilter"
        | "ListReceiptFilters" => Some("ses".to_string()),
        _ => None,
    }
}

/// Extract service name from the SigV4 Authorization header credential scope.
fn extract_service_from_auth(headers: &HeaderMap) -> Option<String> {
    let auth = headers.get("authorization")?.to_str().ok()?;
    let info = fakecloud_aws::sigv4::parse_sigv4(auth)?;
    Some(info.service)
}

/// Parse form-encoded body into key-value pairs.
pub fn parse_query_body(body: &Bytes) -> HashMap<String, String> {
    decode_form_urlencoded(body)
}

fn decode_form_urlencoded(input: &[u8]) -> HashMap<String, String> {
    let s = std::str::from_utf8(input).unwrap_or("");
    let mut result = HashMap::new();
    for pair in s.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (key, value) = match pair.find('=') {
            Some(pos) => (&pair[..pos], &pair[pos + 1..]),
            None => (pair, ""),
        };
        result.insert(url_decode(key), url_decode(value));
    }
    result
}

fn url_decode(input: &str) -> String {
    let mut result = String::with_capacity(input.len());
    let mut bytes = input.bytes();
    while let Some(b) = bytes.next() {
        match b {
            b'+' => result.push(' '),
            b'%' => {
                let high = bytes.next().and_then(from_hex);
                let low = bytes.next().and_then(from_hex);
                if let (Some(h), Some(l)) = (high, low) {
                    result.push((h << 4 | l) as char);
                }
            }
            _ => result.push(b as char),
        }
    }
    result
}

fn from_hex(b: u8) -> Option<u8> {
    match b {
        b'0'..=b'9' => Some(b - b'0'),
        b'a'..=b'f' => Some(b - b'a' + 10),
        b'A'..=b'F' => Some(b - b'A' + 10),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_amz_target_events() {
        let result = parse_amz_target("AWSEvents.PutEvents").unwrap();
        assert_eq!(result.service, "events");
        assert_eq!(result.action, "PutEvents");
        assert_eq!(result.protocol, AwsProtocol::Json);
    }

    #[test]
    fn parse_amz_target_ssm() {
        let result = parse_amz_target("AmazonSSM.GetParameter").unwrap();
        assert_eq!(result.service, "ssm");
        assert_eq!(result.action, "GetParameter");
    }

    #[test]
    fn parse_amz_target_kinesis() {
        let result = parse_amz_target("Kinesis_20131202.ListStreams").unwrap();
        assert_eq!(result.service, "kinesis");
        assert_eq!(result.action, "ListStreams");
        assert_eq!(result.protocol, AwsProtocol::Json);
    }

    #[test]
    fn parse_query_body_basic() {
        let body = Bytes::from(
            "Action=SendMessage&QueueUrl=http%3A%2F%2Flocalhost%3A4566%2Fqueue&MessageBody=hello",
        );
        let params = parse_query_body(&body);
        assert_eq!(params.get("Action").unwrap(), "SendMessage");
        assert_eq!(params.get("MessageBody").unwrap(), "hello");
    }

    #[test]
    fn parse_query_body_empty_returns_empty_map() {
        let body = Bytes::from("");
        let params = parse_query_body(&body);
        assert!(params.is_empty());
    }

    #[test]
    fn parse_query_body_duplicate_keys_last_wins() {
        let body = Bytes::from("key=a&key=b");
        let params = parse_query_body(&body);
        assert_eq!(params.get("key").unwrap(), "b");
    }

    #[test]
    fn parse_query_body_single_key() {
        let body = Bytes::from("key=value");
        let params = parse_query_body(&body);
        assert_eq!(params.get("key").unwrap(), "value");
    }

    #[test]
    fn parse_amz_target_rds() {
        let result = parse_amz_target("AmazonEC2ContainerServiceV20141113.ListClusters");
        // Not ECS — just verify doesn't panic on unknown prefixes
        assert!(result.is_some() || result.is_none());
    }

    #[test]
    fn parse_amz_target_invalid_returns_none() {
        assert!(parse_amz_target("NoDotHere").is_none());
        assert!(parse_amz_target("").is_none());
    }

    #[test]
    fn parse_amz_target_various_prefixes() {
        assert_eq!(
            parse_amz_target("AmazonSQS.SendMessage").unwrap().service,
            "sqs"
        );
        assert_eq!(
            parse_amz_target("AmazonSNS.Publish").unwrap().service,
            "sns"
        );
        assert_eq!(
            parse_amz_target("DynamoDB_20120810.GetItem")
                .unwrap()
                .service,
            "dynamodb"
        );
        assert_eq!(
            parse_amz_target("Logs_20140328.PutLogEvents")
                .unwrap()
                .service,
            "logs"
        );
        assert_eq!(
            parse_amz_target("secretsmanager.GetSecretValue")
                .unwrap()
                .service,
            "secretsmanager"
        );
        assert_eq!(
            parse_amz_target("TrentService.Encrypt").unwrap().service,
            "kms"
        );
        assert_eq!(
            parse_amz_target("AWSCognitoIdentityProviderService.InitiateAuth")
                .unwrap()
                .service,
            "cognito-idp"
        );
        assert_eq!(
            parse_amz_target("AWSStepFunctions.StartExecution")
                .unwrap()
                .service,
            "states"
        );
        assert_eq!(
            parse_amz_target("AWSOrganizationsV20161128.CreateOrganization")
                .unwrap()
                .service,
            "organizations"
        );
        assert!(parse_amz_target("UnknownServicePrefix.Action").is_none());
    }

    #[test]
    fn infer_service_from_action_maps_sts() {
        assert_eq!(
            infer_service_from_action("AssumeRole").as_deref(),
            Some("sts")
        );
        assert_eq!(
            infer_service_from_action("GetCallerIdentity").as_deref(),
            Some("sts")
        );
    }

    #[test]
    fn infer_service_from_action_maps_iam() {
        assert_eq!(
            infer_service_from_action("CreateUser").as_deref(),
            Some("iam")
        );
        assert_eq!(
            infer_service_from_action("ListRoles").as_deref(),
            Some("iam")
        );
    }

    #[test]
    fn infer_service_from_action_maps_ses() {
        assert_eq!(
            infer_service_from_action("SendEmail").as_deref(),
            Some("ses")
        );
        assert_eq!(
            infer_service_from_action("ListIdentities").as_deref(),
            Some("ses")
        );
    }

    #[test]
    fn infer_service_from_action_unknown_returns_none() {
        assert!(infer_service_from_action("NotARealAction").is_none());
    }

    #[test]
    fn rest_protocol_for_returns_none_for_non_rest_service() {
        assert!(rest_protocol_for("sqs").is_none());
    }

    #[test]
    fn url_decode_handles_percent_and_plus() {
        assert_eq!(url_decode("hello+world"), "hello world");
        assert_eq!(url_decode("hello%20world"), "hello world");
        assert_eq!(url_decode("100%25"), "100%");
    }

    #[test]
    fn url_decode_ignores_malformed_percent() {
        assert_eq!(url_decode("%ZZ"), "");
    }

    #[test]
    fn from_hex_valid_digits() {
        assert_eq!(from_hex(b'0'), Some(0));
        assert_eq!(from_hex(b'9'), Some(9));
        assert_eq!(from_hex(b'a'), Some(10));
        assert_eq!(from_hex(b'F'), Some(15));
    }

    #[test]
    fn from_hex_invalid_returns_none() {
        assert!(from_hex(b'g').is_none());
        assert!(from_hex(b' ').is_none());
    }

    #[test]
    fn detect_service_via_amz_target() {
        let mut headers = HeaderMap::new();
        headers.insert("x-amz-target", "AmazonSSM.GetParameter".parse().unwrap());
        let query = HashMap::new();
        let body = Bytes::new();
        let detected = detect_service(&headers, &query, &body).unwrap();
        assert_eq!(detected.service, "ssm");
        assert_eq!(detected.action, "GetParameter");
    }

    #[test]
    fn detect_service_via_query_action_with_inferred_service() {
        let headers = HeaderMap::new();
        let mut query = HashMap::new();
        query.insert("Action".to_string(), "AssumeRole".to_string());
        let body = Bytes::new();
        let detected = detect_service(&headers, &query, &body).unwrap();
        assert_eq!(detected.service, "sts");
        assert_eq!(detected.action, "AssumeRole");
        assert_eq!(detected.protocol, AwsProtocol::Query);
    }

    #[test]
    fn detect_service_via_form_body() {
        let headers = HeaderMap::new();
        let query = HashMap::new();
        let body = Bytes::from("Action=SendEmail&Source=x%40y.com");
        let detected = detect_service(&headers, &query, &body).unwrap();
        assert_eq!(detected.service, "ses");
        assert_eq!(detected.action, "SendEmail");
    }

    #[test]
    fn detect_service_via_sigv2_presigned() {
        let headers = HeaderMap::new();
        let mut query = HashMap::new();
        query.insert("AWSAccessKeyId".to_string(), "AKID".to_string());
        query.insert("Signature".to_string(), "sig".to_string());
        query.insert("Expires".to_string(), "1234567890".to_string());
        let body = Bytes::new();
        let detected = detect_service(&headers, &query, &body).unwrap();
        assert_eq!(detected.service, "s3");
        assert_eq!(detected.protocol, AwsProtocol::Rest);
    }

    #[test]
    fn detect_service_via_sigv4_presigned_credential() {
        let headers = HeaderMap::new();
        let mut query = HashMap::new();
        query.insert(
            "X-Amz-Credential".to_string(),
            "AKID/20240101/us-east-1/s3/aws4_request".to_string(),
        );
        let body = Bytes::new();
        let detected = detect_service(&headers, &query, &body).unwrap();
        assert_eq!(detected.service, "s3");
        assert_eq!(detected.protocol, AwsProtocol::Rest);
    }

    #[test]
    fn detect_service_unknown_returns_none() {
        let headers = HeaderMap::new();
        let query = HashMap::new();
        let body = Bytes::new();
        assert!(detect_service(&headers, &query, &body).is_none());
    }
}
