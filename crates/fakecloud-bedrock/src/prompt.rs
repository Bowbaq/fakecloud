use serde_json::Value;

use crate::state::{ResponseRule, SharedBedrockState};

/// Extract the user-visible prompt text from a runtime request body.
///
/// Handles both InvokeModel (provider-specific shapes) and Converse bodies.
/// Returns an empty string when nothing recognizable is found, so a rule
/// with `prompt_contains = ""` matches any call.
pub fn extract_prompt_text(model_id: &str, body: &[u8]) -> String {
    let Ok(value): Result<Value, _> = serde_json::from_slice(body) else {
        return String::new();
    };

    // Converse shape: top-level `messages` array with `content[].text`,
    // plus optional `system[].text`.
    let mut out = String::new();
    if let Some(system) = value.get("system").and_then(|s| s.as_array()) {
        for block in system {
            if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                out.push_str(t);
                out.push(' ');
            }
        }
    }
    if let Some(messages) = value.get("messages").and_then(|m| m.as_array()) {
        for msg in messages {
            match msg.get("content") {
                // Converse: content is an array of blocks.
                Some(Value::Array(blocks)) => {
                    for block in blocks {
                        if let Some(t) = block.get("text").and_then(|t| t.as_str()) {
                            out.push_str(t);
                            out.push(' ');
                        }
                    }
                }
                // Anthropic InvokeModel: content may be a plain string.
                Some(Value::String(s)) => {
                    out.push_str(s);
                    out.push(' ');
                }
                _ => {}
            }
        }
    }
    if !out.is_empty() {
        return out.trim_end().to_string();
    }

    // Provider-specific InvokeModel shapes.
    if model_id.starts_with("amazon.") {
        if let Some(t) = value.get("inputText").and_then(|t| t.as_str()) {
            return t.to_string();
        }
    }
    if let Some(t) = value.get("prompt").and_then(|t| t.as_str()) {
        return t.to_string();
    }
    if let Some(t) = value.get("inputText").and_then(|t| t.as_str()) {
        return t.to_string();
    }

    String::new()
}

/// Return the first rule whose `prompt_contains` filter matches the current prompt.
/// A rule with `prompt_contains = None` or an empty string matches anything.
pub fn match_rule<'a>(rules: &'a [ResponseRule], prompt: &str) -> Option<&'a ResponseRule> {
    rules.iter().find(|rule| match &rule.prompt_contains {
        None => true,
        Some(needle) if needle.is_empty() => true,
        Some(needle) => prompt.contains(needle.as_str()),
    })
}

/// Resolve the response body a runtime call should use, applying
/// rule-based overrides first, then the legacy single-response override.
/// Returns `None` when neither is configured — caller falls back to canned.
pub fn resolve_override(state: &SharedBedrockState, model_id: &str, body: &[u8]) -> Option<String> {
    let prompt = extract_prompt_text(model_id, body);
    let s = state.read();
    if let Some(rules) = s.response_rules.get(model_id) {
        if let Some(rule) = match_rule(rules, &prompt) {
            return Some(rule.response.clone());
        }
    }
    s.custom_responses.get(model_id).cloned()
}
