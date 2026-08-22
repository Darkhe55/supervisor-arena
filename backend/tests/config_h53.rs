//! H-53 integration tests: `AppConfig::from_env()` must fail-closed when
//! any of the 5 sensitive env vars are missing or malformed.
//!
//! Uses `serial_test` because env vars are process-global — concurrent tests
//! would race.

use serial_test::serial;
use supervisor_arena::config::AppConfig;

const VALID_JWT: &str = "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef";
const VALID_FIELD_KEY: &str = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef";
const VALID_HMAC_KEY: &str = "cafebabecafebabecafebabecafebabecafebabecafebabecafebabecafebabe";
const VALID_DB_URL: &str = "postgres://supervisor:dev_pwd@localhost:5433/supervisor_arena";
const VALID_REDIS_URL: &str = "redis://localhost:6379";

const SENSITIVE_VARS: &[&str] = &[
    "DATABASE__URL",
    "REDIS__URL",
    "AUTH__JWT_SECRET",
    "ENCRYPTION__FIELD_KEY",
    "ENCRYPTION__HMAC_SALT_KEY",
];

fn set_valid_env() {
    std::env::set_var("DATABASE__URL", VALID_DB_URL);
    std::env::set_var("REDIS__URL", VALID_REDIS_URL);
    std::env::set_var("AUTH__JWT_SECRET", VALID_JWT);
    std::env::set_var("ENCRYPTION__FIELD_KEY", VALID_FIELD_KEY);
    std::env::set_var("ENCRYPTION__HMAC_SALT_KEY", VALID_HMAC_KEY);
}

fn clear_sensitive_env() {
    for var in SENSITIVE_VARS {
        std::env::remove_var(var);
    }
}

/// anyhow::Error::to_string() only returns the outermost context. To see the
/// inner bail!() messages (which is what we want to assert on) we walk the
/// chain with `{:#}` format or `chain().collect()`.
fn full_error(err: &anyhow::Error) -> String {
    err.chain()
        .map(|e| e.to_string())
        .collect::<Vec<_>>()
        .join(" | ")
}

#[test]
#[serial]
fn from_env_succeeds_when_all_5_sensitive_vars_present() {
    set_valid_env();
    let result = AppConfig::from_env();
    clear_sensitive_env();
    let cfg = result.expect("valid env should produce a config");
    assert_eq!(cfg.database.url, VALID_DB_URL);
    assert_eq!(cfg.auth.jwt_secret, VALID_JWT);
}

#[test]
#[serial]
fn from_env_fails_when_database_url_missing() {
    set_valid_env();
    std::env::remove_var("DATABASE__URL");
    let result = AppConfig::from_env();
    clear_sensitive_env();
    let err = result.expect_err("missing DATABASE__URL must fail");
    let msg = full_error(&err);
    assert!(
        msg.contains("DATABASE__URL") || msg.contains("missing"),
        "expected DATABASE__URL or missing-field error, got: {msg}"
    );
}

#[test]
#[serial]
fn from_env_fails_when_redis_url_missing() {
    set_valid_env();
    std::env::remove_var("REDIS__URL");
    let result = AppConfig::from_env();
    clear_sensitive_env();
    assert!(result.is_err(), "missing REDIS__URL must fail");
}

#[test]
#[serial]
fn from_env_fails_when_jwt_secret_missing() {
    set_valid_env();
    std::env::remove_var("AUTH__JWT_SECRET");
    let result = AppConfig::from_env();
    clear_sensitive_env();
    let err = result.expect_err("missing JWT secret must fail");
    let msg = full_error(&err);
    assert!(
        msg.contains("AUTH__JWT_SECRET") || msg.contains("missing"),
        "got: {msg}"
    );
}

#[test]
#[serial]
fn from_env_fails_when_field_key_missing() {
    set_valid_env();
    std::env::remove_var("ENCRYPTION__FIELD_KEY");
    let result = AppConfig::from_env();
    clear_sensitive_env();
    assert!(result.is_err(), "missing field_key must fail");
}

#[test]
#[serial]
fn from_env_fails_when_hmac_salt_key_missing() {
    set_valid_env();
    std::env::remove_var("ENCRYPTION__HMAC_SALT_KEY");
    let result = AppConfig::from_env();
    clear_sensitive_env();
    assert!(result.is_err(), "missing hmac_salt_key must fail");
}

#[test]
#[serial]
fn from_env_fails_when_all_sensitive_vars_missing() {
    clear_sensitive_env();
    let result = AppConfig::from_env();
    assert!(result.is_err(), "all 5 missing must fail");
}

#[test]
#[serial]
fn from_env_fails_when_jwt_secret_too_short() {
    set_valid_env();
    std::env::set_var("AUTH__JWT_SECRET", "tooshort"); // 8 bytes
    let result = AppConfig::from_env();
    clear_sensitive_env();
    let err = result.expect_err("short JWT must fail");
    let msg = full_error(&err);
    assert!(msg.contains("32 bytes"), "expected length check, got: {msg}");
}

#[test]
#[serial]
fn from_env_fails_when_field_key_wrong_length() {
    set_valid_env();
    std::env::set_var("ENCRYPTION__FIELD_KEY", "abcd"); // 4 chars, not 64
    let result = AppConfig::from_env();
    clear_sensitive_env();
    let err = result.expect_err("short key must fail");
    let msg = full_error(&err);
    assert!(msg.contains("64 hex chars"), "got: {msg}");
}

#[test]
#[serial]
fn from_env_fails_when_field_key_invalid_hex() {
    set_valid_env();
    // 64 chars but contains 'g'
    std::env::set_var("ENCRYPTION__FIELD_KEY", &"g".repeat(64));
    let result = AppConfig::from_env();
    clear_sensitive_env();
    let err = result.expect_err("non-hex key must fail");
    let msg = full_error(&err);
    assert!(msg.contains("valid hex"), "got: {msg}");
}

#[test]
#[serial]
fn from_env_succeeds_with_dev_placeholder_values_from_dotenv() {
    // The .env.example has these "dev placeholders":
    //   AUTH__JWT_SECRET=replace_with_64_byte_hex_string_at_least_64_chars_long_for_dev_only
    //   ENCRYPTION__FIELD_KEY=deadbeef... (64 hex chars)
    //   ENCRYPTION__HMAC_SALT_KEY=deadbeef...cafebabe... (64 hex chars)
    // H-53 doesn't reject placeholders — it just requires *non-empty* + format-valid
    // values. Detection of "is this actually a dev placeholder" is H-59's job
    // (LocalKeyStore warns on `deadbeef` prefix).
    set_valid_env();
    std::env::set_var(
        "AUTH__JWT_SECRET",
        "replace_with_64_byte_hex_string_at_least_64_chars_long_for_dev_only",
    );
    let result = AppConfig::from_env();
    clear_sensitive_env();
    assert!(
        result.is_ok(),
        "dev placeholder values pass H-53's format check: {:?}",
        result.err().map(|e| full_error(&e))
    );
}

