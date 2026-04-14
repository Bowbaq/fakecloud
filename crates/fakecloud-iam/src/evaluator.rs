//! Phase 1 IAM identity-policy evaluator.
//!
//! This module is a **pure function** over a set of policy documents and a
//! request: it does no I/O, no network, no state mutation, and never panics.
//! Dispatch (in batch 6) wires it up by collecting the principal's effective
//! policy set via [`collect_identity_policies`] and calling
//! [`evaluate`].
//!
//! # Phase 1 scope
//!
//! Implemented:
//! - `Effect: "Allow"` / `Effect: "Deny"` with **Deny precedence**: any
//!   matching `Deny` statement wins, regardless of how many `Allow`s match.
//! - `Action` / `NotAction` with `*` and `?` wildcards (case-insensitive
//!   service prefix match, case-sensitive action match — matches AWS).
//! - `Resource` / `NotResource` with `*` and `?` wildcards.
//! - Identity policies attached to users (inline + managed) and to groups
//!   the user belongs to.
//! - Identity policies attached to roles (inline + managed).
//! - Empty effective policy set → implicit deny.
//!
//! **Not** implemented (returns implicit deny rather than guessing — these
//! are tracked for Phase 2 and documented on `/docs/reference/security`):
//! - `Condition` blocks (StringEquals, IpAddress, DateLessThan, …)
//! - `NotPrincipal` (used in resource-based policies)
//! - Resource-based policies (S3 bucket policies, SNS topic policies,
//!   KMS key policies, Lambda resource policies, …)
//! - Permission boundaries
//! - Service control policies
//! - Session policies passed to `AssumeRole`
//! - ABAC / tag conditions
//!
//! Statements that contain a `Condition` block are **skipped during
//! evaluation** with a `tracing::debug!` so users running in `soft` mode
//! can see which policies didn't get considered.

use std::collections::HashSet;

use fakecloud_core::auth::{Principal, PrincipalType};
use serde_json::Value;

use crate::state::IamState;

/// The result of evaluating a request against a set of policies.
///
/// `Allow` requires at least one matching `Allow` statement and zero
/// matching `Deny` statements. `ExplicitDeny` indicates at least one
/// matching `Deny` statement (which takes precedence over any `Allow`).
/// `ImplicitDeny` is the catch-all for "no policy spoke to this request".
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Allow,
    ImplicitDeny,
    ExplicitDeny,
}

impl Decision {
    /// Returns true if the request should be allowed.
    pub fn is_allow(self) -> bool {
        matches!(self, Decision::Allow)
    }
}

/// One IAM action to evaluate against a policy set.
///
/// `action` follows the canonical `service:Action` shape (e.g.
/// `s3:GetObject`, `sqs:SendMessage`). `resource` is a fully-qualified
/// AWS ARN; the per-service resource extractors in batches 6-8 produce
/// these.
///
/// `_context` is reserved for Phase 2 condition-key plumbing and is
/// currently unused — present as a field so the evaluator's call sites
/// don't need to change when conditions land.
#[derive(Debug, Clone)]
pub struct EvalRequest<'a> {
    pub principal: &'a Principal,
    pub action: String,
    pub resource: String,
    pub context: RequestContext,
}

/// Request-time context keys that Phase 2 will use for `Condition`
/// evaluation. Currently empty so the evaluator API doesn't need a
/// breaking change later.
#[derive(Debug, Clone, Default)]
pub struct RequestContext {}

/// Parsed view of a single statement within a policy document.
#[derive(Debug, Clone)]
pub(crate) struct ParsedStatement {
    pub effect: Effect,
    pub action: ActionMatch,
    pub resource: ResourceMatch,
    /// Whether this statement carried a `Condition` block. Phase 1 cannot
    /// evaluate conditions, so any conditioned statement is skipped during
    /// evaluation rather than silently treated as unconditioned.
    pub has_condition: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Effect {
    Allow,
    Deny,
}

/// Action / NotAction patterns. `Allow` lists are positive matches;
/// `Deny` lists are negative matches (NotAction).
#[derive(Debug, Clone)]
pub(crate) enum ActionMatch {
    Action(Vec<String>),
    NotAction(Vec<String>),
}

/// Resource / NotResource patterns.
#[derive(Debug, Clone)]
pub(crate) enum ResourceMatch {
    Resource(Vec<String>),
    NotResource(Vec<String>),
    /// Statement omitted both `Resource` and `NotResource`. AWS treats
    /// this as "applies to all resources" only inside trust policies; for
    /// identity policies it's a validation error. We treat missing as
    /// wildcard-all to match how some Terraform-generated policies look
    /// in practice, but the evaluator never silently grants more than
    /// the policy text actually says — this maps to the same behavior
    /// as `Resource: ["*"]`.
    Implicit,
}

/// Parsed policy document — only the fields the evaluator needs. Any
/// statement that fails to parse (wrong shape, unknown effect, etc.) is
/// dropped with a warn-level log and the rest of the document is still
/// usable, matching how AWS behaves with invalid statements (the broken
/// statement is ignored, not the whole policy).
#[derive(Debug, Clone, Default)]
pub struct PolicyDocument {
    pub(crate) statements: Vec<ParsedStatement>,
}

impl PolicyDocument {
    /// Parse a policy document from its JSON string form. Returns an
    /// empty document on JSON errors so the caller can fall through to
    /// implicit-deny rather than panicking on malformed state.
    pub fn parse(json: &str) -> Self {
        let value: Value = match serde_json::from_str(json) {
            Ok(v) => v,
            Err(e) => {
                tracing::warn!(error = %e, "failed to parse policy document JSON; ignoring");
                return Self::default();
            }
        };
        Self::from_value(&value)
    }

    /// Parse a policy document from a `serde_json::Value`. Used by both
    /// [`PolicyDocument::parse`] and tests that build inline `serde_json!`
    /// values.
    pub fn from_value(value: &Value) -> Self {
        let statements = match value.get("Statement") {
            Some(Value::Array(arr)) => arr.iter().filter_map(parse_statement).collect::<Vec<_>>(),
            Some(obj @ Value::Object(_)) => parse_statement(obj).into_iter().collect(),
            _ => Vec::new(),
        };
        Self { statements }
    }

    /// Number of parsed statements in this document. Used by tests as a
    /// proxy for "did this statement parse successfully?" without exposing
    /// the internal representation.
    pub fn statement_count(&self) -> usize {
        self.statements.len()
    }
}

fn parse_statement(value: &Value) -> Option<ParsedStatement> {
    let obj = value.as_object()?;
    let effect = match obj.get("Effect")?.as_str()? {
        "Allow" => Effect::Allow,
        "Deny" => Effect::Deny,
        other => {
            tracing::warn!(effect = other, "unknown Effect; ignoring statement");
            return None;
        }
    };
    let action = if let Some(a) = obj.get("Action") {
        ActionMatch::Action(coerce_string_list(a))
    } else if let Some(na) = obj.get("NotAction") {
        ActionMatch::NotAction(coerce_string_list(na))
    } else {
        tracing::warn!("statement has no Action or NotAction; ignoring");
        return None;
    };
    let resource = if let Some(r) = obj.get("Resource") {
        ResourceMatch::Resource(coerce_string_list(r))
    } else if let Some(nr) = obj.get("NotResource") {
        ResourceMatch::NotResource(coerce_string_list(nr))
    } else {
        ResourceMatch::Implicit
    };
    let has_condition = obj.contains_key("Condition");
    Some(ParsedStatement {
        effect,
        action,
        resource,
        has_condition,
    })
}

/// Coerce a JSON value into a list of strings. AWS policy schema accepts
/// either a single string or an array of strings for `Action`/`Resource`.
/// Non-string entries are dropped.
fn coerce_string_list(value: &Value) -> Vec<String> {
    match value {
        Value::String(s) => vec![s.clone()],
        Value::Array(arr) => arr
            .iter()
            .filter_map(|v| v.as_str().map(|s| s.to_string()))
            .collect(),
        _ => Vec::new(),
    }
}

/// Evaluate a request against a set of policy documents.
///
/// Implements AWS's standard identity-policy evaluation logic for Phase 1
/// features only. See the module-level docstring for the exhaustive list
/// of what is and isn't covered.
///
/// # Algorithm
///
/// 1. Walk every statement in every policy.
/// 2. Skip any statement that has a `Condition` block (Phase 2).
/// 3. For each statement that matches the request's action *and* resource:
///    - If `Effect: Deny` → return [`Decision::ExplicitDeny`] immediately.
///    - If `Effect: Allow` → record that we saw an allow.
/// 4. After all statements are scanned: return [`Decision::Allow`] if any
///    allow matched, otherwise [`Decision::ImplicitDeny`].
pub fn evaluate(policies: &[PolicyDocument], request: &EvalRequest<'_>) -> Decision {
    let mut allowed = false;
    for policy in policies {
        for statement in &policy.statements {
            if statement.has_condition {
                tracing::debug!(
                    target: "fakecloud::iam::audit",
                    action = %request.action,
                    "skipping statement with Condition (not yet evaluated in Phase 1)"
                );
                continue;
            }
            if !action_matches(&statement.action, &request.action) {
                continue;
            }
            if !resource_matches(&statement.resource, &request.resource) {
                continue;
            }
            match statement.effect {
                Effect::Deny => return Decision::ExplicitDeny,
                Effect::Allow => allowed = true,
            }
        }
    }
    if allowed {
        Decision::Allow
    } else {
        Decision::ImplicitDeny
    }
}

fn action_matches(action: &ActionMatch, request_action: &str) -> bool {
    match action {
        ActionMatch::Action(patterns) => patterns
            .iter()
            .any(|p| iam_glob_match(p, request_action, true)),
        ActionMatch::NotAction(patterns) => patterns
            .iter()
            .all(|p| !iam_glob_match(p, request_action, true)),
    }
}

fn resource_matches(resource: &ResourceMatch, request_resource: &str) -> bool {
    match resource {
        ResourceMatch::Resource(patterns) => patterns
            .iter()
            .any(|p| iam_glob_match(p, request_resource, false)),
        ResourceMatch::NotResource(patterns) => patterns
            .iter()
            .all(|p| !iam_glob_match(p, request_resource, false)),
        ResourceMatch::Implicit => true,
    }
}

/// IAM-style glob match supporting `*` (any sequence) and `?` (single
/// character). When `case_insensitive_service_prefix` is true and the
/// pattern looks like an action (`service:Action`), the service prefix is
/// matched case-insensitively while the action name is matched as-is —
/// matches how AWS evaluates Action patterns.
fn iam_glob_match(pattern: &str, value: &str, case_insensitive_service_prefix: bool) -> bool {
    if case_insensitive_service_prefix {
        if let (Some((p_svc, p_act)), Some((v_svc, v_act))) =
            (pattern.split_once(':'), value.split_once(':'))
        {
            if !glob_match(&p_svc.to_ascii_lowercase(), &v_svc.to_ascii_lowercase()) {
                return false;
            }
            return glob_match(p_act, v_act);
        }
    }
    glob_match(pattern, value)
}

/// Plain glob matcher with `*` (zero or more) and `?` (exactly one).
/// Iterative two-pointer implementation — runs in `O(pattern.len() *
/// value.len())` worst case, no backtracking explosions.
fn glob_match(pattern: &str, value: &str) -> bool {
    let p: Vec<char> = pattern.chars().collect();
    let v: Vec<char> = value.chars().collect();
    let mut pi = 0usize;
    let mut vi = 0usize;
    let mut star: Option<usize> = None;
    let mut star_v: usize = 0;
    while vi < v.len() {
        if pi < p.len() && (p[pi] == '?' || p[pi] == v[vi]) {
            pi += 1;
            vi += 1;
        } else if pi < p.len() && p[pi] == '*' {
            star = Some(pi);
            star_v = vi;
            pi += 1;
        } else if let Some(s) = star {
            pi = s + 1;
            star_v += 1;
            vi = star_v;
        } else {
            return false;
        }
    }
    while pi < p.len() && p[pi] == '*' {
        pi += 1;
    }
    pi == p.len()
}

/// Collect every identity policy that should be considered when
/// evaluating a request from `principal`.
///
/// Phase 1 walks identity policies only (user inline + managed, group
/// inline + managed via membership, role inline + managed). Resource
/// policies, permission boundaries, and SCPs are not consulted —
/// see the module-level scope notes.
///
/// The returned vector is the **deduplicated** set of policy documents,
/// parsed and ready to feed into [`evaluate`]. Unknown managed policy
/// ARNs are skipped with a debug log.
pub fn collect_identity_policies(state: &IamState, principal: &Principal) -> Vec<PolicyDocument> {
    let mut docs = Vec::new();
    let mut seen_managed: HashSet<String> = HashSet::new();
    match principal.principal_type {
        PrincipalType::User => {
            if let Some(user_name) = user_name_from_arn(&principal.arn) {
                collect_user_policies(state, user_name, &mut docs, &mut seen_managed);
            }
        }
        PrincipalType::AssumedRole => {
            if let Some(role_name) = role_name_from_assumed_role_arn(&principal.arn) {
                collect_role_policies(state, role_name, &mut docs, &mut seen_managed);
            }
        }
        PrincipalType::Root => {
            // Root bypasses evaluation; the caller (dispatch) should
            // short-circuit via `Principal::is_root` before reaching here.
            // Returning an empty vec means an explicit `Allow` is required,
            // which is the safe default if a caller forgets to bypass.
        }
        PrincipalType::FederatedUser | PrincipalType::Unknown => {
            // No identity-policy story for these in Phase 1.
        }
    }
    docs
}

fn collect_user_policies(
    state: &IamState,
    user_name: &str,
    docs: &mut Vec<PolicyDocument>,
    seen_managed: &mut HashSet<String>,
) {
    if let Some(inline) = state.user_inline_policies.get(user_name) {
        for doc in inline.values() {
            docs.push(PolicyDocument::parse(doc));
        }
    }
    if let Some(arns) = state.user_policies.get(user_name) {
        for arn in arns {
            if !seen_managed.insert(arn.clone()) {
                continue;
            }
            if let Some(doc) = managed_policy_default_document(state, arn) {
                docs.push(PolicyDocument::parse(&doc));
            }
        }
    }
    // Group memberships: walk every group whose members include the user.
    for (group_name, group) in &state.groups {
        if !group.members.iter().any(|m| m == user_name) {
            continue;
        }
        for doc in group.inline_policies.values() {
            docs.push(PolicyDocument::parse(doc));
        }
        for arn in &group.attached_policies {
            if !seen_managed.insert(arn.clone()) {
                continue;
            }
            if let Some(doc) = managed_policy_default_document(state, arn) {
                docs.push(PolicyDocument::parse(&doc));
            }
        }
        let _ = group_name;
    }
}

fn collect_role_policies(
    state: &IamState,
    role_name: &str,
    docs: &mut Vec<PolicyDocument>,
    seen_managed: &mut HashSet<String>,
) {
    if let Some(inline) = state.role_inline_policies.get(role_name) {
        for doc in inline.values() {
            docs.push(PolicyDocument::parse(doc));
        }
    }
    if let Some(arns) = state.role_policies.get(role_name) {
        for arn in arns {
            if !seen_managed.insert(arn.clone()) {
                continue;
            }
            if let Some(doc) = managed_policy_default_document(state, arn) {
                docs.push(PolicyDocument::parse(&doc));
            }
        }
    }
}

fn managed_policy_default_document(state: &IamState, arn: &str) -> Option<String> {
    let policy = state.policies.get(arn)?;
    policy
        .versions
        .iter()
        .find(|v| v.is_default)
        .or_else(|| policy.versions.first())
        .map(|v| v.document.clone())
}

fn user_name_from_arn(arn: &str) -> Option<&str> {
    arn.rsplit_once(":user/").map(|(_, name)| name)
}

fn role_name_from_assumed_role_arn(arn: &str) -> Option<&str> {
    // `arn:aws:sts::<account>:assumed-role/<role-name>/<session>`
    let after = arn.rsplit_once(":assumed-role/")?.1;
    Some(after.split('/').next().unwrap_or(after))
}

#[cfg(test)]
#[allow(clippy::cloned_ref_to_slice_refs)]
mod tests {
    use super::*;
    use serde_json::json;

    fn principal_user(arn: &str) -> Principal {
        Principal {
            arn: arn.to_string(),
            user_id: "AIDA".into(),
            account_id: "123456789012".into(),
            principal_type: PrincipalType::User,
            source_identity: None,
        }
    }

    fn req<'a>(principal: &'a Principal, action: &str, resource: &str) -> EvalRequest<'a> {
        EvalRequest {
            principal,
            action: action.to_string(),
            resource: resource.to_string(),
            context: RequestContext::default(),
        }
    }

    fn doc(json: serde_json::Value) -> PolicyDocument {
        PolicyDocument::from_value(&json)
    }

    // --- glob_match -----------------------------------------------------

    #[test]
    fn glob_literal_match() {
        assert!(glob_match("foo", "foo"));
        assert!(!glob_match("foo", "bar"));
    }

    #[test]
    fn glob_star_matches_any() {
        assert!(glob_match("*", "foo"));
        assert!(glob_match("*", ""));
        assert!(glob_match("foo*", "foobar"));
        assert!(glob_match("*bar", "foobar"));
        assert!(glob_match("f*r", "foobar"));
        assert!(!glob_match("foo*", "fo"));
    }

    #[test]
    fn glob_question_mark_matches_one() {
        assert!(glob_match("f?o", "foo"));
        assert!(!glob_match("f?o", "fo"));
        assert!(!glob_match("f?o", "foo!"));
    }

    #[test]
    fn glob_no_backtracking_explosion() {
        // Pattern that would blow up a naive recursive matcher.
        assert!(!glob_match("a*a*a*a*a*b", "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"));
    }

    // --- iam_glob_match (action specifics) ------------------------------

    #[test]
    fn iam_action_service_prefix_is_case_insensitive() {
        assert!(iam_glob_match("S3:GetObject", "s3:GetObject", true));
        assert!(iam_glob_match("s3:GetObject", "S3:GetObject", true));
    }

    #[test]
    fn iam_action_name_is_case_sensitive() {
        // Action name is case-sensitive in AWS.
        assert!(!iam_glob_match("s3:getobject", "s3:GetObject", true));
        assert!(iam_glob_match("s3:GetObject", "s3:GetObject", true));
    }

    #[test]
    fn iam_action_supports_wildcards() {
        assert!(iam_glob_match("s3:Get*", "s3:GetObject", true));
        assert!(iam_glob_match("s3:*", "s3:DeleteObject", true));
        assert!(iam_glob_match("*", "s3:GetObject", true));
        assert!(!iam_glob_match("s3:Get*", "s3:PutObject", true));
    }

    // --- evaluate -------------------------------------------------------

    #[test]
    fn empty_policy_set_is_implicit_deny() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        assert_eq!(
            evaluate(&[], &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")),
            Decision::ImplicitDeny
        );
    }

    #[test]
    fn allow_with_matching_action_and_resource() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Version": "2012-10-17",
            "Statement": [{
                "Effect": "Allow",
                "Action": "s3:GetObject",
                "Resource": "arn:aws:s3:::bucket/key"
            }]
        }));
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::Allow
        );
    }

    #[test]
    fn deny_takes_precedence_over_allow() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let allow = doc(json!({
            "Statement": [{
                "Effect": "Allow",
                "Action": "*",
                "Resource": "*"
            }]
        }));
        let deny = doc(json!({
            "Statement": [{
                "Effect": "Deny",
                "Action": "s3:DeleteObject",
                "Resource": "*"
            }]
        }));
        assert_eq!(
            evaluate(
                &[allow.clone(), deny.clone()],
                &req(&p, "s3:DeleteObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::ExplicitDeny
        );
        // Order doesn't matter — Deny still wins when listed first.
        assert_eq!(
            evaluate(
                &[deny, allow],
                &req(&p, "s3:DeleteObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::ExplicitDeny
        );
    }

    #[test]
    fn allow_with_wrong_action_is_implicit_deny() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Statement": [{
                "Effect": "Allow",
                "Action": "s3:GetObject",
                "Resource": "*"
            }]
        }));
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:DeleteObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::ImplicitDeny
        );
    }

    #[test]
    fn allow_with_wrong_resource_is_implicit_deny() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Statement": [{
                "Effect": "Allow",
                "Action": "s3:GetObject",
                "Resource": "arn:aws:s3:::other-bucket/*"
            }]
        }));
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::ImplicitDeny
        );
    }

    #[test]
    fn resource_wildcard_matches_arn_path() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Statement": [{
                "Effect": "Allow",
                "Action": "s3:GetObject",
                "Resource": "arn:aws:s3:::bucket/*"
            }]
        }));
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/path/to/key")
            ),
            Decision::Allow
        );
    }

    #[test]
    fn not_action_excludes_listed_actions() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Statement": [{
                "Effect": "Allow",
                "NotAction": "s3:DeleteObject",
                "Resource": "*"
            }]
        }));
        // Allowed because GetObject is not in NotAction.
        assert_eq!(
            evaluate(
                &[policy.clone()],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::Allow
        );
        // Implicit-denied because DeleteObject is in NotAction (no allow matches).
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:DeleteObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::ImplicitDeny
        );
    }

    #[test]
    fn not_resource_excludes_listed_resources() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Statement": [{
                "Effect": "Allow",
                "Action": "s3:GetObject",
                "NotResource": "arn:aws:s3:::secret-bucket/*"
            }]
        }));
        assert_eq!(
            evaluate(
                &[policy.clone()],
                &req(&p, "s3:GetObject", "arn:aws:s3:::public-bucket/key")
            ),
            Decision::Allow
        );
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:GetObject", "arn:aws:s3:::secret-bucket/key")
            ),
            Decision::ImplicitDeny
        );
    }

    #[test]
    fn statement_with_condition_is_skipped_in_phase1() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Statement": [{
                "Effect": "Allow",
                "Action": "*",
                "Resource": "*",
                "Condition": {
                    "StringEquals": { "aws:username": "alice" }
                }
            }]
        }));
        // Phase 1 doesn't evaluate Condition, so the statement is skipped
        // and we fall through to implicit deny — safer than guessing.
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::ImplicitDeny
        );
    }

    #[test]
    fn deny_with_condition_does_not_stop_an_otherwise_allowed_request() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        // A Deny-with-Condition is also skipped (we can't tell if the
        // condition would have matched). The Allow that follows still
        // grants the request.
        let policy = doc(json!({
            "Statement": [
                {
                    "Effect": "Deny",
                    "Action": "*",
                    "Resource": "*",
                    "Condition": { "Bool": { "aws:MultiFactorAuthPresent": "false" } }
                },
                {
                    "Effect": "Allow",
                    "Action": "s3:GetObject",
                    "Resource": "*"
                }
            ]
        }));
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::Allow
        );
    }

    #[test]
    fn array_action_matches_any_entry() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Statement": [{
                "Effect": "Allow",
                "Action": ["s3:GetObject", "s3:PutObject"],
                "Resource": "*"
            }]
        }));
        assert_eq!(
            evaluate(
                &[policy.clone()],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::Allow
        );
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:PutObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::Allow
        );
    }

    #[test]
    fn statement_without_effect_is_dropped() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Statement": [
                { "Action": "s3:GetObject", "Resource": "*" },
                { "Effect": "Allow", "Action": "s3:GetObject", "Resource": "*" }
            ]
        }));
        // The dropped statement doesn't contribute, but the second
        // valid one still grants the request.
        assert_eq!(policy.statement_count(), 1);
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::Allow
        );
    }

    #[test]
    fn statement_without_action_is_dropped() {
        let policy = doc(json!({
            "Statement": [{ "Effect": "Allow", "Resource": "*" }]
        }));
        assert_eq!(policy.statement_count(), 0);
    }

    #[test]
    fn implicit_resource_acts_like_wildcard() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Statement": [{ "Effect": "Allow", "Action": "s3:GetObject" }]
        }));
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::Allow
        );
    }

    #[test]
    fn malformed_policy_json_is_implicit_deny() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = PolicyDocument::parse("{ this is not valid json");
        assert_eq!(policy.statement_count(), 0);
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::ImplicitDeny
        );
    }

    #[test]
    fn deny_short_circuits_after_match() {
        let p = principal_user("arn:aws:iam::123456789012:user/alice");
        let policy = doc(json!({
            "Statement": [
                { "Effect": "Deny", "Action": "*", "Resource": "*" },
                { "Effect": "Allow", "Action": "s3:GetObject", "Resource": "*" }
            ]
        }));
        assert_eq!(
            evaluate(
                &[policy],
                &req(&p, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::ExplicitDeny
        );
    }

    #[test]
    fn user_name_from_arn_extracts_trailing_segment() {
        assert_eq!(
            user_name_from_arn("arn:aws:iam::123456789012:user/alice"),
            Some("alice")
        );
        assert_eq!(
            user_name_from_arn("arn:aws:iam::123456789012:user/path/to/alice"),
            Some("path/to/alice")
        );
        assert_eq!(user_name_from_arn("arn:aws:iam::123456789012:role/r"), None);
    }

    #[test]
    fn role_name_from_assumed_role_arn_strips_session() {
        assert_eq!(
            role_name_from_assumed_role_arn("arn:aws:sts::123456789012:assumed-role/ops/session-1"),
            Some("ops")
        );
    }

    // --- collect_identity_policies --------------------------------------

    #[test]
    fn collect_identity_policies_picks_up_user_inline() {
        use crate::state::IamUser;
        use chrono::Utc;
        let mut state = IamState::new("123456789012");
        state.users.insert(
            "alice".to_string(),
            IamUser {
                user_name: "alice".into(),
                user_id: "AIDAALICE".into(),
                arn: "arn:aws:iam::123456789012:user/alice".into(),
                path: "/".into(),
                created_at: Utc::now(),
                tags: Vec::new(),
                permissions_boundary: None,
            },
        );
        let mut inline = std::collections::HashMap::new();
        inline.insert(
            "AllowGet".to_string(),
            r#"{"Statement":[{"Effect":"Allow","Action":"s3:GetObject","Resource":"*"}]}"#
                .to_string(),
        );
        state
            .user_inline_policies
            .insert("alice".to_string(), inline);

        let principal = principal_user("arn:aws:iam::123456789012:user/alice");
        let docs = collect_identity_policies(&state, &principal);
        assert_eq!(docs.len(), 1);
        assert_eq!(
            evaluate(
                &docs,
                &req(&principal, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::Allow
        );
    }

    #[test]
    fn collect_identity_policies_picks_up_managed_via_groups() {
        use crate::state::{IamGroup, IamPolicy, IamUser, PolicyVersion};
        use chrono::Utc;
        let mut state = IamState::new("123456789012");
        state.users.insert(
            "alice".to_string(),
            IamUser {
                user_name: "alice".into(),
                user_id: "AIDAALICE".into(),
                arn: "arn:aws:iam::123456789012:user/alice".into(),
                path: "/".into(),
                created_at: Utc::now(),
                tags: Vec::new(),
                permissions_boundary: None,
            },
        );
        let policy_arn = "arn:aws:iam::123456789012:policy/AllowGet".to_string();
        state.policies.insert(
            policy_arn.clone(),
            IamPolicy {
                policy_name: "AllowGet".into(),
                policy_id: "ANPA1".into(),
                arn: policy_arn.clone(),
                path: "/".into(),
                description: "".into(),
                created_at: Utc::now(),
                tags: Vec::new(),
                default_version_id: "v1".into(),
                versions: vec![PolicyVersion {
                    version_id: "v1".into(),
                    document: r#"{"Statement":[{"Effect":"Allow","Action":"s3:GetObject","Resource":"*"}]}"#.into(),
                    is_default: true,
                    created_at: Utc::now(),
                }],
                next_version_num: 2,
                attachment_count: 1,
            },
        );
        state.groups.insert(
            "readers".to_string(),
            IamGroup {
                group_name: "readers".into(),
                group_id: "AGPA1".into(),
                arn: "arn:aws:iam::123456789012:group/readers".into(),
                path: "/".into(),
                created_at: Utc::now(),
                members: vec!["alice".into()],
                inline_policies: std::collections::HashMap::new(),
                attached_policies: vec![policy_arn],
            },
        );
        let principal = principal_user("arn:aws:iam::123456789012:user/alice");
        let docs = collect_identity_policies(&state, &principal);
        assert_eq!(docs.len(), 1);
        assert_eq!(
            evaluate(
                &docs,
                &req(&principal, "s3:GetObject", "arn:aws:s3:::bucket/key")
            ),
            Decision::Allow
        );
    }

    #[test]
    fn collect_identity_policies_for_root_returns_empty() {
        let state = IamState::new("123456789012");
        let principal = Principal {
            arn: "arn:aws:iam::123456789012:root".into(),
            user_id: "ROOT".into(),
            account_id: "123456789012".into(),
            principal_type: PrincipalType::Root,
            source_identity: None,
        };
        // Root short-circuits via Principal::is_root in dispatch; here we
        // just assert collect_identity_policies doesn't synthesize a
        // wildcard allow on its behalf.
        assert!(collect_identity_policies(&state, &principal).is_empty());
    }
}
