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

/// Credentials resolved from an access key ID.
///
/// Returned by [`CredentialResolver::resolve`]. `account_id` is always
/// sourced from the credential's owning account, never from global config,
/// so that once multi-account isolation (#381) lands the same lookup returns
/// the correct account for the credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedCredential {
    pub secret_access_key: String,
    pub session_token: Option<String>,
    pub principal_arn: String,
    pub user_id: String,
    pub account_id: String,
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
    fn root_bypass_rejects_non_test_keys() {
        assert!(!is_root_bypass(""));
        assert!(!is_root_bypass("   "));
        assert!(!is_root_bypass("AKIAIOSFODNN7EXAMPLE"));
        assert!(!is_root_bypass("FKIA123456"));
        assert!(!is_root_bypass("tes"));
        assert!(!is_root_bypass("tst"));
    }
}
