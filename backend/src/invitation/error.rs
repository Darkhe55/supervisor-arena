//! Invitation module errors.

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum InvitationError {
    /// The provided code doesn't exist in the DB (typo, never
    /// issued, or already fully consumed + cleaned up).
    #[error("invitation code not found: {0}")]
    CodeNotFound(String),

    /// The code exists but has reached `max_uses` and can't
    /// accept more redemptions.
    #[error("invitation code has been fully used")]
    FullyUsed,

    /// The code's `expires_at` is in the past.
    #[error("invitation code expired at {0}")]
    Expired(DateTime<Utc>),

    /// An admin has explicitly revoked the code.
    #[error("invitation code was revoked at {0}")]
    Revoked(DateTime<Utc>),

    /// The new account_id would violate a constraint (e.g. the
    /// inviter doesn't exist).
    #[error("invalid inviter: {0}")]
    InvalidInviter(Uuid),

    /// Underlying DB error.
    #[error("database error")]
    Database(#[source] anyhow::Error),
}

impl From<deadpool::managed::PoolError<tokio_postgres::Error>> for InvitationError {
    fn from(e: deadpool::managed::PoolError<tokio_postgres::Error>) -> Self {
        InvitationError::Database(anyhow::anyhow!("pool: {e}"))
    }
}

impl From<tokio_postgres::Error> for InvitationError {
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
        InvitationError::Database(anyhow::anyhow!(
            "pg: {e}{}",
            detail.map(|d| format!(" [{d}]")).unwrap_or_default()
        ))
    }
}

impl From<anyhow::Error> for InvitationError {
    fn from(e: anyhow::Error) -> Self {
        InvitationError::Database(e)
    }
}
