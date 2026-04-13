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
