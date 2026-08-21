//! Lookup module errors

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum LookupError {
    #[error("database error")]
    Database(#[source] anyhow::Error),
}

impl From<deadpool::managed::PoolError<tokio_postgres::Error>> for LookupError {
    fn from(e: deadpool::managed::PoolError<tokio_postgres::Error>) -> Self {
        LookupError::Database(anyhow::anyhow!("pool: {e}"))
    }
}

impl From<tokio_postgres::Error> for LookupError {
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
        LookupError::Database(anyhow::anyhow!(
            "pg: {e}{}",
            detail.map(|d| format!(" [{d}]")).unwrap_or_default()
        ))
    }
}

impl From<anyhow::Error> for LookupError {
    fn from(e: anyhow::Error) -> Self {
        LookupError::Database(e)
    }
}
