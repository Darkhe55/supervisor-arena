//! supervisor-arena library crate
//!
//! Phase 1 scaffold — modules will be added incrementally:
//!   Phase 2: db (migrations + connection pool) ✅ Plan B (deadpool-postgres + tokio-postgres)
//!   Phase 3: crypto (AES-256-GCM + HMAC-SHA256 + Argon2id) ✅ LocalKeyStore
//!   Phase 4: account (registration, login, JWT, /auth/me) ✅
//!   Phase 5: supervisor + alias_generator
//!   Phase 6: rating
//!   Phase 7: aggregation + public_api
//!   Phase 8: tests

pub mod account;
pub mod config;
pub mod crypto;
pub mod db;
pub mod observability;
pub mod supervisor;

use anyhow::Result;
use axum::{extract::State, routing::get, Json, Router};
use deadpool_postgres::Pool;
use serde::Serialize;
use std::sync::Arc;
use tracing::info;

use crate::config::AppConfig;
use crate::crypto::LocalKeyStore;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: Pool,
    pub keys: Arc<LocalKeyStore>,
}

pub async fn run(config: AppConfig) -> Result<()> {
    // Initialize key store (parses hex keys, fails fast on bad config)
    let keys = LocalKeyStore::from_config(&config.encryption)
        .map_err(|e| anyhow::anyhow!("invalid encryption config: {e}"))?;
    info!(key_id = %keys.key_id(), "LocalKeyStore initialized");

    // Initialize database pool
    let db = db::build_pool(&config.database).await?;
    db::run_migrations(&db).await?;

    // Build state
    let state = AppState {
        config: Arc::new(config.clone()),
        db,
        keys: Arc::new(keys),
    };

    let app = build_router(state);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bind {addr}: {e}"))?;

    info!(addr = %addr, "listening");

    axum::serve(listener, app)
        .await
        .map_err(|e| anyhow::anyhow!("server error: {e}"))?;

    Ok(())
}

fn build_router(state: AppState) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/health/db", get(health_db))
        .route("/health/crypto", get(health_crypto))
        .route("/version", get(version))
        .nest("/auth", account::handler::auth_router())
        .with_state(state)
}

#[derive(Serialize)]
struct HealthResponse {
    status: &'static str,
    version: &'static str,
}

async fn health() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn health_db(State(state): State<AppState>) -> Json<HealthResponse> {
    let db_ok = db::health_check(&state.db).await.is_ok();
    Json(HealthResponse {
        status: if db_ok { "ok" } else { "degraded" },
        version: env!("CARGO_PKG_VERSION"),
    })
}

/// Smoke-test the key store: encrypt then decrypt a sentinel string.
/// Returns "ok" if round-trip succeeds, "degraded" otherwise.
async fn health_crypto(State(state): State<AppState>) -> Json<HealthResponse> {
    use crate::crypto::aes;
    let key = state.keys.field_key();
    let sentinel = "supervisor-arena/health";
    let ok = match aes::encrypt(key, sentinel.as_bytes(), Some(b"health-check")) {
        Ok(blob) => aes::decrypt(key, &blob, Some(b"health-check"))
            .map(|pt| pt.as_slice() == sentinel.as_bytes())
            .unwrap_or(false),
        Err(_) => false,
    };
    Json(HealthResponse {
        status: if ok { "ok" } else { "degraded" },
        version: env!("CARGO_PKG_VERSION"),
    })
}

async fn version() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
