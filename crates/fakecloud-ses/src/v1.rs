//! SES v1 Query protocol handlers for receipt rules, receipt filters,
//! and inbound email processing.

use chrono::Utc;
use http::StatusCode;
use std::collections::HashMap;

use fakecloud_core::service::{AwsRequest, AwsResponse, AwsServiceError};

use crate::state::{
    IpFilter, ReceiptAction, ReceiptFilter, ReceiptRule, ReceiptRuleSet, SharedSesState,
};

/// XML namespace for SES v1 responses.
const SES_NS: &str = "http://ses.amazonaws.com/doc/2010-12-01/";

/// Wrap a v1 action result in the standard SES Query protocol XML envelope.
fn xml_wrap(action: &str, inner: &str, request_id: &str) -> String {
    format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <{action}Response xmlns=\"{SES_NS}\">\
         <{action}Result>{inner}</{action}Result>\
         <ResponseMetadata><RequestId>{request_id}</RequestId></ResponseMetadata>\
         </{action}Response>"
    )
}

/// Response with only metadata (no result body).
fn xml_metadata_only(action: &str, request_id: &str) -> AwsResponse {
    let xml = format!(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\
         <{action}Response xmlns=\"{SES_NS}\">\
         <ResponseMetadata><RequestId>{request_id}</RequestId></ResponseMetadata>\
         </{action}Response>"
    );
    AwsResponse::xml(StatusCode::OK, xml)
}

/// Dispatch a v1 Query protocol action.
pub fn handle_v1_action(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    match req.action.as_str() {
        // Receipt Rule Sets
        "CreateReceiptRuleSet" => create_receipt_rule_set(state, req),
        "DeleteReceiptRuleSet" => delete_receipt_rule_set(state, req),
        "DescribeReceiptRuleSet" => describe_receipt_rule_set(state, req),
        "ListReceiptRuleSets" => list_receipt_rule_sets(state, req),
        "CloneReceiptRuleSet" => clone_receipt_rule_set(state, req),
        "SetActiveReceiptRuleSet" => set_active_receipt_rule_set(state, req),
        "ReorderReceiptRuleSet" => reorder_receipt_rule_set(state, req),
        // Receipt Rules
        "CreateReceiptRule" => create_receipt_rule(state, req),
        "DeleteReceiptRule" => delete_receipt_rule(state, req),
        "DescribeReceiptRule" => describe_receipt_rule(state, req),
        "UpdateReceiptRule" => update_receipt_rule(state, req),
        // Receipt Filters
        "CreateReceiptFilter" => create_receipt_filter(state, req),
        "DeleteReceiptFilter" => delete_receipt_filter(state, req),
        "ListReceiptFilters" => list_receipt_filters(state, req),
        _ => Err(AwsServiceError::action_not_implemented("ses", &req.action)),
    }
}

/// List of v1 actions supported.
pub const V1_ACTIONS: &[&str] = &[
    "CreateReceiptRuleSet",
    "DeleteReceiptRuleSet",
    "DescribeReceiptRuleSet",
    "ListReceiptRuleSets",
    "CloneReceiptRuleSet",
    "SetActiveReceiptRuleSet",
    "ReorderReceiptRuleSet",
    "CreateReceiptRule",
    "DeleteReceiptRule",
    "DescribeReceiptRule",
    "UpdateReceiptRule",
    "CreateReceiptFilter",
    "DeleteReceiptFilter",
    "ListReceiptFilters",
];

// ── Helpers ──

fn required_param<'a>(
    params: &'a HashMap<String, String>,
    key: &str,
) -> Result<&'a str, AwsServiceError> {
    params.get(key).map(|s| s.as_str()).ok_or_else(|| {
        AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "ValidationError",
            format!("Value for parameter {key} is required"),
        )
    })
}

/// Parse a receipt rule from form parameters (for Create/Update).
fn parse_receipt_rule(params: &HashMap<String, String>) -> Result<ReceiptRule, AwsServiceError> {
    let name = required_param(params, "Rule.Name")?.to_string();
    let enabled = params
        .get("Rule.Enabled")
        .map(|v| v == "true")
        .unwrap_or(false);
    let scan_enabled = params
        .get("Rule.ScanEnabled")
        .map(|v| v == "true")
        .unwrap_or(false);
    let tls_policy = params
        .get("Rule.TlsPolicy")
        .cloned()
        .unwrap_or_else(|| "Optional".to_string());

    // Parse recipients: Rule.Recipients.member.1, Rule.Recipients.member.2, ...
    let mut recipients = Vec::new();
    for i in 1.. {
        let key = format!("Rule.Recipients.member.{i}");
        match params.get(&key) {
            Some(v) => recipients.push(v.clone()),
            None => break,
        }
    }

    // Parse actions: Rule.Actions.member.1.*, Rule.Actions.member.2.*, ...
    let mut actions = Vec::new();
    for i in 1.. {
        let prefix = format!("Rule.Actions.member.{i}");
        // Detect which action type is present
        if let Some(action) = parse_action(params, &prefix) {
            actions.push(action);
        } else {
            break;
        }
    }

    Ok(ReceiptRule {
        name,
        enabled,
        scan_enabled,
        tls_policy,
        recipients,
        actions,
    })
}

fn parse_action(params: &HashMap<String, String>, prefix: &str) -> Option<ReceiptAction> {
    // S3Action
    if let Some(bucket) = params.get(&format!("{prefix}.S3Action.BucketName")) {
        return Some(ReceiptAction::S3 {
            bucket_name: bucket.clone(),
            object_key_prefix: params
                .get(&format!("{prefix}.S3Action.ObjectKeyPrefix"))
                .cloned(),
            topic_arn: params.get(&format!("{prefix}.S3Action.TopicArn")).cloned(),
            kms_key_arn: params.get(&format!("{prefix}.S3Action.KmsKeyArn")).cloned(),
        });
    }
    // SNSAction
    if let Some(topic_arn) = params.get(&format!("{prefix}.SNSAction.TopicArn")) {
        return Some(ReceiptAction::Sns {
            topic_arn: topic_arn.clone(),
            encoding: params.get(&format!("{prefix}.SNSAction.Encoding")).cloned(),
        });
    }
    // LambdaAction
    if let Some(function_arn) = params.get(&format!("{prefix}.LambdaAction.FunctionArn")) {
        return Some(ReceiptAction::Lambda {
            function_arn: function_arn.clone(),
            invocation_type: params
                .get(&format!("{prefix}.LambdaAction.InvocationType"))
                .cloned(),
            topic_arn: params
                .get(&format!("{prefix}.LambdaAction.TopicArn"))
                .cloned(),
        });
    }
    // BounceAction
    if let Some(smtp_code) = params.get(&format!("{prefix}.BounceAction.SmtpReplyCode")) {
        return Some(ReceiptAction::Bounce {
            smtp_reply_code: smtp_code.clone(),
            message: params
                .get(&format!("{prefix}.BounceAction.Message"))
                .cloned()
                .unwrap_or_default(),
            sender: params
                .get(&format!("{prefix}.BounceAction.Sender"))
                .cloned()
                .unwrap_or_default(),
            status_code: params
                .get(&format!("{prefix}.BounceAction.StatusCode"))
                .cloned(),
            topic_arn: params
                .get(&format!("{prefix}.BounceAction.TopicArn"))
                .cloned(),
        });
    }
    // AddHeaderAction
    if let Some(header_name) = params.get(&format!("{prefix}.AddHeaderAction.HeaderName")) {
        return Some(ReceiptAction::AddHeader {
            header_name: header_name.clone(),
            header_value: params
                .get(&format!("{prefix}.AddHeaderAction.HeaderValue"))
                .cloned()
                .unwrap_or_default(),
        });
    }
    // StopAction
    if let Some(scope) = params.get(&format!("{prefix}.StopAction.Scope")) {
        return Some(ReceiptAction::Stop {
            scope: scope.clone(),
            topic_arn: params
                .get(&format!("{prefix}.StopAction.TopicArn"))
                .cloned(),
        });
    }
    None
}

/// Serialize a ReceiptRule to XML.
fn rule_to_xml(rule: &ReceiptRule) -> String {
    let mut xml = String::new();
    xml.push_str("<member>");
    xml.push_str(&format!("<Name>{}</Name>", xml_escape(&rule.name)));
    xml.push_str(&format!("<Enabled>{}</Enabled>", rule.enabled));
    xml.push_str(&format!("<ScanEnabled>{}</ScanEnabled>", rule.scan_enabled));
    xml.push_str(&format!(
        "<TlsPolicy>{}</TlsPolicy>",
        xml_escape(&rule.tls_policy)
    ));
    if !rule.recipients.is_empty() {
        xml.push_str("<Recipients>");
        for r in &rule.recipients {
            xml.push_str(&format!("<member>{}</member>", xml_escape(r)));
        }
        xml.push_str("</Recipients>");
    }
    if !rule.actions.is_empty() {
        xml.push_str("<Actions>");
        for action in &rule.actions {
            xml.push_str("<member>");
            match action {
                ReceiptAction::S3 {
                    bucket_name,
                    object_key_prefix,
                    topic_arn,
                    kms_key_arn,
                } => {
                    xml.push_str("<S3Action>");
                    xml.push_str(&format!(
                        "<BucketName>{}</BucketName>",
                        xml_escape(bucket_name)
                    ));
                    if let Some(p) = object_key_prefix {
                        xml.push_str(&format!(
                            "<ObjectKeyPrefix>{}</ObjectKeyPrefix>",
                            xml_escape(p)
                        ));
                    }
                    if let Some(t) = topic_arn {
                        xml.push_str(&format!("<TopicArn>{}</TopicArn>", xml_escape(t)));
                    }
                    if let Some(k) = kms_key_arn {
                        xml.push_str(&format!("<KmsKeyArn>{}</KmsKeyArn>", xml_escape(k)));
                    }
                    xml.push_str("</S3Action>");
                }
                ReceiptAction::Sns {
                    topic_arn,
                    encoding,
                } => {
                    xml.push_str("<SNSAction>");
                    xml.push_str(&format!("<TopicArn>{}</TopicArn>", xml_escape(topic_arn)));
                    if let Some(e) = encoding {
                        xml.push_str(&format!("<Encoding>{}</Encoding>", xml_escape(e)));
                    }
                    xml.push_str("</SNSAction>");
                }
                ReceiptAction::Lambda {
                    function_arn,
                    invocation_type,
                    topic_arn,
                } => {
                    xml.push_str("<LambdaAction>");
                    xml.push_str(&format!(
                        "<FunctionArn>{}</FunctionArn>",
                        xml_escape(function_arn)
                    ));
                    if let Some(t) = invocation_type {
                        xml.push_str(&format!(
                            "<InvocationType>{}</InvocationType>",
                            xml_escape(t)
                        ));
                    }
                    if let Some(t) = topic_arn {
                        xml.push_str(&format!("<TopicArn>{}</TopicArn>", xml_escape(t)));
                    }
                    xml.push_str("</LambdaAction>");
                }
                ReceiptAction::Bounce {
                    smtp_reply_code,
                    message,
                    sender,
                    status_code,
                    topic_arn,
                } => {
                    xml.push_str("<BounceAction>");
                    xml.push_str(&format!(
                        "<SmtpReplyCode>{}</SmtpReplyCode>",
                        xml_escape(smtp_reply_code)
                    ));
                    xml.push_str(&format!("<Message>{}</Message>", xml_escape(message)));
                    xml.push_str(&format!("<Sender>{}</Sender>", xml_escape(sender)));
                    if let Some(sc) = status_code {
                        xml.push_str(&format!("<StatusCode>{}</StatusCode>", xml_escape(sc)));
                    }
                    if let Some(t) = topic_arn {
                        xml.push_str(&format!("<TopicArn>{}</TopicArn>", xml_escape(t)));
                    }
                    xml.push_str("</BounceAction>");
                }
                ReceiptAction::AddHeader {
                    header_name,
                    header_value,
                } => {
                    xml.push_str("<AddHeaderAction>");
                    xml.push_str(&format!(
                        "<HeaderName>{}</HeaderName>",
                        xml_escape(header_name)
                    ));
                    xml.push_str(&format!(
                        "<HeaderValue>{}</HeaderValue>",
                        xml_escape(header_value)
                    ));
                    xml.push_str("</AddHeaderAction>");
                }
                ReceiptAction::Stop { scope, topic_arn } => {
                    xml.push_str("<StopAction>");
                    xml.push_str(&format!("<Scope>{}</Scope>", xml_escape(scope)));
                    if let Some(t) = topic_arn {
                        xml.push_str(&format!("<TopicArn>{}</TopicArn>", xml_escape(t)));
                    }
                    xml.push_str("</StopAction>");
                }
            }
            xml.push_str("</member>");
        }
        xml.push_str("</Actions>");
    }
    xml.push_str("</member>");
    xml
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

// ── Receipt Rule Set operations ──

fn create_receipt_rule_set(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let name = required_param(&req.query_params, "RuleSetName")?;
    let mut st = state.write();
    if st.receipt_rule_sets.contains_key(name) {
        return Err(AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "AlreadyExistsException",
            format!("Rule set with name '{name}' already exists"),
        ));
    }
    st.receipt_rule_sets.insert(
        name.to_string(),
        ReceiptRuleSet {
            name: name.to_string(),
            rules: Vec::new(),
            created_at: Utc::now(),
        },
    );
    Ok(xml_metadata_only("CreateReceiptRuleSet", &req.request_id))
}

fn delete_receipt_rule_set(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let name = required_param(&req.query_params, "RuleSetName")?;
    let mut st = state.write();
    if !st.receipt_rule_sets.contains_key(name) {
        return Err(AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "RuleSetDoesNotExistException",
            format!("Rule set with name '{name}' does not exist"),
        ));
    }
    // Cannot delete the active rule set
    if st.active_receipt_rule_set.as_deref() == Some(name) {
        return Err(AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "CannotDeleteException",
            "Cannot delete the active receipt rule set. Deactivate it first.",
        ));
    }
    st.receipt_rule_sets.remove(name);
    Ok(xml_metadata_only("DeleteReceiptRuleSet", &req.request_id))
}

fn describe_receipt_rule_set(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let name = required_param(&req.query_params, "RuleSetName")?;
    let st = state.read();
    let rule_set = st.receipt_rule_sets.get(name).ok_or_else(|| {
        AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "RuleSetDoesNotExistException",
            format!("Rule set with name '{name}' does not exist"),
        )
    })?;

    let mut rules_xml = String::from("<Rules>");
    for rule in &rule_set.rules {
        rules_xml.push_str(&rule_to_xml(rule));
    }
    rules_xml.push_str("</Rules>");

    let inner = format!(
        "<Metadata><Name>{}</Name><CreatedTimestamp>{}</CreatedTimestamp></Metadata>{}",
        xml_escape(&rule_set.name),
        rule_set.created_at.to_rfc3339(),
        rules_xml,
    );
    Ok(AwsResponse::xml(
        StatusCode::OK,
        xml_wrap("DescribeReceiptRuleSet", &inner, &req.request_id),
    ))
}

fn list_receipt_rule_sets(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let st = state.read();
    let mut inner = String::from("<RuleSets>");
    let mut sets: Vec<&ReceiptRuleSet> = st.receipt_rule_sets.values().collect();
    sets.sort_by_key(|s| &s.name);
    for rs in sets {
        inner.push_str(&format!(
            "<member><Name>{}</Name><CreatedTimestamp>{}</CreatedTimestamp></member>",
            xml_escape(&rs.name),
            rs.created_at.to_rfc3339(),
        ));
    }
    inner.push_str("</RuleSets>");
    Ok(AwsResponse::xml(
        StatusCode::OK,
        xml_wrap("ListReceiptRuleSets", &inner, &req.request_id),
    ))
}

fn clone_receipt_rule_set(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let new_name = required_param(&req.query_params, "RuleSetName")?;
    let source_name = required_param(&req.query_params, "OriginalRuleSetName")?;
    let mut st = state.write();

    if st.receipt_rule_sets.contains_key(new_name) {
        return Err(AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "AlreadyExistsException",
            format!("Rule set with name '{new_name}' already exists"),
        ));
    }
    let source = st.receipt_rule_sets.get(source_name).ok_or_else(|| {
        AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "RuleSetDoesNotExistException",
            format!("Rule set with name '{source_name}' does not exist"),
        )
    })?;
    let cloned = ReceiptRuleSet {
        name: new_name.to_string(),
        rules: source.rules.clone(),
        created_at: Utc::now(),
    };
    st.receipt_rule_sets.insert(new_name.to_string(), cloned);
    Ok(xml_metadata_only("CloneReceiptRuleSet", &req.request_id))
}

fn set_active_receipt_rule_set(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let mut st = state.write();
    // If RuleSetName is empty or absent, deactivate.
    match req.query_params.get("RuleSetName") {
        Some(name) if !name.is_empty() => {
            if !st.receipt_rule_sets.contains_key(name.as_str()) {
                return Err(AwsServiceError::aws_error(
                    StatusCode::BAD_REQUEST,
                    "RuleSetDoesNotExistException",
                    format!("Rule set with name '{name}' does not exist"),
                ));
            }
            st.active_receipt_rule_set = Some(name.clone());
        }
        _ => {
            st.active_receipt_rule_set = None;
        }
    }
    Ok(xml_metadata_only(
        "SetActiveReceiptRuleSet",
        &req.request_id,
    ))
}

fn reorder_receipt_rule_set(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let rule_set_name = required_param(&req.query_params, "RuleSetName")?;
    let mut st = state.write();
    let rule_set = st.receipt_rule_sets.get_mut(rule_set_name).ok_or_else(|| {
        AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "RuleSetDoesNotExistException",
            format!("Rule set with name '{rule_set_name}' does not exist"),
        )
    })?;

    // Parse ordered rule names: RuleNames.member.1, RuleNames.member.2, ...
    let mut ordered_names = Vec::new();
    for i in 1.. {
        let key = format!("RuleNames.member.{i}");
        match req.query_params.get(&key) {
            Some(v) => ordered_names.push(v.clone()),
            None => break,
        }
    }

    // Validate all names exist
    for name in &ordered_names {
        if !rule_set.rules.iter().any(|r| &r.name == name) {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "RuleDoesNotExistException",
                format!("Rule '{name}' does not exist in rule set '{rule_set_name}'"),
            ));
        }
    }

    // Reorder
    let mut reordered = Vec::with_capacity(rule_set.rules.len());
    for name in &ordered_names {
        if let Some(pos) = rule_set.rules.iter().position(|r| &r.name == name) {
            reordered.push(rule_set.rules.remove(pos));
        }
    }
    // Append any rules not mentioned in the new order
    reordered.append(&mut rule_set.rules);
    rule_set.rules = reordered;

    Ok(xml_metadata_only("ReorderReceiptRuleSet", &req.request_id))
}

// ── Receipt Rule operations ──

fn create_receipt_rule(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let rule_set_name = required_param(&req.query_params, "RuleSetName")?;
    let rule = parse_receipt_rule(&req.query_params)?;
    let after = req.query_params.get("After").cloned();

    let mut st = state.write();
    let rule_set = st.receipt_rule_sets.get_mut(rule_set_name).ok_or_else(|| {
        AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "RuleSetDoesNotExistException",
            format!("Rule set with name '{rule_set_name}' does not exist"),
        )
    })?;

    if rule_set.rules.iter().any(|r| r.name == rule.name) {
        return Err(AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "AlreadyExistsException",
            format!(
                "Rule '{}' already exists in rule set '{rule_set_name}'",
                rule.name
            ),
        ));
    }

    if let Some(after_name) = after {
        if let Some(pos) = rule_set.rules.iter().position(|r| r.name == after_name) {
            rule_set.rules.insert(pos + 1, rule);
        } else {
            rule_set.rules.push(rule);
        }
    } else {
        rule_set.rules.push(rule);
    }

    Ok(xml_metadata_only("CreateReceiptRule", &req.request_id))
}

fn delete_receipt_rule(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let rule_set_name = required_param(&req.query_params, "RuleSetName")?;
    let rule_name = required_param(&req.query_params, "RuleName")?;

    let mut st = state.write();
    let rule_set = st.receipt_rule_sets.get_mut(rule_set_name).ok_or_else(|| {
        AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "RuleSetDoesNotExistException",
            format!("Rule set with name '{rule_set_name}' does not exist"),
        )
    })?;

    let pos = rule_set
        .rules
        .iter()
        .position(|r| r.name == rule_name)
        .ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "RuleDoesNotExistException",
                format!("Rule '{rule_name}' does not exist in rule set '{rule_set_name}'"),
            )
        })?;
    rule_set.rules.remove(pos);
    Ok(xml_metadata_only("DeleteReceiptRule", &req.request_id))
}

fn describe_receipt_rule(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let rule_set_name = required_param(&req.query_params, "RuleSetName")?;
    let rule_name = required_param(&req.query_params, "RuleName")?;

    let st = state.read();
    let rule_set = st.receipt_rule_sets.get(rule_set_name).ok_or_else(|| {
        AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "RuleSetDoesNotExistException",
            format!("Rule set with name '{rule_set_name}' does not exist"),
        )
    })?;
    let rule = rule_set
        .rules
        .iter()
        .find(|r| r.name == rule_name)
        .ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "RuleDoesNotExistException",
                format!("Rule '{rule_name}' does not exist in rule set '{rule_set_name}'"),
            )
        })?;

    // rule_to_xml wraps in <member>, strip it for describe
    let rule_xml = rule_to_xml(rule);
    let inner_xml = rule_xml
        .strip_prefix("<member>")
        .and_then(|s| s.strip_suffix("</member>"))
        .unwrap_or(&rule_xml);
    let inner = format!("<Rule>{inner_xml}</Rule>");
    Ok(AwsResponse::xml(
        StatusCode::OK,
        xml_wrap("DescribeReceiptRule", &inner, &req.request_id),
    ))
}

fn update_receipt_rule(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let rule_set_name = required_param(&req.query_params, "RuleSetName")?;
    let new_rule = parse_receipt_rule(&req.query_params)?;

    let mut st = state.write();
    let rule_set = st.receipt_rule_sets.get_mut(rule_set_name).ok_or_else(|| {
        AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "RuleSetDoesNotExistException",
            format!("Rule set with name '{rule_set_name}' does not exist"),
        )
    })?;

    let rule = rule_set
        .rules
        .iter_mut()
        .find(|r| r.name == new_rule.name)
        .ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "RuleDoesNotExistException",
                format!(
                    "Rule '{}' does not exist in rule set '{rule_set_name}'",
                    new_rule.name
                ),
            )
        })?;

    *rule = new_rule;
    Ok(xml_metadata_only("UpdateReceiptRule", &req.request_id))
}

// ── Receipt Filter operations ──

fn create_receipt_filter(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let name = required_param(&req.query_params, "Filter.Name")?;
    let cidr = required_param(&req.query_params, "Filter.IpFilter.Cidr")?;
    let policy = required_param(&req.query_params, "Filter.IpFilter.Policy")?;

    let mut st = state.write();
    if st.receipt_filters.contains_key(name) {
        return Err(AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "AlreadyExistsException",
            format!("Filter with name '{name}' already exists"),
        ));
    }

    st.receipt_filters.insert(
        name.to_string(),
        ReceiptFilter {
            name: name.to_string(),
            ip_filter: IpFilter {
                cidr: cidr.to_string(),
                policy: policy.to_string(),
            },
        },
    );
    Ok(xml_metadata_only("CreateReceiptFilter", &req.request_id))
}

fn delete_receipt_filter(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let name = required_param(&req.query_params, "FilterName")?;
    let mut st = state.write();
    if st.receipt_filters.remove(name).is_none() {
        return Err(AwsServiceError::aws_error(
            StatusCode::BAD_REQUEST,
            "FilterDoesNotExistException",
            format!("Filter with name '{name}' does not exist"),
        ));
    }
    Ok(xml_metadata_only("DeleteReceiptFilter", &req.request_id))
}

fn list_receipt_filters(
    state: &SharedSesState,
    req: &AwsRequest,
) -> Result<AwsResponse, AwsServiceError> {
    let st = state.read();
    let mut inner = String::from("<Filters>");
    let mut filters: Vec<&ReceiptFilter> = st.receipt_filters.values().collect();
    filters.sort_by_key(|f| &f.name);
    for f in filters {
        inner.push_str(&format!(
            "<member><Name>{}</Name><IpFilter><Cidr>{}</Cidr><Policy>{}</Policy></IpFilter></member>",
            xml_escape(&f.name),
            xml_escape(&f.ip_filter.cidr),
            xml_escape(&f.ip_filter.policy),
        ));
    }
    inner.push_str("</Filters>");
    Ok(AwsResponse::xml(
        StatusCode::OK,
        xml_wrap("ListReceiptFilters", &inner, &req.request_id),
    ))
}

// ── Inbound email processing ──

/// Evaluate an inbound email against the active receipt rule set.
/// Returns the list of matched rules and actions that should be executed.
pub fn evaluate_inbound_email(
    state: &SharedSesState,
    from: &str,
    to: &[String],
    subject: &str,
    body: &str,
) -> (String, Vec<String>, Vec<(String, ReceiptAction)>) {
    let message_id = uuid::Uuid::new_v4().to_string();
    let st = state.read();

    let active_name = match &st.active_receipt_rule_set {
        Some(name) => name.clone(),
        None => return (message_id, Vec::new(), Vec::new()),
    };

    let rule_set = match st.receipt_rule_sets.get(&active_name) {
        Some(rs) => rs,
        None => return (message_id, Vec::new(), Vec::new()),
    };

    let mut matched_rules = Vec::new();
    let mut actions_to_execute = Vec::new();
    let mut stop = false;

    for rule in &rule_set.rules {
        if !rule.enabled {
            continue;
        }
        if stop {
            break;
        }

        // Check if any recipient matches the rule's recipients list.
        // If the rule has no recipients, it matches all emails.
        let matches = rule.recipients.is_empty()
            || to.iter().any(|recipient| {
                rule.recipients.iter().any(|r| {
                    // Match exact address or domain
                    recipient == r || recipient.ends_with(&format!("@{r}"))
                })
            });

        if matches {
            matched_rules.push(rule.name.clone());
            for action in &rule.actions {
                actions_to_execute.push((rule.name.clone(), action.clone()));
                if matches!(action, ReceiptAction::Stop { .. }) {
                    stop = true;
                    break;
                }
            }
        }
    }

    // Record the inbound email
    drop(st);
    let mut st = state.write();
    st.inbound_emails.push(crate::state::InboundEmail {
        message_id: message_id.clone(),
        from: from.to_string(),
        to: to.to_vec(),
        subject: subject.to_string(),
        body: body.to_string(),
        matched_rules: matched_rules.clone(),
        actions_executed: actions_to_execute
            .iter()
            .map(|(rule, action)| format!("{rule}:{}", action_type_name(action)))
            .collect(),
        timestamp: Utc::now(),
    });

    (message_id, matched_rules, actions_to_execute)
}

fn action_type_name(action: &ReceiptAction) -> &'static str {
    match action {
        ReceiptAction::S3 { .. } => "S3",
        ReceiptAction::Sns { .. } => "SNS",
        ReceiptAction::Lambda { .. } => "Lambda",
        ReceiptAction::Bounce { .. } => "Bounce",
        ReceiptAction::AddHeader { .. } => "AddHeader",
        ReceiptAction::Stop { .. } => "Stop",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::SesState;
    use bytes::Bytes;
    use fakecloud_core::service::AwsService;
    use http::HeaderMap;
    use parking_lot::RwLock;
    use std::sync::Arc;

    fn make_state() -> SharedSesState {
        Arc::new(RwLock::new(SesState::new("123456789012", "us-east-1")))
    }

    fn make_v1_request(action: &str, params: Vec<(&str, &str)>) -> AwsRequest {
        let mut query_params: HashMap<String, String> = HashMap::new();
        query_params.insert("Action".to_string(), action.to_string());
        for (k, v) in params {
            query_params.insert(k.to_string(), v.to_string());
        }
        AwsRequest {
            service: "ses".to_string(),
            action: action.to_string(),
            region: "us-east-1".to_string(),
            account_id: "123456789012".to_string(),
            request_id: "test-request-id".to_string(),
            headers: HeaderMap::new(),
            query_params,
            body: Bytes::new(),
            path_segments: Vec::new(),
            raw_path: "/".to_string(),
            raw_query: String::new(),
            method: http::Method::POST,
            is_query_protocol: true,
            access_key_id: None,
        }
    }

    #[test]
    fn test_create_receipt_rule_set() {
        let state = make_state();
        let req = make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "my-rules")]);
        let resp = handle_v1_action(&state, &req).unwrap();
        assert_eq!(resp.status, StatusCode::OK);
        let body = String::from_utf8(resp.body.to_vec()).unwrap();
        assert!(body.contains("CreateReceiptRuleSetResponse"));

        // Duplicate should fail
        let req = make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "my-rules")]);
        match handle_v1_action(&state, &req) {
            Err(e) => assert_eq!(e.code(), "AlreadyExistsException"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_list_receipt_rule_sets() {
        let state = make_state();
        // Create two rule sets
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "set-a")]),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "set-b")]),
        )
        .unwrap();

        let req = make_v1_request("ListReceiptRuleSets", vec![]);
        let resp = handle_v1_action(&state, &req).unwrap();
        let body = String::from_utf8(resp.body.to_vec()).unwrap();
        assert!(body.contains("<Name>set-a</Name>"));
        assert!(body.contains("<Name>set-b</Name>"));
    }

    #[test]
    fn test_delete_receipt_rule_set() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "to-delete")]),
        )
        .unwrap();
        let req = make_v1_request("DeleteReceiptRuleSet", vec![("RuleSetName", "to-delete")]);
        let resp = handle_v1_action(&state, &req).unwrap();
        assert_eq!(resp.status, StatusCode::OK);

        // Should not exist anymore
        match handle_v1_action(&state, &req) {
            Err(e) => assert_eq!(e.code(), "RuleSetDoesNotExistException"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_cannot_delete_active_rule_set() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "active-set")]),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request(
                "SetActiveReceiptRuleSet",
                vec![("RuleSetName", "active-set")],
            ),
        )
        .unwrap();

        match handle_v1_action(
            &state,
            &make_v1_request("DeleteReceiptRuleSet", vec![("RuleSetName", "active-set")]),
        ) {
            Err(e) => assert_eq!(e.code(), "CannotDeleteException"),
            Ok(_) => panic!("expected error"),
        }
    }

    #[test]
    fn test_clone_receipt_rule_set() {
        let state = make_state();
        // Create source with a rule
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "source")]),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request(
                "CreateReceiptRule",
                vec![
                    ("RuleSetName", "source"),
                    ("Rule.Name", "rule1"),
                    ("Rule.Enabled", "true"),
                ],
            ),
        )
        .unwrap();

        // Clone
        let req = make_v1_request(
            "CloneReceiptRuleSet",
            vec![("RuleSetName", "cloned"), ("OriginalRuleSetName", "source")],
        );
        handle_v1_action(&state, &req).unwrap();

        // Verify clone has the rule
        let st = state.read();
        let cloned = st.receipt_rule_sets.get("cloned").unwrap();
        assert_eq!(cloned.rules.len(), 1);
        assert_eq!(cloned.rules[0].name, "rule1");
    }

    #[test]
    fn test_set_active_receipt_rule_set() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "my-set")]),
        )
        .unwrap();

        // Activate
        handle_v1_action(
            &state,
            &make_v1_request("SetActiveReceiptRuleSet", vec![("RuleSetName", "my-set")]),
        )
        .unwrap();
        assert_eq!(
            state.read().active_receipt_rule_set.as_deref(),
            Some("my-set")
        );

        // Deactivate (empty name)
        handle_v1_action(
            &state,
            &make_v1_request("SetActiveReceiptRuleSet", vec![("RuleSetName", "")]),
        )
        .unwrap();
        assert!(state.read().active_receipt_rule_set.is_none());
    }

    #[test]
    fn test_create_and_describe_receipt_rule() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "my-set")]),
        )
        .unwrap();

        // Create rule with S3 action and recipients
        let req = make_v1_request(
            "CreateReceiptRule",
            vec![
                ("RuleSetName", "my-set"),
                ("Rule.Name", "store-email"),
                ("Rule.Enabled", "true"),
                ("Rule.ScanEnabled", "true"),
                ("Rule.TlsPolicy", "Require"),
                ("Rule.Recipients.member.1", "user@example.com"),
                ("Rule.Recipients.member.2", "example.com"),
                ("Rule.Actions.member.1.S3Action.BucketName", "my-bucket"),
                ("Rule.Actions.member.1.S3Action.ObjectKeyPrefix", "emails/"),
            ],
        );
        handle_v1_action(&state, &req).unwrap();

        // Describe the rule
        let req = make_v1_request(
            "DescribeReceiptRule",
            vec![("RuleSetName", "my-set"), ("RuleName", "store-email")],
        );
        let resp = handle_v1_action(&state, &req).unwrap();
        let body = String::from_utf8(resp.body.to_vec()).unwrap();
        assert!(body.contains("<Name>store-email</Name>"));
        assert!(body.contains("<Enabled>true</Enabled>"));
        assert!(body.contains("<ScanEnabled>true</ScanEnabled>"));
        assert!(body.contains("<TlsPolicy>Require</TlsPolicy>"));
        assert!(body.contains("<BucketName>my-bucket</BucketName>"));
        assert!(body.contains("<ObjectKeyPrefix>emails/</ObjectKeyPrefix>"));
        assert!(body.contains("user@example.com"));
    }

    #[test]
    fn test_update_receipt_rule() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "my-set")]),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request(
                "CreateReceiptRule",
                vec![
                    ("RuleSetName", "my-set"),
                    ("Rule.Name", "rule1"),
                    ("Rule.Enabled", "true"),
                ],
            ),
        )
        .unwrap();

        // Update: disable the rule and add action
        let req = make_v1_request(
            "UpdateReceiptRule",
            vec![
                ("RuleSetName", "my-set"),
                ("Rule.Name", "rule1"),
                ("Rule.Enabled", "false"),
                (
                    "Rule.Actions.member.1.SNSAction.TopicArn",
                    "arn:aws:sns:us-east-1:123456789012:my-topic",
                ),
            ],
        );
        handle_v1_action(&state, &req).unwrap();

        let st = state.read();
        let rule = &st.receipt_rule_sets.get("my-set").unwrap().rules[0];
        assert!(!rule.enabled);
        assert_eq!(rule.actions.len(), 1);
        assert!(matches!(&rule.actions[0], ReceiptAction::Sns { .. }));
    }

    #[test]
    fn test_delete_receipt_rule() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "my-set")]),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request(
                "CreateReceiptRule",
                vec![("RuleSetName", "my-set"), ("Rule.Name", "rule1")],
            ),
        )
        .unwrap();

        let req = make_v1_request(
            "DeleteReceiptRule",
            vec![("RuleSetName", "my-set"), ("RuleName", "rule1")],
        );
        handle_v1_action(&state, &req).unwrap();

        let st = state.read();
        assert!(st.receipt_rule_sets.get("my-set").unwrap().rules.is_empty());
    }

    #[test]
    fn test_reorder_receipt_rule_set() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "my-set")]),
        )
        .unwrap();
        for name in &["a", "b", "c"] {
            handle_v1_action(
                &state,
                &make_v1_request(
                    "CreateReceiptRule",
                    vec![("RuleSetName", "my-set"), ("Rule.Name", name)],
                ),
            )
            .unwrap();
        }

        // Reorder: c, a, b
        let req = make_v1_request(
            "ReorderReceiptRuleSet",
            vec![
                ("RuleSetName", "my-set"),
                ("RuleNames.member.1", "c"),
                ("RuleNames.member.2", "a"),
                ("RuleNames.member.3", "b"),
            ],
        );
        handle_v1_action(&state, &req).unwrap();

        let st = state.read();
        let names: Vec<&str> = st
            .receipt_rule_sets
            .get("my-set")
            .unwrap()
            .rules
            .iter()
            .map(|r| r.name.as_str())
            .collect();
        assert_eq!(names, vec!["c", "a", "b"]);
    }

    #[test]
    fn test_receipt_filter_lifecycle() {
        let state = make_state();

        // Create filter
        let req = make_v1_request(
            "CreateReceiptFilter",
            vec![
                ("Filter.Name", "allow-internal"),
                ("Filter.IpFilter.Cidr", "10.0.0.0/8"),
                ("Filter.IpFilter.Policy", "Allow"),
            ],
        );
        handle_v1_action(&state, &req).unwrap();

        // List filters
        let req = make_v1_request("ListReceiptFilters", vec![]);
        let resp = handle_v1_action(&state, &req).unwrap();
        let body = String::from_utf8(resp.body.to_vec()).unwrap();
        assert!(body.contains("<Name>allow-internal</Name>"));
        assert!(body.contains("<Cidr>10.0.0.0/8</Cidr>"));
        assert!(body.contains("<Policy>Allow</Policy>"));

        // Delete filter
        let req = make_v1_request(
            "DeleteReceiptFilter",
            vec![("FilterName", "allow-internal")],
        );
        handle_v1_action(&state, &req).unwrap();

        // List should be empty
        let req = make_v1_request("ListReceiptFilters", vec![]);
        let resp = handle_v1_action(&state, &req).unwrap();
        let body = String::from_utf8(resp.body.to_vec()).unwrap();
        assert!(!body.contains("allow-internal"));
    }

    #[test]
    fn test_evaluate_inbound_email_no_active_set() {
        let state = make_state();
        let (msg_id, matched, actions) = evaluate_inbound_email(
            &state,
            "sender@example.com",
            &["recipient@example.com".to_string()],
            "Test",
            "Hello",
        );
        assert!(!msg_id.is_empty());
        assert!(matched.is_empty());
        assert!(actions.is_empty());
    }

    #[test]
    fn test_evaluate_inbound_email_matching_rule() {
        let state = make_state();

        // Setup: create rule set, add rule, activate
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "active")]),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request(
                "CreateReceiptRule",
                vec![
                    ("RuleSetName", "active"),
                    ("Rule.Name", "catch-all"),
                    ("Rule.Enabled", "true"),
                    ("Rule.Actions.member.1.S3Action.BucketName", "emails-bucket"),
                ],
            ),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request("SetActiveReceiptRuleSet", vec![("RuleSetName", "active")]),
        )
        .unwrap();

        let (_msg_id, matched, actions) = evaluate_inbound_email(
            &state,
            "sender@example.com",
            &["anyone@example.com".to_string()],
            "Hello",
            "Body",
        );
        assert_eq!(matched, vec!["catch-all"]);
        assert_eq!(actions.len(), 1);
        assert!(
            matches!(&actions[0].1, ReceiptAction::S3 { bucket_name, .. } if bucket_name == "emails-bucket")
        );
    }

    #[test]
    fn test_evaluate_inbound_email_recipient_filter() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "set")]),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request(
                "CreateReceiptRule",
                vec![
                    ("RuleSetName", "set"),
                    ("Rule.Name", "domain-rule"),
                    ("Rule.Enabled", "true"),
                    ("Rule.Recipients.member.1", "example.com"),
                    (
                        "Rule.Actions.member.1.SNSAction.TopicArn",
                        "arn:aws:sns:us-east-1:123456789012:topic",
                    ),
                ],
            ),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request("SetActiveReceiptRuleSet", vec![("RuleSetName", "set")]),
        )
        .unwrap();

        // Should match: recipient@example.com matches domain "example.com"
        let (_msg_id, matched, _actions) = evaluate_inbound_email(
            &state,
            "sender@other.com",
            &["recipient@example.com".to_string()],
            "Test",
            "Body",
        );
        assert_eq!(matched, vec!["domain-rule"]);

        // Should NOT match: recipient@other.com
        let (_msg_id, matched, _actions) = evaluate_inbound_email(
            &state,
            "sender@other.com",
            &["recipient@other.com".to_string()],
            "Test",
            "Body",
        );
        assert!(matched.is_empty());
    }

    #[test]
    fn test_evaluate_inbound_email_stop_action() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "set")]),
        )
        .unwrap();
        // Rule 1: stop action
        handle_v1_action(
            &state,
            &make_v1_request(
                "CreateReceiptRule",
                vec![
                    ("RuleSetName", "set"),
                    ("Rule.Name", "stop-rule"),
                    ("Rule.Enabled", "true"),
                    ("Rule.Actions.member.1.StopAction.Scope", "RuleSet"),
                ],
            ),
        )
        .unwrap();
        // Rule 2: should not be reached
        handle_v1_action(
            &state,
            &make_v1_request(
                "CreateReceiptRule",
                vec![
                    ("RuleSetName", "set"),
                    ("Rule.Name", "after-stop"),
                    ("Rule.Enabled", "true"),
                    ("Rule.Actions.member.1.S3Action.BucketName", "bucket"),
                ],
            ),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request("SetActiveReceiptRuleSet", vec![("RuleSetName", "set")]),
        )
        .unwrap();

        let (_msg_id, matched, actions) = evaluate_inbound_email(
            &state,
            "sender@example.com",
            &["anyone@example.com".to_string()],
            "Test",
            "Body",
        );
        // Only stop-rule should match, after-stop should not be evaluated
        assert_eq!(matched, vec!["stop-rule"]);
        assert_eq!(actions.len(), 1);
        assert!(matches!(&actions[0].1, ReceiptAction::Stop { .. }));
    }

    #[test]
    fn test_describe_receipt_rule_set() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "my-set")]),
        )
        .unwrap();
        handle_v1_action(
            &state,
            &make_v1_request(
                "CreateReceiptRule",
                vec![
                    ("RuleSetName", "my-set"),
                    ("Rule.Name", "rule1"),
                    ("Rule.Enabled", "true"),
                ],
            ),
        )
        .unwrap();

        let req = make_v1_request("DescribeReceiptRuleSet", vec![("RuleSetName", "my-set")]);
        let resp = handle_v1_action(&state, &req).unwrap();
        let body = String::from_utf8(resp.body.to_vec()).unwrap();
        assert!(body.contains("<Name>my-set</Name>"));
        assert!(body.contains("<Name>rule1</Name>"));
        assert!(body.contains("<Rules>"));
    }

    #[test]
    fn test_all_action_types_parsing() {
        let state = make_state();
        handle_v1_action(
            &state,
            &make_v1_request("CreateReceiptRuleSet", vec![("RuleSetName", "set")]),
        )
        .unwrap();

        let req = make_v1_request(
            "CreateReceiptRule",
            vec![
                ("RuleSetName", "set"),
                ("Rule.Name", "multi-action"),
                ("Rule.Enabled", "true"),
                ("Rule.Actions.member.1.S3Action.BucketName", "bucket"),
                (
                    "Rule.Actions.member.2.SNSAction.TopicArn",
                    "arn:aws:sns:us-east-1:123:topic",
                ),
                ("Rule.Actions.member.2.SNSAction.Encoding", "UTF-8"),
                (
                    "Rule.Actions.member.3.LambdaAction.FunctionArn",
                    "arn:aws:lambda:us-east-1:123:function:my-fn",
                ),
                ("Rule.Actions.member.3.LambdaAction.InvocationType", "Event"),
                ("Rule.Actions.member.4.BounceAction.SmtpReplyCode", "550"),
                ("Rule.Actions.member.4.BounceAction.Message", "rejected"),
                (
                    "Rule.Actions.member.4.BounceAction.Sender",
                    "noreply@example.com",
                ),
                ("Rule.Actions.member.5.AddHeaderAction.HeaderName", "X-Test"),
                ("Rule.Actions.member.5.AddHeaderAction.HeaderValue", "true"),
                ("Rule.Actions.member.6.StopAction.Scope", "RuleSet"),
            ],
        );
        handle_v1_action(&state, &req).unwrap();

        let st = state.read();
        let rule = &st.receipt_rule_sets.get("set").unwrap().rules[0];
        assert_eq!(rule.actions.len(), 6);
        assert!(matches!(&rule.actions[0], ReceiptAction::S3 { .. }));
        assert!(matches!(&rule.actions[1], ReceiptAction::Sns { .. }));
        assert!(matches!(&rule.actions[2], ReceiptAction::Lambda { .. }));
        assert!(matches!(&rule.actions[3], ReceiptAction::Bounce { .. }));
        assert!(matches!(&rule.actions[4], ReceiptAction::AddHeader { .. }));
        assert!(matches!(&rule.actions[5], ReceiptAction::Stop { .. }));
    }
}
