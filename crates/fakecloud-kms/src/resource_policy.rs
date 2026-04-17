//! KMS implementation of [`ResourcePolicyProvider`].
//!
//! Every KMS key carries a key policy (stored in [`KmsKey::policy`]).
//! `CreateKey` auto-generates a default policy granting `kms:*` to the
//! account root, matching AWS behavior. `PutKeyPolicy` / `GetKeyPolicy`
//! replace and read it. This provider is the read-side bridge into the
//! `fakecloud-core::auth::ResourcePolicyProvider` trait so the IAM
//! evaluator can combine key policies with identity policies.

use std::sync::Arc;

use fakecloud_core::auth::ResourcePolicyProvider;

use crate::state::SharedKmsState;

pub struct KmsResourcePolicyProvider {
    state: SharedKmsState,
}

impl KmsResourcePolicyProvider {
    pub fn new(state: SharedKmsState) -> Self {
        Self { state }
    }

    pub fn shared(state: SharedKmsState) -> Arc<dyn ResourcePolicyProvider> {
        Arc::new(Self::new(state))
    }
}

impl ResourcePolicyProvider for KmsResourcePolicyProvider {
    fn resource_policy(&self, service: &str, resource_arn: &str) -> Option<String> {
        if !service.eq_ignore_ascii_case("kms") {
            return None;
        }
        let key_id = parse_key_id_from_arn(resource_arn)?;
        let account_id = resource_arn.split(':').nth(4)?;
        let accounts = self.state.read();
        let state = accounts.get(account_id)?;
        let key = state.keys.get(&key_id)?;
        Some(key.policy.clone())
    }
}

/// Extract the key UUID from a KMS key ARN.
///
/// KMS key ARNs have the form `arn:aws:kms:REGION:ACCOUNT:key/KEY_ID`.
/// Returns `None` for wildcard (`*`) or malformed ARNs.
fn parse_key_id_from_arn(arn: &str) -> Option<String> {
    if arn == "*" {
        return None;
    }
    // arn:aws:kms:us-east-1:123456789012:key/UUID
    let rest = arn.strip_prefix("arn:aws:kms:")?;
    let key_id = rest.rsplit_once(":key/")?.1;
    if key_id.is_empty() {
        return None;
    }
    Some(key_id.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_key_id_valid_arn() {
        assert_eq!(
            parse_key_id_from_arn("arn:aws:kms:us-east-1:123456789012:key/abc-123"),
            Some("abc-123".to_string())
        );
    }

    #[test]
    fn parse_key_id_wildcard_returns_none() {
        assert_eq!(parse_key_id_from_arn("*"), None);
    }

    #[test]
    fn parse_key_id_not_kms_arn() {
        assert_eq!(parse_key_id_from_arn("arn:aws:s3:::my-bucket"), None);
    }

    #[test]
    fn parse_key_id_empty_key_part() {
        assert_eq!(
            parse_key_id_from_arn("arn:aws:kms:us-east-1:123456789012:key/"),
            None
        );
    }

    #[test]
    fn parse_key_id_mrk_key() {
        assert_eq!(
            parse_key_id_from_arn("arn:aws:kms:us-east-1:123456789012:key/mrk-abc123"),
            Some("mrk-abc123".to_string())
        );
    }
}
