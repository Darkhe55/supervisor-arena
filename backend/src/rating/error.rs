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
