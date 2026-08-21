//! Account module error types
//!
//! All variants are `#[non_exhaustive]` so future additions don't break callers.
//! Auth errors are intentionally **opaque** to the client: we never say
//! "user not found" vs "wrong password" — both return the same `InvalidCredentials`
//! to prevent account enumeration.

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AccountError {
    /// Email format is invalid.
    #[error("invalid email: {0}")]
    InvalidEmail(String),

    /// Password is too weak or empty.
    #[error("weak password: {0}")]
    WeakPassword(String),

    /// Discipline / institution strings are empty or too long.
    #[error("invalid {field}: {message}")]
    InvalidField { field: &'static str, message: String },

    /// Email already registered. Same wire response as `InvalidCredentials` —
    /// actually we DO return a distinct code here so legitimate clients get
    /// a useful error. Account enumeration is mitigated by the rate limit.
    #[error("email already registered")]
    EmailTaken,

    /// Login: email not found OR wrong password. Same response either way.
    #[error("invalid credentials")]
    InvalidCredentials,

    /// JWT token is missing, malformed, or expired.
    #[error("invalid or expired token")]
    InvalidToken,

    /// JWT signature is valid but `sub` is not a valid UUID.
    #[error("malformed token subject")]
    MalformedSubject,

    /// Account has been soft-removed (E-1) or banned.
    #[error("account unavailable")]
    AccountUnavailable,

    /// Rate limit hit (too many logins, registrations, etc.).
    /// Carries the kind (e.g. "login_per_min") and the suggested
    /// retry-after in seconds — the handler surfaces these in the
    /// 429 response body and `Retry-After` header.
    #[error("too many requests, retry in {retry_after_secs}s (kind: {kind})")]
    RateLimited {
        kind: &'static str,
        retry_after_secs: u64,
    },

    /// Underlying DB error. Logged but never surfaced to the client
    /// (we return 500 with a generic message).
    #[error("database error")]
    Database(#[source] anyhow::Error),

    /// Underlying crypto error.
    #[error("crypto error")]
    Crypto(#[source] crate::crypto::CryptoError),

    /// JWT library error.
    #[error("jwt error")]
    Jwt(#[source] jsonwebtoken::errors::Error),
}

impl From<crate::crypto::CryptoError> for AccountError {
    fn from(e: crate::crypto::CryptoError) -> Self {
        AccountError::Crypto(e)
    }
}

impl From<jsonwebtoken::errors::Error> for AccountError {
    fn from(e: jsonwebtoken::errors::Error) -> Self {
        AccountError::Jwt(e)
    }
}

impl From<crate::rate_limit::RateLimitError> for AccountError {
    fn from(e: crate::rate_limit::RateLimitError) -> Self {
        use crate::rate_limit::RateLimitError as R;
        match e {
            R::RateLimited { kind, retry_after_secs } => {
                AccountError::RateLimited {
                    kind,
                    retry_after_secs,
                }
            }
        }
    }
}

impl From<anyhow::Error> for AccountError {
    fn from(e: anyhow::Error) -> Self {
        AccountError::Database(e)
    }
}
