//! Database connection pool + migrations
//!
//! Phase 2: 提供 sqlx Postgres 连接池 + 自动 migration runner
//!
//! Note: 使用 sqlx 0.7(0.8 在 Windows + Alpine musl PG 下有 non-UTF-8 错误)

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
    let connect_options = PgConnectOptions::from_str(&config.url)
        .context("invalid DATABASE_URL")?
        .application_name("supervisor-arena");

    // Silence SQLx INFO logs (too noisy for every query)
    let connect_options = connect_options.log_statements(LevelFilter::Debug);

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

    // sqlx 0.7: takes &pool
    sqlx::migrate!("./migrations")
        .run(pool)
        .await
        .context("failed to run migrations")?;

    // Count applied migrations from the tracking table
    let (count,): (i64,) =
        sqlx::query_as("SELECT COUNT(*) FROM _sqlx_migrations")
            .fetch_one(pool)
            .await
            .context("failed to count applied migrations")?;

    info!(applied = count, "Migrations applied successfully");

    Ok(())
}

/// Graceful pool shutdown (called on app exit)
pub async fn close_pool(pool: PgPool) {
    pool.close().await;
    info!("PostgreSQL pool closed");
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
