//! Application configuration loaded from environment variables

use anyhow::{Context, Result};
use serde::Deserialize;
use std::time::Duration;

#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    pub server: ServerConfig,
    pub database: DatabaseConfig,
    pub redis: RedisConfig,
    pub auth: AuthConfig,
    pub encryption: EncryptionConfig,
    pub rate_limit: RateLimitConfig,
    pub rating: RatingLimitsConfig,
    pub alias: AliasConfig,
    pub k_anonymity: KAnonymityConfig,
    pub review: ReviewConfig,
    pub logging: LoggingConfig,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ServerConfig {
    pub host: String,
    pub port: u16,
    pub workers: usize,
}

#[derive(Debug, Clone, Deserialize)]
pub struct DatabaseConfig {
    pub url: String,
    pub max_connections: u32,
    pub min_connections: u32,
    pub acquire_timeout_secs: u64,
}

impl DatabaseConfig {
    pub fn acquire_timeout(&self) -> Duration {
        Duration::from_secs(self.acquire_timeout_secs)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct RedisConfig {
    pub url: String,
    pub pool_size: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AuthConfig {
    pub jwt_secret: String,
    pub jwt_access_ttl_secs: u64,
    pub jwt_refresh_ttl_secs: u64,
}

impl AuthConfig {
    pub fn access_ttl(&self) -> Duration {
        Duration::from_secs(self.jwt_access_ttl_secs)
    }
    pub fn refresh_ttl(&self) -> Duration {
        Duration::from_secs(self.jwt_refresh_ttl_secs)
    }
}

#[derive(Debug, Clone, Deserialize)]
pub struct EncryptionConfig {
    /// 64 hex chars (32 bytes) for AES-256-GCM
    pub field_key: String,
    /// 64 hex chars (32 bytes) for HMAC-SHA256
    pub hmac_salt_key: String,
    pub key_rotation_days: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RateLimitConfig {
    pub ratings_per_day_basic: u32,
    pub ratings_per_day_member: u32,
    pub login_per_min: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RatingLimitsConfig {
    pub max_per_supervisor_basic: u32,
    pub max_per_supervisor_member: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct AliasConfig {
    pub literary_words: u32,
    pub nature_words: u32,
    pub whitelist_refresh_days: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct KAnonymityConfig {
    pub threshold: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ReviewConfig {
    pub sla_hours_workday: u32,
    pub sla_hours_offhours: u32,
    pub mode: ReviewMode,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewMode {
    AutoPass, // M1: skip human review
    Manual,   // M7: human review
}

#[derive(Debug, Clone, Deserialize)]
pub struct LoggingConfig {
    pub format: LogFormat,
    pub retention_days: u32,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum LogFormat {
    Pretty,
    Json,
}

impl AppConfig {
    pub fn from_env() -> Result<Self> {
        let config = config::Config::builder()
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 8080)?
            .set_default("server.workers", 4)?
            .set_default("database.url", "postgres://supervisor:supervisor_dev_pwd@localhost:5432/supervisor_arena")?
            .set_default("database.max_connections", 20)?
            .set_default("database.min_connections", 2)?
            .set_default("database.acquire_timeout_secs", 5)?
            .set_default("redis.url", "redis://localhost:6379")?
            .set_default("redis.pool_size", 10)?
            .set_default("auth.jwt_secret", "dev_secret_replace_in_prod")?
            .set_default("auth.jwt_access_ttl_secs", 900)?
            .set_default("auth.jwt_refresh_ttl_secs", 604800)?
            .set_default("encryption.field_key", "dev_field_key_replace_in_prod_must_be_64_hex_chars")?
            .set_default("encryption.hmac_salt_key", "dev_hmac_key_replace_in_prod_must_be_64_hex_chars")?
            .set_default("encryption.key_rotation_days", 90)?
            .set_default("rate_limit.ratings_per_day_basic", 10)?
            .set_default("rate_limit.ratings_per_day_member", 30)?
            .set_default("rate_limit.login_per_min", 5)?
            .set_default("rating.max_per_supervisor_basic", 3)?
            .set_default("rating.max_per_supervisor_member", 10)?
            .set_default("alias.literary_words", 300)?
            .set_default("alias.nature_words", 300)?
            .set_default("alias.whitelist_refresh_days", 90)?
            .set_default("k_anonymity.threshold", 10)?
            .set_default("review.sla_hours_workday", 24)?
            .set_default("review.sla_hours_offhours", 72)?
            .set_default("review.mode", "auto_pass")?
            .set_default("logging.format", "pretty")?
            .set_default("logging.retention_days", 30)?
            .add_source(
                config::Environment::default()
                    .try_parsing(true)
                    .separator("__"),
            )
            .build()
            .context("failed to build configuration")?;

        config
            .try_deserialize::<AppConfig>()
            .context("failed to deserialize configuration")
    }
}
