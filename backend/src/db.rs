//! Database connection pool + migrations
//!
//! Phase 2: 提供 sqlx Postgres 连接池 + 自动 migration runner

use anyhow::{Context, Result};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions, PgPool,
};
use std::str::FromStr;
use std::time::Duration;
use tracing::{info, log::LevelFilter};

use crate::config::DatabaseConfig;

/// Build a Postgres connection pool from configuration
pub async fn build_pool(config: &DatabaseConfig) -> Result<PgPool> {
    let mut connect_options = PgConnectOptions::from_str(&config.url)
        .context("invalid DATABASE_URL")?
        .application_name("supervisor-arena");

    // Silence SQLx INFO logs (too noisy for every query)
    connect_options = connect_options.log_statements(LevelFilter::Debug);

    let pool = PgPoolOptions::new()
        .max_connections(config.max_connections)
        .min_connections(config.min_connections)
        .acquire_timeout(config.acquire_timeout())
        .connect_with(connect_options)
        .await
        .context("failed to connect to PostgreSQL")?;

    // Sanity check: SELECT 1
    sqlx::query("SELECT 1")
        .execute(&pool)
        .await
        .context("PostgreSQL sanity check failed")?;

    info!(
        max = config.max_connections,
        min = config.min_connections,
        "PostgreSQL pool initialized"
    );

    Ok(pool)
}

/// Run all pending migrations from ./migrations
pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    info!("Running migrations...");

    let migrator = sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("failed to run migrations")?;

    info!(
        applied = migrator.applied_migrations().len(),
        "Migrations applied successfully"
    );

    Ok(())
}

/// Graceful pool shutdown (called on app exit)
pub async fn close_pool(pool: PgPool) {
    pool.close().await;
    info!("PostgreSQL pool closed");
}

/// Acquire a single connection (for transactional operations)
///
/// Helper: many migrations need explicit transactions to be atomic.
pub async fn acquire(pool: &PgPool) -> Result<sqlx::PgConnection> {
    let conn = pool
        .acquire()
        .await
        .context("failed to acquire connection")?
        .detach();
    Ok(conn)
}

/// Database health check (for /health endpoint in future)
pub async fn health_check(pool: &PgPool) -> Result<()> {
    let timeout = Duration::from_secs(3);
    tokio::time::timeout(timeout, sqlx::query("SELECT 1").execute(pool))
        .await
        .context("database health check timeout")?
        .context("database health check failed")?;
    Ok(())
}
