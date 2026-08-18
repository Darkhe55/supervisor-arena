//! supervisor-arena backend entry point

use anyhow::Context;
use supervisor_arena::{config::AppConfig, run};
use tracing::info;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    // Load .env (development only; production uses real env vars)
    let _ = dotenvy::dotenv();

    // Initialize tracing
    supervisor_arena::observability::init();

    info!("supervisor-arena starting up");

    // Load configuration
    let config = AppConfig::from_env().context("failed to load configuration")?;
    info!(host = %config.server.host, port = %config.server.port, "loaded config");

    // Run the application
    run(config).await
}
