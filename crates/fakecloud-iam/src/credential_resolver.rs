//! Adapter that implements [`fakecloud_core::auth::CredentialResolver`] over
//! the shared IAM state.
//!
//! SigV4 verification (and later IAM enforcement) runs in `fakecloud-core`,
//! which intentionally doesn't depend on `fakecloud-iam`. The trait lives in
//! core and the concrete resolver lives here, keeping the dependency edge
//! pointing the right way.

use std::sync::Arc;

use fakecloud_core::auth::{CredentialResolver, Principal, PrincipalType, ResolvedCredential};

use crate::state::SharedIamState;

/// [`CredentialResolver`] backed by an [`IamState`] shared via
/// [`SharedIamState`]. Acquires a write lock on lookup so expired STS
/// temporary credentials are purged in place.
#[derive(Clone)]
pub struct IamCredentialResolver {
    state: SharedIamState,
}

impl IamCredentialResolver {
    pub fn new(state: SharedIamState) -> Self {
        Self { state }
    }

    pub fn shared(state: SharedIamState) -> Arc<dyn CredentialResolver> {
        Arc::new(Self::new(state))
    }
}

impl CredentialResolver for IamCredentialResolver {
    fn resolve(&self, access_key_id: &str) -> Option<ResolvedCredential> {
        let mut state = self.state.write();
        let lookup = state.credential_secret(access_key_id)?;
        // Classify the principal by ARN shape. IAM user access keys carry
        // a `:user/` ARN; STS temp credentials carry the assumed-role /
        // federated-user / root ARN that was stashed when the credential
        // was issued (batch 2).
        let principal_type = PrincipalType::from_arn(&lookup.principal_arn);
        Some(ResolvedCredential {
            secret_access_key: lookup.secret_access_key,
            session_token: lookup.session_token,
            principal: Principal {
                arn: lookup.principal_arn,
                user_id: lookup.user_id,
                account_id: lookup.account_id,
                principal_type,
                source_identity: None,
            },
            session_policies: lookup.session_policies,
        })
    }
}

// Prevent rustc from warning on the unused import when the module is
// included via `lib.rs` without tests referencing it.
#[allow(dead_code)]
fn _assert_impl<T: CredentialResolver>() {}
const _: fn() = || {
    _assert_impl::<IamCredentialResolver>();
};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::state::{IamAccessKey, IamState, IamUser};
    use chrono::Utc;
    use parking_lot::RwLock;

    #[test]
    fn resolves_iam_user_secret_from_state() {
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
        state.access_keys.insert(
            "alice".to_string(),
            vec![IamAccessKey {
                access_key_id: "FKIAALICE".into(),
                secret_access_key: "the-secret".into(),
                user_name: "alice".into(),
                status: "Active".into(),
                created_at: Utc::now(),
            }],
        );
        let resolver = IamCredentialResolver::new(Arc::new(RwLock::new(state)));
        let resolved = resolver.resolve("FKIAALICE").unwrap();
        assert_eq!(resolved.secret_access_key, "the-secret");
        assert_eq!(
            resolved.principal.arn,
            "arn:aws:iam::123456789012:user/alice"
        );
        assert_eq!(resolved.principal.principal_type, PrincipalType::User);
        assert_eq!(resolved.session_token, None);
    }

    #[test]
    fn returns_none_for_unknown_akid() {
        let state = IamState::new("123456789012");
        let resolver = IamCredentialResolver::new(Arc::new(RwLock::new(state)));
        assert!(resolver.resolve("FKIANONE").is_none());
    }

    #[test]
    fn classifies_sts_assumed_role_principal() {
        use crate::state::StsTempCredential;
        let mut state = IamState::new("123456789012");
        state.sts_temp_credentials.insert(
            "FSIATEMP".to_string(),
            StsTempCredential {
                access_key_id: "FSIATEMP".into(),
                secret_access_key: "temp-secret".into(),
                session_token: "temp-token".into(),
                principal_arn: "arn:aws:sts::123456789012:assumed-role/ops/session".into(),
                user_id: "AROA:session".into(),
                account_id: "123456789012".into(),
                expiration: Utc::now() + chrono::Duration::minutes(30),
                session_policies: Vec::new(),
            },
        );
        let resolver = IamCredentialResolver::new(Arc::new(RwLock::new(state)));
        let resolved = resolver.resolve("FSIATEMP").unwrap();
        assert_eq!(
            resolved.principal.principal_type,
            PrincipalType::AssumedRole
        );
        assert_eq!(resolved.session_token.as_deref(), Some("temp-token"));
    }
}
