//! Aggregation repository: read approved ratings for a supervisor.

use deadpool_postgres::Pool;
use uuid::Uuid;

use super::error::AggregationError;

/// One approved rating row, as seen by the aggregator.
#[derive(Debug, Clone)]
pub struct ApprovedRating {
    pub dim: String,
    pub value: i16,
}

#[derive(Clone)]
pub struct RatingRepo {
    pool: Pool,
}

impl RatingRepo {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Fetch all approved ratings for a supervisor. The result is small
    /// (one row per (account, dim) current row) — at most a few hundred
    /// per supervisor for an active system.
    pub async fn list_approved(
        &self,
        supervisor_id: Uuid,
    ) -> Result<Vec<ApprovedRating>, AggregationError> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "SELECT dim, value FROM ratings
                 WHERE supervisor_id = $1::uuid
                   AND review_status = 'approved'
                   AND superseded_by IS NULL",
                &[&supervisor_id],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| ApprovedRating {
                dim: r.get(0),
                value: r.get(1),
            })
            .collect())
    }
}

impl From<deadpool::managed::PoolError<tokio_postgres::Error>> for AggregationError {
    fn from(e: deadpool::managed::PoolError<tokio_postgres::Error>) -> Self {
        AggregationError::Database(anyhow::anyhow!("pool: {e}"))
    }
}

impl From<tokio_postgres::Error> for AggregationError {
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
        AggregationError::Database(anyhow::anyhow!(
            "pg: {e}{}",
            detail.map(|d| format!(" [{d}]")).unwrap_or_default()
        ))
    }
}
