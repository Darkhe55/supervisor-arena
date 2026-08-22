//! Application configuration loaded from environment variables
//!
//! **H-53 fail-closed**: 5 sensitive fields have **no** `set_default` fallback —
//! `database.url`, `redis.url`, `auth.jwt_secret`, `encryption.field_key`,
//! `encryption.hmac_salt_key`. If any of those env vars are missing, startup
//! fails fast with a clear error. Dev placeholders live in `.env.example`
//! (loaded by `dotenvy` in `main.rs`); production must inject real values.

use anyhow::{bail, Context, Result};
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
            // Non-sensitive server / pool / TTL / rate-limit defaults.
            // Sensitive fields (DB URL, Redis URL, JWT secret, encryption keys)
            // have NO fallback — see H-53. They must come from env / .env file.
            .set_default("server.host", "0.0.0.0")?
            .set_default("server.port", 8080)?
            .set_default("server.workers", 4)?
            .set_default("database.max_connections", 20)?
            .set_default("database.min_connections", 2)?
            .set_default("database.acquire_timeout_secs", 5)?
            .set_default("redis.pool_size", 10)?
            .set_default("auth.jwt_access_ttl_secs", 900)?
            .set_default("auth.jwt_refresh_ttl_secs", 604800)?
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

        let cfg: AppConfig = config
            .try_deserialize::<AppConfig>()
            .context("failed to deserialize configuration (missing required env vars?)")?;

        cfg.validate().context("config validation failed")?;
        Ok(cfg)
    }

    /// H-53: explicit post-build validation. Sensitive fields must be present
    /// and meet length / format requirements. Rejects empty strings, short
    /// JWT secrets, and non-hex / wrong-length encryption keys.
    pub fn validate(&self) -> Result<()> {
        if self.database.url.trim().is_empty() {
            bail!("DATABASE__URL is required (H-53: no default for sensitive fields)");
        }
        if self.redis.url.trim().is_empty() {
            bail!("REDIS__URL is required (H-53: no default for sensitive fields)");
        }
        if self.auth.jwt_secret.is_empty() {
            bail!("AUTH__JWT_SECRET is required (H-53: no default for sensitive fields)");
        }
        if self.auth.jwt_secret.len() < 32 {
            bail!(
                "AUTH__JWT_SECRET must be at least 32 bytes (got {}). \
                 Generate with: openssl rand -hex 32",
                self.auth.jwt_secret.len()
            );
        }
        if self.encryption.field_key.is_empty() {
            bail!("ENCRYPTION__FIELD_KEY is required (H-53: no default for sensitive fields)");
        }
        if self.encryption.hmac_salt_key.is_empty() {
            bail!("ENCRYPTION__HMAC_SALT_KEY is required (H-53: no default for sensitive fields)");
        }
        validate_hex_key(&self.encryption.field_key, "ENCRYPTION__FIELD_KEY")?;
        validate_hex_key(&self.encryption.hmac_salt_key, "ENCRYPTION__HMAC_SALT_KEY")?;
        Ok(())
    }
}

/// 32-byte AES-256 / HMAC-SHA256 key, hex-encoded → 64 ASCII hex chars.
fn validate_hex_key(value: &str, name: &str) -> Result<()> {
    if value.len() != 64 {
        bail!(
            "{} must be exactly 64 hex chars (32 bytes); got {} chars. \
             Generate with: openssl rand -hex 32",
            name,
            value.len()
        );
    }
    if hex::decode(value).is_err() {
        bail!("{} must be valid hex (0-9 a-f A-F); got invalid chars", name);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    //! H-53: config fail-closed validation tests.
    //!
    //! We test `validate()` directly (no env pollution). The env-driven
    //! `from_env()` path is covered by integration tests in `tests/`.

    use super::*;
    use crate::config::AppConfig;

    fn good_config() -> AppConfig {
        AppConfig {
            server: ServerConfig {
                host: "0.0.0.0".into(),
                port: 8080,
                workers: 4,
            },
            database: DatabaseConfig {
                url: "postgres://u:p@localhost:5432/db".into(),
                max_connections: 20,
                min_connections: 2,
                acquire_timeout_secs: 5,
            },
            redis: RedisConfig {
                url: "redis://localhost:6379".into(),
                pool_size: 10,
            },
            auth: AuthConfig {
                jwt_secret: "a".repeat(64),
                jwt_access_ttl_secs: 900,
                jwt_refresh_ttl_secs: 604800,
            },
            encryption: EncryptionConfig {
                field_key: "a".repeat(64),
                hmac_salt_key: "b".repeat(64),
                key_rotation_days: 90,
            },
            rate_limit: RateLimitConfig {
                ratings_per_day_basic: 10,
                ratings_per_day_member: 30,
                login_per_min: 5,
            },
            rating: RatingLimitsConfig {
                max_per_supervisor_basic: 3,
                max_per_supervisor_member: 10,
            },
            alias: AliasConfig {
                literary_words: 300,
                nature_words: 300,
                whitelist_refresh_days: 90,
            },
            k_anonymity: KAnonymityConfig { threshold: 10 },
            review: ReviewConfig {
                sla_hours_workday: 24,
                sla_hours_offhours: 72,
                mode: ReviewMode::AutoPass,
            },
            logging: LoggingConfig {
                format: LogFormat::Pretty,
                retention_days: 30,
            },
        }
    }

    #[test]
    fn validate_ok_on_healthy_config() {
        assert!(good_config().validate().is_ok());
    }

    #[test]
    fn validate_rejects_empty_database_url() {
        let mut c = good_config();
        c.database.url = String::new();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("DATABASE__URL"), "got: {err}");
        assert!(err.contains("H-53"), "must reference H-53: {err}");
    }

    #[test]
    fn validate_rejects_whitespace_only_database_url() {
        let mut c = good_config();
        c.database.url = "   \t\n".into();
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_rejects_empty_redis_url() {
        let mut c = good_config();
        c.redis.url = String::new();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("REDIS__URL"));
    }

    #[test]
    fn validate_rejects_empty_jwt_secret() {
        let mut c = good_config();
        c.auth.jwt_secret = String::new();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("AUTH__JWT_SECRET"));
    }

    #[test]
    fn validate_rejects_short_jwt_secret() {
        let mut c = good_config();
        c.auth.jwt_secret = "short".into(); // 5 bytes
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("at least 32 bytes"), "got: {err}");
        assert!(err.contains("got 5"), "must report actual length: {err}");
    }

    #[test]
    fn validate_accepts_exactly_32_byte_jwt_secret() {
        let mut c = good_config();
        c.auth.jwt_secret = "a".repeat(32);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn validate_rejects_empty_field_key() {
        let mut c = good_config();
        c.encryption.field_key = String::new();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("ENCRYPTION__FIELD_KEY"));
    }

    #[test]
    fn validate_rejects_empty_hmac_salt_key() {
        let mut c = good_config();
        c.encryption.hmac_salt_key = String::new();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("ENCRYPTION__HMAC_SALT_KEY"));
    }

    #[test]
    fn validate_rejects_short_field_key() {
        let mut c = good_config();
        c.encryption.field_key = "a".repeat(63);
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("64 hex chars"), "got: {err}");
    }

    #[test]
    fn validate_rejects_long_field_key() {
        let mut c = good_config();
        c.encryption.field_key = "a".repeat(65);
        assert!(c.validate().is_err());
    }

    #[test]
    fn validate_rejects_non_hex_field_key() {
        let mut c = good_config();
        // 64 chars but contains 'g' which is not hex
        c.encryption.field_key = "g".repeat(64);
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("valid hex"), "got: {err}");
    }

    #[test]
    fn validate_accepts_uppercase_hex_field_key() {
        let mut c = good_config();
        c.encryption.field_key = "DEADBEEF".repeat(8); // 64 hex chars
        c.encryption.hmac_salt_key = "CAFEBABE".repeat(8);
        assert!(c.validate().is_ok());
    }

    #[test]
    fn validate_reports_first_error_only() {
        // Multiple bad fields — we report the first one we hit.
        // The ordering is fixed (DB → Redis → JWT → field_key → hmac_salt_key)
        // so this is a stable contract test.
        let mut c = good_config();
        c.database.url = String::new();
        c.auth.jwt_secret = String::new();
        let err = c.validate().unwrap_err().to_string();
        assert!(err.contains("DATABASE__URL"), "expected first error, got: {err}");
    }
}
