//! supervisor-arena library crate
//!
//! Phase 1 scaffold — modules will be added incrementally:
//!   Phase 2: db (migrations + connection pool) ✅
//!   Phase 3: crypto (AES-256-GCM + HMAC-SHA256 + Argon2id)
//!   Phase 4: account (registration, login, JWT)
//!   Phase 5: supervisor + alias_generator
//!   Phase 6: rating
//!   Phase 7: aggregation + public_api
//!   Phase 8: tests

pub mod config;
pub mod db;
pub mod observability;

use anyhow::Result;
use axum::{extract::State, routing::get, Json, Router};
use serde::Serialize;
use sqlx::PgPool;
use std::sync::Arc;
use tracing::info;

use crate::config::AppConfig;

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: PgPool,
}

pub async fn run(config: AppConfig) -> Result<()> {
    // Initialize database pool
    let db = db::build_pool(&config.database).await?;
    db::run_migrations(&db).await?;

    // Build state
    let state = AppState {
        config: Arc::new(config.clone()),
        db,
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
        .route("/version", get(version))
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

async fn version() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
