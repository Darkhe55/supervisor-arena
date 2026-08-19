//! Database connection pool + migrations
//!
//! Phase 2: 提供 sqlx Postgres 连接池 + 自动 migration runner
//!
//! Note: 用 `after_connect` 钩子绕过 sqlx 0.8 + Alpine musl PG 的
//! startup packet 编码问题 — 连接建立后再跑 `SET lc_messages = 'C'`

use anyhow::{Context, Result};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    ConnectOptions, Executor, PgPool,
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
        // `after_connect` runs AFTER the connection is fully established
        // (so it bypasses the startup packet ErrorResponse encoding bug).
        // Forces C locale on every connection in the pool.
        .after_connect(|conn, _meta| {
            Box::pin(async move {
                conn.execute("SET lc_messages = 'C'").await?;
                conn.execute("SET client_encoding = 'UTF8'").await?;
                Ok(())
            })
        })
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
