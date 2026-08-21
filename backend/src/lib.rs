//! supervisor-arena library crate
//!
//! Phases 1-8 from the M1 MVP backend plan; M2 (Phase 9) is the
//! discipline-adaptive weights layer per OUTLINE §4.4 / DECISIONS C-2 /
//! H-42 / H-43.
//!
//!   Phase 1 scaffold ✅
//!   Phase 2: db (migrations + connection pool) ✅ Plan B (deadpool-postgres + tokio-postgres)
//!   Phase 3: crypto (AES-256-GCM + HMAC-SHA256 + Argon2id) ✅ LocalKeyStore
//!   Phase 4: account (registration, login, JWT, /auth/me) ✅
//!   Phase 5: supervisor + alias_generator ✅
//!   Phase 6: rating (submit + sensitivity + P1 redaction) ✅
//!   Phase 7: aggregation + public_api ✅
//!   Phase 8: tests (unit + proptest + integration) ✅
//!   Phase 9: discipline-adaptive weights (M2) ✅
//!   Phase 10: anti-abuse + privacy (M3 — partial) ✅
//!             - aggregation filters soft_removed / is_banned (H-48)
//!             - report (举报) module: submit / claim / resolve
//!               (H-49..H-51)
//!   Phase 11: rate limiting (M3 §7.6 / E-3) ✅
//!             - per-account daily rating counter
//!             - per-IP per-minute login counter

pub mod account;
pub mod aggregation;
pub mod audit;
pub mod config;
pub mod crypto;
pub mod db;
pub mod discipline;
pub mod invitation;
pub mod lookup;
pub mod observability;
pub mod rating;
pub mod rate_limit;
pub mod report;
pub mod supervisor;

use anyhow::Result;
use axum::{extract::State, routing::get, Json, Router};
use deadpool_postgres::Pool;
use serde::Serialize;
use std::sync::Arc;
use tracing::info;

use crate::config::AppConfig;
use crate::crypto::{LocalKeyStore, SharedKeyStore};

/// Application state shared across handlers
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<AppConfig>,
    pub db: Pool,
    /// M6 — the key store is held behind a `KeyStore` trait object
    /// so a future KMS integration (AWS / Aliyun / Vault) can
    /// drop in without changing every call site. M3 used the
    /// concrete `LocalKeyStore`; the conversion to `Arc<dyn
    /// KeyStore>` is the only change required at the construction
    /// site (`lib.rs::run`).
    pub keys: SharedKeyStore,
    pub rate_limit: rate_limit::RateLimitState,
    pub audit: audit::AuditLog,
}

pub async fn run(config: AppConfig) -> Result<()> {
    // Initialize key store (parses hex keys, fails fast on bad config).
    // M6: wrap the concrete `LocalKeyStore` in a `KeyStore` trait
    // object so a future `KmsKeyStore` can drop in here without
    // changing the rest of the app.
    let local = LocalKeyStore::from_config(&config.encryption)
        .map_err(|e| anyhow::anyhow!("invalid encryption config: {e}"))?;
    info!(key_id = %local.key_id(), "LocalKeyStore initialized");
    let keys: SharedKeyStore = Arc::new(local);

    // Initialize database pool
    let db = db::build_pool(&config.database).await?;
    db::run_migrations(&db).await?;

    // Build state
    let state = AppState {
        config: Arc::new(config.clone()),
        db: db.clone(),
        keys, // already a SharedKeyStore (= Arc<dyn KeyStore>)
        rate_limit: rate_limit::RateLimitState::new(),
        audit: audit::AuditLog::new(db),
    };

    let app = build_router(state);

    let addr = format!("{}:{}", config.server.host, config.server.port);
    let listener = tokio::net::TcpListener::bind(&addr)
        .await
        .map_err(|e| anyhow::anyhow!("failed to bind {addr}: {e}"))?;

    info!(addr = %addr, "listening");

    // into_make_service_with_connect_info makes the client SocketAddr
    // available to the login handler (M3 §7.6 per-IP rate limit).
    axum::serve(
        listener,
        app.into_make_service_with_connect_info::<std::net::SocketAddr>(),
    )
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
        .nest(
            "/supervisors",
            supervisor::handler::supervisor_router()
                .merge(rating::handler::rating_router()),
        )
        .nest("/disciplines", discipline::handler::discipline_router())
        .nest("/invitations", invitation::handler::invitation_router())
        .nest("/lookup", lookup::handler::lookup_router())
        .nest("/reports", report::handler::report_router())
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
