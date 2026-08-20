//! Account / auth module — registration, login, JWT
//!
//! Phase 4 (M4) in the project plan. Implements:
//! - `POST /auth/register` — create account (email + password + discipline + institution)
//! - `POST /auth/login` — exchange credentials for an access token
//! - `GET /auth/me` — protected, returns current account
//!
//! Storage fields are encrypted per G-8 (see `src/crypto`):
//! - `email_enc`     — AES-256-GCM (P0)
//! - `email_hash`    — HMAC-SHA256 (P1, unique lookup key)
//! - `password_hash` — Argon2id PHC string
//! - `discipline_hash` / `institution_hash` — HMAC-SHA256 (P1)
//! - `grade_enc`     — AES-256-GCM (P2, optional)
//!
//! Refresh tokens and the Redis blacklist are deferred to M5; for now the
//! access token has a 15-minute TTL and is the only credential. Re-login is
//! the only way to recover from expiry (acceptable for a v0 backend).
//!
//! Rate limiting: per the `rate_limit.login_per_min` config (default 5/min).
//! For M4 this is enforced in the service layer with a simple in-memory
//! sliding window; M5 will replace it with Redis.

pub mod dto;
pub mod error;
pub mod handler;
pub mod jwt;
pub mod repo;
pub mod service;
pub mod validation;

pub use dto::{AccountResponse, AuthResponse, LoginRequest, RegisterRequest};
pub use error::AccountError;
pub use handler::{auth_router, AuthAccount};
pub use jwt::{Claims, JwtService};
pub use service::AccountService;
