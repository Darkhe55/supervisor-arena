//! Request and response DTOs for the auth API
//!
//! All wire types use `serde` with camelCase (Rust default for non-rename)
//! so JSON keys match field names directly. Validation happens in the
//! service layer, not via `validator` derive (we want explicit error
//! messages tied to `AccountError`).

use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// POST /auth/register
#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub password: String,
    /// Discipline key from the seeded `disciplines` table (e.g. "computer_science").
    pub discipline: String,
    /// Institution name as free-form text (we hash it, never index plaintext).
    pub institution: String,
    /// Optional grade label, e.g. "2024-MS" — stored AES-256-GCM encrypted.
    #[serde(default)]
    pub grade: Option<String>,
}

/// POST /auth/login
#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    pub email: String,
    pub password: String,
}

/// Response from /auth/register and /auth/login.
#[derive(Debug, Serialize)]
pub struct AuthResponse {
    pub account_id: Uuid,
    pub access_token: String,
    /// Access token lifetime in seconds (mirrors the JWT `exp` claim).
    pub expires_in: i64,
    pub tier: String,
}

/// Response from /auth/me — minimal public-safe view.
#[derive(Debug, Serialize)]
pub struct AccountResponse {
    pub account_id: Uuid,
    pub tier: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
}
