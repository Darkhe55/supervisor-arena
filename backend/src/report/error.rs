//! Report (举报) module error types

use chrono::{DateTime, Utc};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum ReportError {
    /// Reporter tried to report their own content.
    #[error("cannot report your own content")]
    SelfReport,

    /// Target (rating/supervisor/additional_info) does not exist.
    #[error("target {target_type} {target_id} not found")]
    TargetNotFound { target_type: String, target_id: Uuid },

    /// The target exists but is in a state where it can't be reported
    /// (e.g. rating already superseded, supervisor already hidden).
    #[error("target {target_id} is not reportable in its current state")]
    TargetNotReportable { target_id: Uuid },

    /// Report is not in `pending` status (trying to claim/resolve an
    /// already-resolved one).
    #[error("report {0} is not in a reviewable state (status: {1})")]
    ReportNotPending(Uuid, String),

    /// Report id not found.
    #[error("report not found: {0}")]
    ReportNotFound(Uuid),

    /// Description / note too long.
    #[error("description / note too long ({0} chars, max 2000)")]
    TextTooLong(usize),

    /// Free-form validation failure.
    #[error("invalid input: {0}")]
    InvalidInput(String),

    /// Underlying DB error.
    #[error("database error")]
    Database(#[source] anyhow::Error),
}

impl From<deadpool::managed::PoolError<tokio_postgres::Error>> for ReportError {
    fn from(e: deadpool::managed::PoolError<tokio_postgres::Error>) -> Self {
        ReportError::Database(anyhow::anyhow!("pool: {e}"))
    }
}

impl From<tokio_postgres::Error> for ReportError {
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
        ReportError::Database(anyhow::anyhow!(
            "pg: {e}{}",
            detail.map(|d| format!(" [{d}]")).unwrap_or_default()
        ))
    }
}

impl From<anyhow::Error> for ReportError {
    fn from(e: anyhow::Error) -> Self {
        ReportError::Database(e)
    }
}

// Quiet unused-import warning when only one of these is used.
#[allow(dead_code)]
fn _phantom(_: DateTime<Utc>) {}
