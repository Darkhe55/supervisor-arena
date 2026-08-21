//! Rating module errors

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RatingError {
    #[error("invalid rating dimension: {0} (expected one of: research, resource, fit, currency, ethic, tool)")]
    InvalidDim(String),

    #[error("invalid value: {0} (expected -100..=100)")]
    InvalidValue(i16),

    #[error("invalid additional_level: {0} (expected L1, L2, L3, L4)")]
    InvalidAdditionalLevel(String),

    #[error("supervisor not found: {0}")]
    SupervisorNotFound(String),

    #[error("supervisor not approved: {0}")]
    SupervisorNotApproved(String),

    #[error("invalid evidence URL: {0}")]
    InvalidEvidence(String),

    /// Daily / per-minute rate limit hit (M3 §7.6 / E-3). The handler
    /// maps this to HTTP 429 with a `Retry-After` header.
    #[error("rate limit hit: {kind}, retry in {retry_after_secs}s")]
    RateLimited { kind: &'static str, retry_after_secs: u64 },

    #[error("database error")]
    Database(#[source] anyhow::Error),

    #[error("crypto error")]
    Crypto(#[source] crate::crypto::CryptoError),
}

impl From<crate::crypto::CryptoError> for RatingError {
    fn from(e: crate::crypto::CryptoError) -> Self {
        RatingError::Crypto(e)
    }
}

impl From<anyhow::Error> for RatingError {
    fn from(e: anyhow::Error) -> Self {
        RatingError::Database(e)
    }
}

impl From<deadpool::managed::PoolError<tokio_postgres::Error>> for RatingError {
    fn from(e: deadpool::managed::PoolError<tokio_postgres::Error>) -> Self {
        RatingError::Database(anyhow::anyhow!("pool: {e}"))
    }
}

impl From<tokio_postgres::Error> for RatingError {
    fn from(e: tokio_postgres::Error) -> Self {
        let detail = e.as_db_error().map(|db| {
            format!(
                "pg sqlstate={:?} message={} detail={:?} hint={:?}",
                db.code(),
                db.message(),
                db.detail(),
                db.hint()
            )
        });
        RatingError::Database(anyhow::anyhow!(
            "pg: {e}{}",
            detail.map(|d| format!(" [{d}]")).unwrap_or_default()
        ))
    }
}

impl From<crate::rate_limit::RateLimitError> for RatingError {
    fn from(e: crate::rate_limit::RateLimitError) -> Self {
        use crate::rate_limit::RateLimitError as R;
        match e {
            R::RateLimited { kind, retry_after_secs } => {
                RatingError::RateLimited { kind, retry_after_secs }
            }
        }
    }
}
