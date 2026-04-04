use async_trait::async_trait;
use chrono::Utc;
use http::StatusCode;
use serde_json::{json, Value};

use std::sync::Arc;

use fakecloud_core::delivery::DeliveryBus;
use fakecloud_core::service::{AwsRequest, AwsResponse, AwsService, AwsServiceError};

use crate::state::{EventBus, EventRule, EventTarget, PutEvent, SharedEventBridgeState};

pub struct EventBridgeService {
    state: SharedEventBridgeState,
    delivery: Arc<DeliveryBus>,
}

impl EventBridgeService {
    pub fn new(state: SharedEventBridgeState, delivery: Arc<DeliveryBus>) -> Self {
        Self { state, delivery }
    }
}

#[async_trait]
impl AwsService for EventBridgeService {
    fn service_name(&self) -> &str {
        "events"
    }

    async fn handle(&self, req: AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        match req.action.as_str() {
            "CreateEventBus" => self.create_event_bus(&req),
            "DeleteEventBus" => self.delete_event_bus(&req),
            "ListEventBuses" => self.list_event_buses(&req),
            "DescribeEventBus" => self.describe_event_bus(&req),
            "PutRule" => self.put_rule(&req),
            "DeleteRule" => self.delete_rule(&req),
            "ListRules" => self.list_rules(&req),
            "DescribeRule" => self.describe_rule(&req),
            "PutTargets" => self.put_targets(&req),
            "RemoveTargets" => self.remove_targets(&req),
            "ListTargetsByRule" => self.list_targets_by_rule(&req),
            "PutEvents" => self.put_events(&req),
            _ => Err(AwsServiceError::action_not_implemented(
                "events",
                &req.action,
            )),
        }
    }

    fn supported_actions(&self) -> &[&str] {
        &[
            "CreateEventBus",
            "DeleteEventBus",
            "ListEventBuses",
            "DescribeEventBus",
            "PutRule",
            "DeleteRule",
            "ListRules",
            "DescribeRule",
            "PutTargets",
            "RemoveTargets",
            "ListTargetsByRule",
            "PutEvents",
        ]
    }
}

fn parse_body(req: &AwsRequest) -> Value {
    serde_json::from_slice(&req.body).unwrap_or(Value::Object(Default::default()))
}

fn json_resp(body: Value) -> AwsResponse {
    AwsResponse::json(StatusCode::OK, serde_json::to_string(&body).unwrap())
}

impl EventBridgeService {
    fn create_event_bus(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"]
            .as_str()
            .ok_or_else(|| missing("Name"))?
            .to_string();

        let mut state = self.state.write();

        if state.buses.contains_key(&name) {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "ResourceAlreadyExistsException",
                format!("Event bus {name} already exists."),
            ));
        }

        let arn = format!(
            "arn:aws:events:{}:{}:event-bus/{}",
            state.region, state.account_id, name
        );
        let bus = EventBus {
            name: name.clone(),
            arn: arn.clone(),
        };
        state.buses.insert(name, bus);

        Ok(json_resp(json!({ "EventBusArn": arn })))
    }

    fn delete_event_bus(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"].as_str().ok_or_else(|| missing("Name"))?;

        if name == "default" {
            return Err(AwsServiceError::aws_error(
                StatusCode::BAD_REQUEST,
                "ValidationException",
                "Cannot delete the default event bus.",
            ));
        }

        let mut state = self.state.write();
        state.buses.remove(name);
        state.rules.retain(|_, r| r.event_bus_name != name);

        Ok(json_resp(json!({})))
    }

    fn list_event_buses(&self, _req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let state = self.state.read();
        let buses: Vec<Value> = state
            .buses
            .values()
            .map(|b| json!({ "Name": b.name, "Arn": b.arn }))
            .collect();

        Ok(json_resp(json!({ "EventBuses": buses })))
    }

    fn describe_event_bus(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"].as_str().unwrap_or("default");

        let state = self.state.read();
        let bus = state.buses.get(name).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("Event bus {name} does not exist."),
            )
        })?;

        Ok(json_resp(json!({
            "Name": bus.name,
            "Arn": bus.arn,
        })))
    }

    fn put_rule(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"]
            .as_str()
            .ok_or_else(|| missing("Name"))?
            .to_string();
        let event_bus_name = body["EventBusName"]
            .as_str()
            .unwrap_or("default")
            .to_string();
        let event_pattern = body["EventPattern"].as_str().map(|s| s.to_string());
        let description = body["Description"].as_str().map(|s| s.to_string());
        let rule_state = body["State"].as_str().unwrap_or("ENABLED").to_string();

        let mut state = self.state.write();

        if !state.buses.contains_key(&event_bus_name) {
            return Err(AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("Event bus {event_bus_name} does not exist."),
            ));
        }

        let arn = format!(
            "arn:aws:events:{}:{}:rule/{}/{}",
            state.region, state.account_id, event_bus_name, name
        );

        let targets = state
            .rules
            .get(&name)
            .map(|r| r.targets.clone())
            .unwrap_or_default();

        let rule = EventRule {
            name: name.clone(),
            arn: arn.clone(),
            event_bus_name,
            event_pattern,
            state: rule_state,
            description,
            targets,
        };

        state.rules.insert(name, rule);
        Ok(json_resp(json!({ "RuleArn": arn })))
    }

    fn delete_rule(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"].as_str().ok_or_else(|| missing("Name"))?;

        self.state.write().rules.remove(name);
        Ok(json_resp(json!({})))
    }

    fn list_rules(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let event_bus_name = body["EventBusName"].as_str().unwrap_or("default");

        let state = self.state.read();
        let rules: Vec<Value> = state
            .rules
            .values()
            .filter(|r| r.event_bus_name == event_bus_name)
            .map(|r| {
                json!({
                    "Name": r.name,
                    "Arn": r.arn,
                    "EventBusName": r.event_bus_name,
                    "State": r.state,
                    "Description": r.description,
                    "EventPattern": r.event_pattern,
                })
            })
            .collect();

        Ok(json_resp(json!({ "Rules": rules })))
    }

    fn describe_rule(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let name = body["Name"].as_str().ok_or_else(|| missing("Name"))?;

        let state = self.state.read();
        let rule = state.rules.get(name).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("Rule {name} does not exist."),
            )
        })?;

        Ok(json_resp(json!({
            "Name": rule.name,
            "Arn": rule.arn,
            "EventBusName": rule.event_bus_name,
            "State": rule.state,
            "Description": rule.description,
            "EventPattern": rule.event_pattern,
        })))
    }

    fn put_targets(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let rule_name = body["Rule"].as_str().ok_or_else(|| missing("Rule"))?;
        let targets = body["Targets"]
            .as_array()
            .ok_or_else(|| missing("Targets"))?;

        let mut state = self.state.write();
        let rule = state.rules.get_mut(rule_name).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("Rule {rule_name} does not exist."),
            )
        })?;

        for target in targets {
            let id = target["Id"].as_str().unwrap_or("").to_string();
            let arn = target["Arn"].as_str().unwrap_or("").to_string();
            // Remove existing target with same ID
            rule.targets.retain(|t| t.id != id);
            rule.targets.push(EventTarget { id, arn });
        }

        Ok(json_resp(json!({
            "FailedEntryCount": 0,
            "FailedEntries": [],
        })))
    }

    fn remove_targets(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let rule_name = body["Rule"].as_str().ok_or_else(|| missing("Rule"))?;
        let ids = body["Ids"].as_array().ok_or_else(|| missing("Ids"))?;

        let target_ids: Vec<String> = ids
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect();

        let mut state = self.state.write();
        let rule = state.rules.get_mut(rule_name).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("Rule {rule_name} does not exist."),
            )
        })?;

        rule.targets.retain(|t| !target_ids.contains(&t.id));

        Ok(json_resp(json!({
            "FailedEntryCount": 0,
            "FailedEntries": [],
        })))
    }

    fn list_targets_by_rule(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let rule_name = body["Rule"].as_str().ok_or_else(|| missing("Rule"))?;

        let state = self.state.read();
        let rule = state.rules.get(rule_name).ok_or_else(|| {
            AwsServiceError::aws_error(
                StatusCode::NOT_FOUND,
                "ResourceNotFoundException",
                format!("Rule {rule_name} does not exist."),
            )
        })?;

        let targets: Vec<Value> = rule
            .targets
            .iter()
            .map(|t| json!({ "Id": t.id, "Arn": t.arn }))
            .collect();

        Ok(json_resp(json!({ "Targets": targets })))
    }

    fn put_events(&self, req: &AwsRequest) -> Result<AwsResponse, AwsServiceError> {
        let body = parse_body(req);
        let entries = body["Entries"]
            .as_array()
            .ok_or_else(|| missing("Entries"))?;

        let mut state = self.state.write();
        let mut result_entries = Vec::new();
        let mut events_to_deliver = Vec::new();

        for entry in entries {
            let event_id = uuid::Uuid::new_v4().to_string();
            let source = entry["Source"].as_str().unwrap_or("").to_string();
            let detail_type = entry["DetailType"].as_str().unwrap_or("").to_string();
            let detail = entry["Detail"].as_str().unwrap_or("{}").to_string();
            let event_bus_name = entry["EventBusName"]
                .as_str()
                .unwrap_or("default")
                .to_string();

            let event = PutEvent {
                event_id: event_id.clone(),
                source: source.clone(),
                detail_type: detail_type.clone(),
                detail: detail.clone(),
                event_bus_name: event_bus_name.clone(),
                time: Utc::now(),
            };
            state.events.push(event);

            // Find matching rules and their targets
            let matching_targets: Vec<EventTarget> = state
                .rules
                .values()
                .filter(|r| {
                    r.event_bus_name == event_bus_name
                        && r.state == "ENABLED"
                        && matches_pattern(
                            r.event_pattern.as_deref(),
                            &source,
                            &detail_type,
                            &detail,
                        )
                })
                .flat_map(|r| r.targets.clone())
                .collect();

            if !matching_targets.is_empty() {
                events_to_deliver.push((
                    event_id.clone(),
                    source,
                    detail_type,
                    detail,
                    matching_targets,
                ));
            }

            result_entries.push(json!({ "EventId": event_id }));
        }

        // Drop the lock before delivering
        drop(state);

        // Deliver to targets
        for (event_id, source, detail_type, detail, targets) in events_to_deliver {
            let event_json = json!({
                "version": "0",
                "id": event_id,
                "source": source,
                "detail-type": detail_type,
                "detail": serde_json::from_str::<Value>(&detail).unwrap_or(json!({})),
                "time": Utc::now().to_rfc3339(),
                "region": "us-east-1",
            });
            let event_str = event_json.to_string();

            for target in targets {
                let arn = &target.arn;
                if arn.contains(":sqs:") {
                    self.delivery
                        .send_to_sqs(arn, &event_str, &std::collections::HashMap::new());
                } else if arn.contains(":sns:") {
                    self.delivery
                        .publish_to_sns(arn, &event_str, Some(&detail_type));
                }
            }
        }

        Ok(json_resp(json!({
            "FailedEntryCount": 0,
            "Entries": result_entries,
        })))
    }
}

/// Match an event against an EventBridge event pattern.
/// Supports matching on source, detail-type, and detail fields.
fn matches_pattern(
    pattern_json: Option<&str>,
    source: &str,
    detail_type: &str,
    detail: &str,
) -> bool {
    let pattern_json = match pattern_json {
        Some(p) => p,
        None => return true, // No pattern = match everything (schedule rules)
    };

    let pattern: Value = match serde_json::from_str(pattern_json) {
        Ok(v) => v,
        Err(_) => return false,
    };

    // Check "source" field
    if let Some(sources) = pattern.get("source").and_then(|v| v.as_array()) {
        if !sources.iter().any(|s| s.as_str() == Some(source)) {
            return false;
        }
    }

    // Check "detail-type" field
    if let Some(types) = pattern.get("detail-type").and_then(|v| v.as_array()) {
        if !types.iter().any(|t| t.as_str() == Some(detail_type)) {
            return false;
        }
    }

    // Check "detail" fields (simple top-level matching)
    if let Some(detail_pattern) = pattern.get("detail").and_then(|v| v.as_object()) {
        let detail_value: Value = serde_json::from_str(detail).unwrap_or(json!({}));
        for (key, expected_values) in detail_pattern {
            if let Some(expected_arr) = expected_values.as_array() {
                let actual = &detail_value[key];
                if !expected_arr.iter().any(|e| e == actual) {
                    return false;
                }
            }
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pattern_matches_source() {
        assert!(matches_pattern(
            Some(r#"{"source": ["my.app"]}"#),
            "my.app",
            "OrderPlaced",
            "{}"
        ));
        assert!(!matches_pattern(
            Some(r#"{"source": ["other.app"]}"#),
            "my.app",
            "OrderPlaced",
            "{}"
        ));
    }

    #[test]
    fn pattern_matches_detail_type() {
        assert!(matches_pattern(
            Some(r#"{"detail-type": ["OrderPlaced"]}"#),
            "my.app",
            "OrderPlaced",
            "{}"
        ));
        assert!(!matches_pattern(
            Some(r#"{"detail-type": ["OrderShipped"]}"#),
            "my.app",
            "OrderPlaced",
            "{}"
        ));
    }

    #[test]
    fn pattern_matches_detail_field() {
        assert!(matches_pattern(
            Some(r#"{"detail": {"status": ["ACTIVE"]}}"#),
            "my.app",
            "StatusChange",
            r#"{"status": "ACTIVE"}"#
        ));
        assert!(!matches_pattern(
            Some(r#"{"detail": {"status": ["ACTIVE"]}}"#),
            "my.app",
            "StatusChange",
            r#"{"status": "INACTIVE"}"#
        ));
    }

    #[test]
    fn no_pattern_matches_everything() {
        assert!(matches_pattern(None, "any", "any", "{}"));
    }

    #[test]
    fn combined_pattern() {
        let pattern = r#"{"source": ["orders"], "detail-type": ["OrderPlaced"]}"#;
        assert!(matches_pattern(
            Some(pattern),
            "orders",
            "OrderPlaced",
            "{}"
        ));
        assert!(!matches_pattern(
            Some(pattern),
            "orders",
            "OrderShipped",
            "{}"
        ));
        assert!(!matches_pattern(
            Some(pattern),
            "other",
            "OrderPlaced",
            "{}"
        ));
    }
}
