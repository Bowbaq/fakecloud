//! Authentication and authorization primitives shared across services.
//!
//! This module defines the opt-in modes for SigV4 signature verification and
//! IAM policy enforcement, plus the reserved "root bypass" identity that
//! short-circuits both checks when enabled.
//!
//! Neither feature is enforced at this layer — the types are plumbed through
//! [`crate::dispatch::DispatchConfig`] and consulted later by dispatch and
//! service handlers once the corresponding batches land. See
//! `/docs/reference/security` (added in a later batch) for the user-facing
//! contract.

use std::fmt;
use std::str::FromStr;

/// Kind of principal a set of credentials resolves to.
///
/// Used to drive IAM policy evaluation (Phase 2) and the `GetCallerIdentity`
/// response shape. Inferred from the credential's storage path in
/// [`IamState`] and — for STS temporary credentials — from the ARN form
/// `arn:aws:sts::<account>:assumed-role/...` or `federated-user/...`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum PrincipalType {
    /// An IAM user access key (AKID created via `CreateAccessKey`).
    User,
    /// An assumed role session issued by `AssumeRole` /
    /// `AssumeRoleWithWebIdentity` / `AssumeRoleWithSAML`.
    AssumedRole,
    /// Credentials issued by `GetFederationToken` — i.e. a federated user.
    FederatedUser,
    /// The account root identity. Reserved for explicit `...:root` ARNs
    /// only; do not return this from a generic fallback because root
    /// principals bypass IAM enforcement (see `Principal::is_root`).
    Root,
    /// The ARN didn't match any known shape. Treated as a non-root,
    /// non-bypassable principal so a malformed or unexpected ARN can never
    /// silently grant elevated permissions during IAM evaluation.
    Unknown,
}

impl PrincipalType {
    pub fn as_str(self) -> &'static str {
        match self {
            PrincipalType::User => "user",
            PrincipalType::AssumedRole => "assumed-role",
            PrincipalType::FederatedUser => "federated-user",
            PrincipalType::Root => "root",
            PrincipalType::Unknown => "unknown",
        }
    }

    /// Classify a principal from its ARN. Returns [`PrincipalType::Unknown`]
    /// for ARNs that don't match any of the well-known principal shapes —
    /// **never** [`PrincipalType::Root`] as a fallback, because root
    /// bypasses IAM enforcement and silently treating malformed ARNs as
    /// root would let unexpected inputs grant elevated permissions
    /// (identified by cubic in PR #391 review).
    pub fn from_arn(arn: &str) -> Self {
        if arn.ends_with(":root") {
            PrincipalType::Root
        } else if arn.contains(":user/") {
            PrincipalType::User
        } else if arn.contains(":assumed-role/") {
            PrincipalType::AssumedRole
        } else if arn.contains(":federated-user/") {
            PrincipalType::FederatedUser
        } else {
            PrincipalType::Unknown
        }
    }
}

/// Identity of the caller making a request, once its credentials have been
/// resolved. Attached to [`crate::service::AwsRequest::principal`] so
/// handlers can make identity-based decisions without re-parsing the
/// Authorization header.
///
/// `account_id` is always sourced from the credential itself (via
/// [`CredentialResolver`]), never from global config — #381 note.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Principal {
    pub arn: String,
    pub user_id: String,
    pub account_id: String,
    pub principal_type: PrincipalType,
    /// Optional source identity string, carried through from
    /// `AssumeRole`'s `SourceIdentity` parameter. Reserved for later
    /// batches that wire session policies and auditing.
    pub source_identity: Option<String>,
}

impl Principal {
    /// Is this caller the account's root identity? Root bypasses IAM
    /// evaluation, matching AWS.
    pub fn is_root(&self) -> bool {
        matches!(self.principal_type, PrincipalType::Root) || self.arn.ends_with(":root")
    }
}

/// Credentials resolved from an access key ID.
///
/// Returned by [`CredentialResolver::resolve`]. Holds both the secret access
/// key (needed for SigV4 verification) and the resolved [`Principal`]
/// (needed for IAM enforcement and `GetCallerIdentity` consolidation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCredential {
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub principal: Principal,
}

impl ResolvedCredential {
    /// Convenience accessors for the flat fields batch 3 callers use. Kept
    /// as methods rather than re-adding the fields to avoid making the
    /// shape inconsistent with [`Principal`] itself.
    pub fn principal_arn(&self) -> &str {
        &self.principal.arn
    }

    pub fn user_id(&self) -> &str {
        &self.principal.user_id
    }

    pub fn account_id(&self) -> &str {
        &self.principal.account_id
    }
}

/// Abstraction over "given an access key ID, return the secret and resolved
/// principal." Implemented by the IAM crate against `IamState`; the core
/// crate depends only on the trait so there's no circular dependency.
///
/// Implementations must be cheap to clone-share via `Arc` and must be
/// thread-safe — dispatch calls them from an axum handler under a tokio
/// worker.
pub trait CredentialResolver: Send + Sync {
    /// Resolve `access_key_id` to its secret access key and principal.
    /// Returns `None` when the AKID is unknown or its underlying credential
    /// has expired.
    fn resolve(&self, access_key_id: &str) -> Option<ResolvedCredential>;
}

/// How IAM identity policies are evaluated for incoming requests.
///
/// Default is [`IamMode::Off`] — existing behavior, policies are stored but
/// never consulted. [`IamMode::Soft`] evaluates and logs denied decisions via
/// the `fakecloud::iam::audit` tracing target without failing the request, and
/// [`IamMode::Strict`] returns an `AccessDeniedException` in the protocol-
/// correct shape.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum IamMode {
    /// Do not evaluate IAM policies.
    #[default]
    Off,
    /// Evaluate policies and log audit events for denied requests, but allow
    /// the request to proceed.
    Soft,
    /// Evaluate policies and reject denied requests with `AccessDeniedException`.
    Strict,
}

impl IamMode {
    /// Returns true when policy evaluation should occur at all.
    pub fn is_enabled(self) -> bool {
        !matches!(self, IamMode::Off)
    }

    /// Returns true when denied decisions should fail the request.
    pub fn is_strict(self) -> bool {
        matches!(self, IamMode::Strict)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            IamMode::Off => "off",
            IamMode::Soft => "soft",
            IamMode::Strict => "strict",
        }
    }
}

impl fmt::Display for IamMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Parse error for [`IamMode`] from string.
#[derive(Debug)]
pub struct ParseIamModeError(String);

impl fmt::Display for ParseIamModeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "invalid IAM mode `{}`; expected one of: off, soft, strict",
            self.0
        )
    }
}

impl std::error::Error for ParseIamModeError {}

impl FromStr for IamMode {
    type Err = ParseIamModeError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "off" | "none" | "disabled" => Ok(IamMode::Off),
            "soft" | "audit" | "warn" => Ok(IamMode::Soft),
            "strict" | "enforce" | "deny" => Ok(IamMode::Strict),
            other => Err(ParseIamModeError(other.to_string())),
        }
    }
}

/// Reserved root-identity convention.
///
/// Any access key whose ID begins with `test` (case-insensitive) is treated as
/// the de-facto root bypass. This matches the long-standing community
/// convention used by LocalStack and Floci: `test`/`test` credentials should
/// always "just work" for local development.
///
/// When SigV4 verification or IAM enforcement is enabled, callers using a
/// bypass AKID skip both checks. We emit a one-time startup WARN whenever
/// enforcement is turned on so users understand that unsigned `test` clients
/// will silently receive positive results.
pub fn is_root_bypass(access_key_id: &str) -> bool {
    access_key_id
        .trim()
        .get(..4)
        .is_some_and(|prefix| prefix.eq_ignore_ascii_case("test"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn iam_mode_default_is_off() {
        assert_eq!(IamMode::default(), IamMode::Off);
        assert!(!IamMode::default().is_enabled());
    }

    #[test]
    fn iam_mode_from_str_accepts_primary_values() {
        assert_eq!(IamMode::from_str("off").unwrap(), IamMode::Off);
        assert_eq!(IamMode::from_str("soft").unwrap(), IamMode::Soft);
        assert_eq!(IamMode::from_str("strict").unwrap(), IamMode::Strict);
    }

    #[test]
    fn iam_mode_from_str_is_case_insensitive_and_trimmed() {
        assert_eq!(IamMode::from_str(" OFF ").unwrap(), IamMode::Off);
        assert_eq!(IamMode::from_str("Soft").unwrap(), IamMode::Soft);
        assert_eq!(IamMode::from_str("STRICT").unwrap(), IamMode::Strict);
    }

    #[test]
    fn iam_mode_from_str_accepts_aliases() {
        assert_eq!(IamMode::from_str("disabled").unwrap(), IamMode::Off);
        assert_eq!(IamMode::from_str("audit").unwrap(), IamMode::Soft);
        assert_eq!(IamMode::from_str("enforce").unwrap(), IamMode::Strict);
    }

    #[test]
    fn iam_mode_from_str_rejects_garbage() {
        assert!(IamMode::from_str("").is_err());
        assert!(IamMode::from_str("allow").is_err());
        assert!(IamMode::from_str("yes").is_err());
    }

    #[test]
    fn iam_mode_display_roundtrips() {
        for mode in [IamMode::Off, IamMode::Soft, IamMode::Strict] {
            assert_eq!(IamMode::from_str(&mode.to_string()).unwrap(), mode);
        }
    }

    #[test]
    fn iam_mode_flags() {
        assert!(!IamMode::Off.is_enabled());
        assert!(!IamMode::Off.is_strict());
        assert!(IamMode::Soft.is_enabled());
        assert!(!IamMode::Soft.is_strict());
        assert!(IamMode::Strict.is_enabled());
        assert!(IamMode::Strict.is_strict());
    }

    #[test]
    fn root_bypass_matches_test_prefix() {
        assert!(is_root_bypass("test"));
        assert!(is_root_bypass("TEST"));
        assert!(is_root_bypass("Test"));
        assert!(is_root_bypass("testAccessKey"));
        assert!(is_root_bypass("TESTAKIAIOSFODNN7EXAMPLE"));
    }

    #[test]
    fn root_bypass_does_not_panic_on_multibyte_input() {
        // Byte index 4 falls inside a multi-byte UTF-8 character; must not panic.
        assert!(!is_root_bypass("té"));
        assert!(!is_root_bypass("日本語キー"));
        assert!(!is_root_bypass("🔑🔑"));
    }

    #[test]
    fn principal_type_from_arn_classifies_known_shapes() {
        assert_eq!(
            PrincipalType::from_arn("arn:aws:iam::123456789012:user/alice"),
            PrincipalType::User
        );
        assert_eq!(
            PrincipalType::from_arn("arn:aws:sts::123456789012:assumed-role/R/s"),
            PrincipalType::AssumedRole
        );
        assert_eq!(
            PrincipalType::from_arn("arn:aws:sts::123456789012:federated-user/bob"),
            PrincipalType::FederatedUser
        );
        assert_eq!(
            PrincipalType::from_arn("arn:aws:iam::123456789012:root"),
            PrincipalType::Root
        );
    }

    #[test]
    fn principal_type_unparseable_is_unknown_not_root() {
        // Identified by cubic on PR #391: falling back to Root would let
        // malformed or unexpected ARNs bypass IAM enforcement, since
        // Principal::is_root short-circuits evaluation. The fallback must
        // be the non-bypassable Unknown variant.
        assert_eq!(PrincipalType::from_arn("not-an-arn"), PrincipalType::Unknown);
        assert_eq!(PrincipalType::from_arn(""), PrincipalType::Unknown);
        assert_eq!(
            PrincipalType::from_arn("arn:aws:iam::123456789012:something-weird"),
            PrincipalType::Unknown
        );

        // And a Principal built from an Unknown ARN must not be treated
        // as root for enforcement decisions.
        let p = Principal {
            arn: "garbage".to_string(),
            user_id: "x".to_string(),
            account_id: "123456789012".to_string(),
            principal_type: PrincipalType::Unknown,
            source_identity: None,
        };
        assert!(!p.is_root());
    }

    #[test]
    fn principal_is_root_covers_root_type_and_arn_suffix() {
        let p = Principal {
            arn: "arn:aws:iam::123456789012:root".to_string(),
            user_id: "AIDAROOT".to_string(),
            account_id: "123456789012".to_string(),
            principal_type: PrincipalType::Root,
            source_identity: None,
        };
        assert!(p.is_root());

        let user = Principal {
            arn: "arn:aws:iam::123456789012:user/alice".to_string(),
            user_id: "AIDAALICE".to_string(),
            account_id: "123456789012".to_string(),
            principal_type: PrincipalType::User,
            source_identity: None,
        };
        assert!(!user.is_root());
    }

    #[test]
    fn resolved_credential_accessors_forward_to_principal() {
        let rc = ResolvedCredential {
            secret_access_key: "s".into(),
            session_token: None,
            principal: Principal {
                arn: "arn:aws:iam::123456789012:user/alice".into(),
                user_id: "AIDAALICE".into(),
                account_id: "123456789012".into(),
                principal_type: PrincipalType::User,
                source_identity: None,
            },
        };
        assert_eq!(rc.principal_arn(), "arn:aws:iam::123456789012:user/alice");
        assert_eq!(rc.user_id(), "AIDAALICE");
        assert_eq!(rc.account_id(), "123456789012");
    }

    #[test]
    fn root_bypass_rejects_non_test_keys() {
        assert!(!is_root_bypass(""));
        assert!(!is_root_bypass("   "));
        assert!(!is_root_bypass("AKIAIOSFODNN7EXAMPLE"));
        assert!(!is_root_bypass("FKIA123456"));
        assert!(!is_root_bypass("tes"));
        assert!(!is_root_bypass("tst"));
    }
}
