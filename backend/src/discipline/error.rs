//! Discipline (weight voting) module error types.
//!
//! Mirrors the pattern of `rating::error` / `account::error` —
//! `#[non_exhaustive]`, `thiserror`, and an `IntoResponse` wrapper in
//! `handler.rs` that maps to HTTP status codes.

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum DisciplineError {
    /// Discipline code is empty / too long / not in the lookup table.
    #[error("unknown discipline: {0}")]
    UnknownDiscipline(String),

    /// Dim is not one of the 6 valid codes.
    #[error("invalid dim: {0} (expected one of: research, resource, fit, currency, ethic, tool)")]
    InvalidDim(String),

    /// proposed_weight is out of [0, 1].
    #[error("invalid weight: {0} (expected 0.0..=1.0)")]
    InvalidWeight(f64),

    /// User is not eligible to vote (fewer than 3 approved ratings in this
    /// discipline, see OUTLINE §4.4 / C-2 "投票门槛").
    #[error("not eligible to vote in {discipline}: need ≥3 approved ratings in this discipline")]
    NotEligible { discipline: String },

    /// A weight was applied (or proposed + applied) for this (discipline, dim)
    /// within the 30-day cooldown window (C-2 "冷却期").
    #[error("cooldown active for ({discipline}, {dim}): last applied at {last_applied_at}")]
    CooldownActive {
        discipline: String,
        dim: String,
        last_applied_at: DateTime<Utc>,
    },

    /// The user already cast a ballot on this vote (one-vote-per-voter).
    #[error("already voted on vote {0}")]
    AlreadyVoted(Uuid),

    /// The vote is not in `pending` status (e.g. trying to ballot on an
    /// already-applied or rejected vote).
    #[error("vote {0} is not pending (status: {1})")]
    VoteNotPending(Uuid, String),

    /// The vote_id was not found.
    #[error("vote not found: {0}")]
    VoteNotFound(Uuid),

    /// User tried to ballot on their own proposal (anti-self-deal).
    #[error("cannot ballot on your own proposal")]
    SelfBallot,

    /// Underlying DB error.
    #[error("database error")]
    Database(#[source] anyhow::Error),
}

impl From<deadpool::managed::PoolError<tokio_postgres::Error>> for DisciplineError {
    fn from(e: deadpool::managed::PoolError<tokio_postgres::Error>) -> Self {
        DisciplineError::Database(anyhow::anyhow!("pool: {e}"))
    }
}

impl From<tokio_postgres::Error> for DisciplineError {
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
        DisciplineError::Database(anyhow::anyhow!(
            "pg: {e}{}",
            detail.map(|d| format!(" [{d}]")).unwrap_or_default()
        ))
    }
}

impl From<anyhow::Error> for DisciplineError {
    fn from(e: anyhow::Error) -> Self {
        DisciplineError::Database(e)
    }
}
