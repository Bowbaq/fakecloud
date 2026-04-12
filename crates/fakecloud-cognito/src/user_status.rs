//! Cognito user-status wire values.
//!
//! Cognito represents a user's account state as one of a small set of
//! string literals (``UNCONFIRMED``, ``CONFIRMED``, ``FORCE_CHANGE_PASSWORD``,
//! etc.) on the wire. We keep them as constants rather than an enum so the
//! JSON representation stays byte-identical to AWS without serde attributes,
//! and to make sure the literal is spelled consistently everywhere.

pub const UNCONFIRMED: &str = "UNCONFIRMED";
pub const CONFIRMED: &str = "CONFIRMED";
pub const ARCHIVED: &str = "ARCHIVED";
pub const COMPROMISED: &str = "COMPROMISED";
pub const UNKNOWN: &str = "UNKNOWN";
pub const RESET_REQUIRED: &str = "RESET_REQUIRED";
pub const FORCE_CHANGE_PASSWORD: &str = "FORCE_CHANGE_PASSWORD";
pub const EXTERNAL_PROVIDER: &str = "EXTERNAL_PROVIDER";
