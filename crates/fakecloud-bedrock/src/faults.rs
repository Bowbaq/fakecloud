use chrono::Utc;
use http::StatusCode;

use fakecloud_core::service::AwsServiceError;

use crate::state::{FaultRule, ModelInvocation, SharedBedrockState};

/// Pop the first queued fault rule that matches the given `(model_id, operation)`
/// pair. Decrements the rule's remaining count; removes the rule when it hits
/// zero. Returns the rule as it looked *before* decrementing so callers can see
/// the intended error type/message/status.
pub fn take_matching_fault(
    state: &SharedBedrockState,
    model_id: &str,
    operation: &str,
) -> Option<FaultRule> {
    let mut s = state.write();
    let idx = s.fault_rules.iter().position(|rule| {
        rule.model_id
            .as_deref()
            .is_none_or(|needle| needle == model_id)
            && rule
                .operation
                .as_deref()
                .is_none_or(|needle| needle == operation)
    })?;
    let snapshot = s.fault_rules[idx].clone();
    if s.fault_rules[idx].remaining <= 1 {
        s.fault_rules.remove(idx);
    } else {
        s.fault_rules[idx].remaining -= 1;
    }
    Some(snapshot)
}

/// Convert a queued fault rule into an `AwsServiceError` for the caller to return.
pub fn fault_to_error(fault: &FaultRule) -> AwsServiceError {
    let status = StatusCode::from_u16(fault.http_status).unwrap_or(StatusCode::BAD_REQUEST);
    AwsServiceError::aws_error(status, &fault.error_type, &fault.message)
}

/// Record an invocation that was rejected by an injected fault.
pub fn record_faulted_invocation(
    state: &SharedBedrockState,
    model_id: &str,
    body: &[u8],
    fault: &FaultRule,
) {
    let mut s = state.write();
    s.invocations.push(ModelInvocation {
        model_id: model_id.to_string(),
        input: String::from_utf8_lossy(body).to_string(),
        output: String::new(),
        timestamp: Utc::now(),
        error: Some(format!("{}: {}", fault.error_type, fault.message)),
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::BedrockState;
    use parking_lot::RwLock;
    use std::sync::Arc;

    fn shared() -> SharedBedrockState {
        Arc::new(RwLock::new(BedrockState::new("123456789012", "us-east-1")))
    }

    fn rule(
        error_type: &str,
        message: &str,
        http_status: u16,
        remaining: u32,
        model_id: Option<&str>,
        operation: Option<&str>,
    ) -> FaultRule {
        FaultRule {
            error_type: error_type.to_string(),
            message: message.to_string(),
            http_status,
            remaining,
            model_id: model_id.map(|s| s.to_string()),
            operation: operation.map(|s| s.to_string()),
        }
    }

    #[test]
    fn take_matching_fault_none_when_empty() {
        let s = shared();
        assert!(take_matching_fault(&s, "model-x", "InvokeModel").is_none());
    }

    #[test]
    fn take_matching_fault_matches_wildcard_rule() {
        let s = shared();
        s.write()
            .fault_rules
            .push(rule("Throttle", "slow down", 429, 3, None, None));
        let hit = take_matching_fault(&s, "any-model", "InvokeModel").unwrap();
        assert_eq!(hit.error_type, "Throttle");
        assert_eq!(s.read().fault_rules[0].remaining, 2);
    }

    #[test]
    fn take_matching_fault_removes_when_remaining_reaches_zero() {
        let s = shared();
        s.write()
            .fault_rules
            .push(rule("Err", "boom", 500, 1, None, None));
        assert!(take_matching_fault(&s, "m", "o").is_some());
        assert!(s.read().fault_rules.is_empty());
    }

    #[test]
    fn take_matching_fault_scoped_by_model() {
        let s = shared();
        s.write()
            .fault_rules
            .push(rule("ModelErr", "fail", 500, 1, Some("target-model"), None));
        assert!(take_matching_fault(&s, "other-model", "o").is_none());
        assert!(take_matching_fault(&s, "target-model", "o").is_some());
    }

    #[test]
    fn take_matching_fault_scoped_by_operation() {
        let s = shared();
        s.write()
            .fault_rules
            .push(rule("OpErr", "fail", 500, 1, None, Some("Converse")));
        assert!(take_matching_fault(&s, "m", "InvokeModel").is_none());
        assert!(take_matching_fault(&s, "m", "Converse").is_some());
    }

    #[test]
    fn fault_to_error_preserves_status() {
        let r = rule("Throttled", "slow", 429, 1, None, None);
        let err = fault_to_error(&r);
        assert_eq!(err.status(), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn fault_to_error_invalid_status_defaults_bad_request() {
        let r = rule("Weird", "x", 9999, 1, None, None);
        let err = fault_to_error(&r);
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
    }

    #[test]
    fn record_faulted_invocation_appends_error_entry() {
        let s = shared();
        let r = rule("Throttled", "slow", 429, 1, None, None);
        record_faulted_invocation(&s, "m-1", b"input-body", &r);
        let inv = &s.read().invocations[0];
        assert_eq!(inv.model_id, "m-1");
        assert_eq!(inv.input, "input-body");
        assert_eq!(inv.output, "");
        assert_eq!(inv.error.as_deref(), Some("Throttled: slow"));
    }
}
