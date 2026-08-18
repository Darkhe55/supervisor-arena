//! Observability setup (tracing / logging)

use anyhow::Result;
use tracing_subscriber::{fmt, prelude::*, EnvFilter};

pub fn init() {
    let env_filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

    let format = std::env::var("LOG_FORMAT").unwrap_or_else(|_| "pretty".to_string());

    let registry = tracing_subscriber::registry().with(env_filter);

    if format == "json" {
        registry
            .with(fmt::layer().json().with_target(true).with_level(true))
            .try_init()
            .ok();
    } else {
        registry
            .with(fmt::layer().pretty().with_target(true).with_level(true))
            .try_init()
            .ok();
    }
}

/// Build a tracing span for a request, with anonymized account info (P0/P1)
pub fn request_span(method: &str, path: &str) -> tracing::Span {
    tracing::info_span!("request", method = %method, path = %path)
}

#[allow(dead_code)]
pub fn setup_metrics() -> Result<()> {
    // Phase 8+: Prometheus / OpenTelemetry integration
    // For now, structured logging is sufficient
    Ok(())
}
