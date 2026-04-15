//! Adapter that implements [`fakecloud_core::auth::IamPolicyEvaluator`]
//! over the shared IAM state + Phase 1 evaluator.
//!
//! Mirrors the shape of [`crate::credential_resolver`]: the trait lives in
//! `fakecloud-core`, the concrete implementation lives here, and dispatch
//! calls through the trait so the dependency edge points core -> iam only.

use std::sync::Arc;

use fakecloud_core::auth::{
    ConditionContext, IamAction, IamDecision, IamPolicyEvaluator, Principal,
};

use crate::evaluator::{self, Decision, EvalRequest};
use crate::state::SharedIamState;

/// [`IamPolicyEvaluator`] backed by shared [`crate::state::IamState`].
#[derive(Clone)]
pub struct IamPolicyEvaluatorImpl {
    state: SharedIamState,
}

impl IamPolicyEvaluatorImpl {
    pub fn new(state: SharedIamState) -> Self {
        Self { state }
    }

    pub fn shared(state: SharedIamState) -> Arc<dyn IamPolicyEvaluator> {
        Arc::new(Self::new(state))
    }
}

impl IamPolicyEvaluator for IamPolicyEvaluatorImpl {
    fn evaluate(
        &self,
        principal: &Principal,
        action: &IamAction,
        context: &ConditionContext,
    ) -> IamDecision {
        let state = self.state.read();
        let policies = evaluator::collect_identity_policies(&state, principal);
        let request = EvalRequest {
            principal,
            action: action.action_string(),
            resource: action.resource.clone(),
            context: context.clone(),
        };
        decision_to_core(evaluator::evaluate(&policies, &request))
    }

    fn evaluate_with_resource_policy(
        &self,
        principal: &Principal,
        action: &IamAction,
        context: &ConditionContext,
        resource_policy_json: Option<&str>,
        resource_account_id: &str,
    ) -> IamDecision {
        let state = self.state.read();
        let identity_policies = evaluator::collect_identity_policies(&state, principal);
        let request = EvalRequest {
            principal,
            action: action.action_string(),
            resource: action.resource.clone(),
            context: context.clone(),
        };
        let resource_policy = resource_policy_json.map(evaluator::PolicyDocument::parse);
        decision_to_core(evaluator::evaluate_with_resource_policy(
            &identity_policies,
            resource_policy.as_ref(),
            &request,
            resource_account_id,
        ))
    }
}

fn decision_to_core(decision: Decision) -> IamDecision {
    match decision {
        Decision::Allow => IamDecision::Allow,
        Decision::ImplicitDeny => IamDecision::ImplicitDeny,
        Decision::ExplicitDeny => IamDecision::ExplicitDeny,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{IamAccessKey, IamState, IamUser};
    use chrono::Utc;
    use fakecloud_core::auth::PrincipalType;
    use parking_lot::RwLock;

    fn principal() -> Principal {
        Principal {
            arn: "arn:aws:iam::123456789012:user/alice".to_string(),
            user_id: "AIDAALICE".to_string(),
            account_id: "123456789012".to_string(),
            principal_type: PrincipalType::User,
            source_identity: None,
        }
    }

    fn setup() -> Arc<RwLock<IamState>> {
        let mut state = IamState::new("123456789012");
        state.users.insert(
            "alice".into(),
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
        state.access_keys.insert(
            "alice".into(),
            vec![IamAccessKey {
                access_key_id: "FKIAALICE".into(),
                secret_access_key: "s".into(),
                user_name: "alice".into(),
                status: "Active".into(),
                created_at: Utc::now(),
            }],
        );
        Arc::new(RwLock::new(state))
    }

    #[test]
    fn allow_policy_produces_allow_decision() {
        let state = setup();
        state.write().user_inline_policies.insert(
            "alice".into(),
            std::collections::HashMap::from([(
                "AllowGet".into(),
                r#"{"Statement":[{"Effect":"Allow","Action":"s3:GetObject","Resource":"*"}]}"#
                    .into(),
            )]),
        );
        let eval = IamPolicyEvaluatorImpl::new(state);
        let action = IamAction {
            service: "s3",
            action: "GetObject",
            resource: "arn:aws:s3:::bucket/key".into(),
        };
        assert_eq!(
            eval.evaluate(&principal(), &action, &ConditionContext::default()),
            IamDecision::Allow
        );
    }

    #[test]
    fn explicit_deny_takes_precedence() {
        let state = setup();
        state.write().user_inline_policies.insert(
            "alice".into(),
            std::collections::HashMap::from([
                (
                    "AllowAll".into(),
                    r#"{"Statement":[{"Effect":"Allow","Action":"*","Resource":"*"}]}"#.into(),
                ),
                (
                    "DenyGet".into(),
                    r#"{"Statement":[{"Effect":"Deny","Action":"s3:GetObject","Resource":"*"}]}"#
                        .into(),
                ),
            ]),
        );
        let eval = IamPolicyEvaluatorImpl::new(state);
        let action = IamAction {
            service: "s3",
            action: "GetObject",
            resource: "arn:aws:s3:::bucket/key".into(),
        };
        assert_eq!(
            eval.evaluate(&principal(), &action, &ConditionContext::default()),
            IamDecision::ExplicitDeny
        );
    }

    #[test]
    fn empty_policy_set_is_implicit_deny() {
        let state = setup();
        let eval = IamPolicyEvaluatorImpl::new(state);
        let action = IamAction {
            service: "s3",
            action: "GetObject",
            resource: "arn:aws:s3:::bucket/key".into(),
        };
        assert_eq!(
            eval.evaluate(&principal(), &action, &ConditionContext::default()),
            IamDecision::ImplicitDeny
        );
    }
}
