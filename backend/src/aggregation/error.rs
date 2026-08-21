//! Aggregation errors

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AggregationError {
    #[error("database error")]
    Database(#[source] anyhow::Error),
}

impl From<anyhow::Error> for AggregationError {
    fn from(e: anyhow::Error) -> Self {
        AggregationError::Database(e)
    }
}
