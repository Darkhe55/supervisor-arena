//! Supervisor module errors
//!
//! `AliasError` lives in `alias.rs` (separate to avoid name collision with
//! the supervisor-level error type).

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum SupervisorError {
    #[error("invalid input: {0}")]
    InvalidInput(String),

    #[error("discipline not found: {0}")]
    UnknownDiscipline(String),

    #[error("college not found: {0}")]
    UnknownCollege(String),

    #[error("not found")]
    NotFound,

    #[error("duplicate: {0}")]
    Duplicate(String),

    #[error("k-anonymity threshold not met: {0} < {1}")]
    KAnonymityHidden(i32, i32),

    #[error("alias generation failed: {0}")]
    AliasGeneration(String),

    #[error("database error")]
    Database(#[source] anyhow::Error),

    #[error("crypto error")]
    Crypto(#[source] crate::crypto::CryptoError),
}

impl From<crate::crypto::CryptoError> for SupervisorError {
    fn from(e: crate::crypto::CryptoError) -> Self {
        SupervisorError::Crypto(e)
    }
}

impl From<anyhow::Error> for SupervisorError {
    fn from(e: anyhow::Error) -> Self {
        SupervisorError::Database(e)
    }
}

impl From<deadpool::managed::PoolError<tokio_postgres::Error>> for SupervisorError {
    fn from(e: deadpool::managed::PoolError<tokio_postgres::Error>) -> Self {
        SupervisorError::Database(anyhow::anyhow!("pool: {e}"))
    }
}

impl From<tokio_postgres::Error> for SupervisorError {
    fn from(e: tokio_postgres::Error) -> Self {
        // Surface the SQL state + detail for debugging — but keep the
        // top-level error as `Database` so the handler maps it to 500.
        let detail = e.as_db_error().map(|db| {
            format!(
                "pg sqlstate={:?} message={} detail={:?} hint={:?}",
                db.code(),
                db.message(),
                db.detail(),
                db.hint()
            )
        });
        SupervisorError::Database(anyhow::anyhow!(
            "pg: {e}{}",
            detail.map(|d| format!(" [{d}]")).unwrap_or_default()
        ))
    }
}
