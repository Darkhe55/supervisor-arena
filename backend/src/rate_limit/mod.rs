//! Rate limiting module — Phase 11 (M3 §7.6 / E-3).
//!
//! In-memory rate limiter for the M3 MVP. Two counters:
//!   - **Per-account rating counter** (daily): `basic` ≤ 10/day,
//!     `member` ≤ 30/day. Counts all `submit` calls (the supervisor
//!     module's submit too, since they share a quota).
//!   - **Per-IP login counter** (per-minute): 5/min. The IP comes
//!     from the `X-Forwarded-For` header (first hop) or the connection
//!     peer address (fallback).
//!
//! # Design notes
//!
//! - **In-memory, not Redis**: M3's MVP keeps it simple. Production
//!   would back this with Redis (or similar) so a restart doesn't
//!   reset the counters, but for the M3 milestone Redis-backed
//!   counters are deferred (M5+).
//! - **Daily reset**: the daily window is "the calendar day in UTC
//!   since the first hit". Cheap to compute, and means we don't need
//!   a background job to reset stale entries — every call already
//!   re-checks the date.
//! - **Per-minute reset**: minute-window since first hit in the
//!   current minute. Same cheap-reset pattern.
//! - **No LRU eviction**: in practice the map has at most O(active
//!   accounts in last 24h) entries; for the M3 traffic volume
//!   (a few hundred active accounts at most) this is fine. M5+ will
//!   add LRU eviction if memory becomes a concern.
//! - **Testability**: the limiter is a separate struct so tests can
//!   construct a fresh instance and call `clear()` between cases.

pub mod counter;
pub mod error;
pub mod login;
pub mod rating;

pub use counter::{RateLimitDecision, WindowCounter};
pub use error::RateLimitError;
pub use login::LoginRateLimiter;
pub use rating::RatingRateLimiter;

use std::sync::Arc;

/// Shared rate-limit state, mounted on `AppState` so handlers can
/// reach it via `State<AppState>`.
#[derive(Clone, Default)]
pub struct RateLimitState {
    pub rating: Arc<RatingRateLimiter>,
    pub login: Arc<LoginRateLimiter>,
}

impl RateLimitState {
    pub fn new() -> Self {
        Self::default()
    }

    /// Clear all counters. Used by integration tests between cases
    /// so per-test state doesn't leak.
    pub fn clear(&self) {
        self.rating.clear();
        self.login.clear();
    }
}
