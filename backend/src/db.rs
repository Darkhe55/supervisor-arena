//! Database connection pool + migrations
//!
//! Phase 2: 提供 tokio-postgres + deadpool-postgres 连接池 + 手写 migration runner
//!
//! Why not sqlx 0.8: ErrorResponse 严格 UTF-8 校验 + Alpine musl PG 不兼容
//! Why not sqlx 0.7: 用户希望用新版本(虽然 0.7 也能工作)
//! Why hand-rolled migrations: 不依赖 sqlx::migrate! 宏
//! Why deadpool-postgres: 提供 async pool(防止 connection leak)

use anyhow::{anyhow, Context, Result};
use deadpool_postgres::{Config as PoolConfig, ManagerConfig, Pool, RecyclingMethod, Runtime};
use std::str::FromStr;
use std::time::Duration;
use tokio_postgres::{Config as PgConfig, NoTls};
use tracing::info;

use crate::config::DatabaseConfig;

/// Parse a postgres:// URL into a tokio-postgres Config
fn parse_postgres_url(url: &str) -> Result<PgConfig> {
    PgConfig::from_str(url).map_err(|e| anyhow!("invalid DATABASE_URL: {e}"))
}

/// Build a Postgres connection pool from a `DatabaseConfig` (production path).
pub async fn build_pool(config: &DatabaseConfig) -> Result<Pool> {
    let pg_config = parse_postgres_url(&config.url)?;

    let mut pool_config = PoolConfig::new();
    pool_config.host = pg_config.get_hosts().iter().find_map(|h| match h {
        tokio_postgres::config::Host::Tcp(s) => Some(s.clone()),
    });
    pool_config.port = pg_config.get_ports().first().copied();
    pool_config.user = pg_config.get_user().map(|s| s.to_string());
    pool_config.password = pg_config.get_password().map(|b| {
        String::from_utf8(b.to_vec()).unwrap_or_default()
    });
    pool_config.dbname = pg_config.get_dbname().map(|s| s.to_string());

    pool_config.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    pool_config.pool = Some(deadpool::managed::PoolConfig {
        max_size: config.max_connections as usize,
        timeouts: deadpool::managed::Timeouts {
            wait: Some(config.acquire_timeout()),
            create: Some(Duration::from_secs(5)),
            recycle: Some(Duration::from_secs(5)),
        },
        queue_mode: deadpool::managed::QueueMode::Fifo,
    });

    let pool = pool_config
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .context("failed to create deadpool")?;

    // Sanity check
    {
        let client = pool.get().await.context("failed to acquire test connection")?;
        client
            .query_one("SELECT 1", &[])
            .await
            .context("PostgreSQL sanity check failed")?;
    }

    info!(
        max = config.max_connections,
        min = config.min_connections,
        "PostgreSQL pool initialized"
    );

    Ok(pool)
}

/// Build a Postgres pool directly from a `postgres://...` URL.
///
/// Used by integration tests (testcontainers) that don't have a
/// `DatabaseConfig` from env vars. Uses sane default pool sizing.
pub async fn build_pool_from_url(url: &str) -> Result<Pool> {
    let pg_config = parse_postgres_url(url)?;

    let mut pool_config = PoolConfig::new();
    pool_config.host = pg_config.get_hosts().iter().find_map(|h| match h {
        tokio_postgres::config::Host::Tcp(s) => Some(s.clone()),
    });
    pool_config.port = pg_config.get_ports().first().copied();
    pool_config.user = pg_config.get_user().map(|s| s.to_string());
    pool_config.password = pg_config.get_password().map(|b| {
        String::from_utf8(b.to_vec()).unwrap_or_default()
    });
    pool_config.dbname = pg_config.get_dbname().map(|s| s.to_string());

    pool_config.manager = Some(ManagerConfig {
        recycling_method: RecyclingMethod::Fast,
    });
    pool_config.pool = Some(deadpool::managed::PoolConfig {
        max_size: 10, // smaller pool for tests
        timeouts: deadpool::managed::Timeouts {
            wait: Some(Duration::from_secs(5)),
            create: Some(Duration::from_secs(5)),
            recycle: Some(Duration::from_secs(5)),
        },
        queue_mode: deadpool::managed::QueueMode::Fifo,
    });

    let pool = pool_config
        .create_pool(Some(Runtime::Tokio1), NoTls)
        .context("failed to create deadpool")?;

    // Sanity check
    {
        let client = pool.get().await.context("failed to acquire test connection")?;
        client
            .query_one("SELECT 1", &[])
            .await
            .context("PostgreSQL sanity check failed")?;
    }

    Ok(pool)
}

/// Run all pending migrations from ./migrations
///
/// Migration file naming: NNNNNNNNNNNNNN_description.sql (e.g. 20260819000001_create_accounts.sql)
/// We track applied migrations in a `_migrations` table (version BIGINT, description TEXT, applied_at TIMESTAMPTZ).
pub async fn run_migrations(pool: &Pool) -> Result<()> {
    info!("Running migrations...");

    let mut client = pool
        .get()
        .await
        .context("failed to acquire connection for migrations")?;

    // Create _migrations tracking table
    client
        .batch_execute(
            "
            CREATE TABLE IF NOT EXISTS _migrations (
                version BIGINT PRIMARY KEY,
                description TEXT NOT NULL,
                applied_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
            );
            ",
        )
        .await
        .context("failed to create _migrations table")?;

    // Load migration files (sorted by filename)
    let migrations_dir = std::path::Path::new("./migrations");
    if !migrations_dir.exists() {
        return Err(anyhow!(
            "migrations directory not found: {}",
            migrations_dir.display()
        ));
    }

    let mut entries: Vec<_> = std::fs::read_dir(migrations_dir)
        .context("failed to read migrations dir")?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().and_then(|x| x.to_str()) == Some("sql"))
        .collect();
    entries.sort_by_key(|e| e.path());

    // Get already-applied versions
    let applied_rows = client
        .query("SELECT version FROM _migrations", &[])
        .await
        .context("failed to query _migrations")?;
    let applied: std::collections::HashSet<i64> = applied_rows
        .iter()
        .map(|r| r.get::<_, i64>(0))
        .collect();

    let mut applied_count = 0;
    for entry in &entries {
        let path = entry.path();
        let filename = entry.file_name().to_string_lossy().to_string();

        // Parse version from filename "NNNNN_description.sql"
        let version: i64 = filename
            .split('_')
            .next()
            .ok_or_else(|| anyhow!("invalid migration filename: {filename}"))?
            .parse()
            .with_context(|| format!("invalid version in {filename}"))?;

        if applied.contains(&version) {
            continue;
        }

        // Read + execute migration SQL
        let sql = std::fs::read_to_string(&path)
            .with_context(|| format!("failed to read {path:?}"))?;

        let tx = client
            .transaction()
            .await
            .context("failed to start migration transaction")?;
        tx.batch_execute(&sql)
            .await
            .with_context(|| format!("failed to execute migration {filename}"))?;
        tx.execute(
            "INSERT INTO _migrations (version, description) VALUES ($1, $2)",
            &[&version, &filename],
        )
        .await
        .context("failed to record migration")?;
        tx.commit()
            .await
            .context("failed to commit migration transaction")?;

        applied_count += 1;
        info!(version = version, file = %filename, "applied migration");
    }

    info!(
        applied = applied_count,
        total = entries.len(),
        "Migrations complete"
    );

    Ok(())
}

/// Graceful pool shutdown
pub async fn close_pool(pool: Pool) {
    pool.close();
    info!("PostgreSQL pool closed");
}

/// Database health check
pub async fn health_check(pool: &Pool) -> Result<()> {
    let timeout = Duration::from_secs(3);
    let client = tokio::time::timeout(timeout, pool.get())
        .await
        .context("database health check timeout")?
        .context("failed to acquire connection")?;
    tokio::time::timeout(
        timeout,
        client.query_one("SELECT 1", &[]),
    )
    .await
    .context("database health check timeout")?
    .context("database query failed")?;
    Ok(())
}
