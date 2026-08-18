//! supervisor-arena library crate
//!
//! Phase 1 scaffold — modules will be added incrementally:
//!   Phase 2: db (migrations + connection pool)
//!   Phase 3: crypto (AES-256-GCM + HMAC-SHA256 + Argon2id)
//!   Phase 4: account (registration, login, JWT)
//!   Phase 5: supervisor + alias_generator
//!   Phase 6: rating
//!   Phase 7: aggregation + public_api
//!   Phase 8: tests

pub mod config;
pub mod observability;

use anyhow::Result;
use axum::{routing::get, Json, Router};
use serde::Serialize;
use tracing::info;

use crate::config::AppConfig;

pub async fn run(config: AppConfig) -> Result<()> {
    let app = build_router(config.clone());

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

fn build_router(_config: AppConfig) -> Router {
    Router::new()
        .route("/health", get(health))
        .route("/version", get(version))
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

async fn version() -> Json<HealthResponse> {
    Json(HealthResponse {
        status: "ok",
        version: env!("CARGO_PKG_VERSION"),
    })
}
