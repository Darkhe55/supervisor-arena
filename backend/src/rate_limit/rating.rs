//! Per-account rating rate limiter.
//!
//! Daily window keyed by the account's UUID. Limit is tier-dependent:
//!   - `basic`:  10 ratings/day
//!   - `member`: 30 ratings/day
//!
//! Counters are stored in an in-memory `HashMap<Uuid, (day, count)>`
//! guarded by a `parking_lot::Mutex` (or `std::sync::Mutex` if we
//! want to avoid a new dep). For the M3 MVP the access pattern is
//! one check + one increment per `submit` call, so the lock is held
//! for microseconds and contention is not a concern.

use std::collections::HashMap;
use std::sync::Mutex;

use chrono::{DateTime, Datelike, Utc};

use super::counter::{seconds_until_next_utc_midnight, RateLimitDecision};
use super::error::RateLimitError;

const LIMIT_BASIC: u32 = 10;
const LIMIT_MEMBER: u32 = 30;

#[derive(Debug, Clone, Copy)]
struct DailyEntry {
    day_key: i32, // (year * 1000 + ordinal_day) — unique per UTC day
    count: u32,
}

impl DailyEntry {
    fn new() -> Self {
        Self {
            day_key: day_key(Utc::now()),
            count: 0,
        }
    }
}

fn day_key(now: DateTime<Utc>) -> i32 {
    // year * 1000 + ordinal_day. 1000 is enough for any year (max
    // ordinal = 366, so 4 digits is enough; 1000 is a safe margin
    // and keeps the number readable).
    now.year() * 1000 + now.ordinal() as i32
}

#[derive(Clone)]
pub struct RatingRateLimiter {
    inner: std::sync::Arc<Mutex<HashMap<uuid::Uuid, DailyEntry>>>,
}

impl RatingRateLimiter {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Check + record a rating submission. Returns `Ok(())` if
    /// allowed, `Err(RateLimited { .. })` if the account has
    /// exceeded its daily quota.
    pub fn check_and_record(
        &self,
        account_id: uuid::Uuid,
        tier: &str,
    ) -> Result<(), RateLimitError> {
        let limit = match tier {
            "member" => LIMIT_MEMBER,
            // Unknown tiers fall back to basic (safe default).
            _ => LIMIT_BASIC,
        };
        let now = Utc::now();
        let today = day_key(now);

        let mut map = self.inner.lock().unwrap();
        let entry = map.entry(account_id).or_insert_with(DailyEntry::new);

        // Reset the counter if the stored entry is from a previous day.
        if entry.day_key != today {
            entry.day_key = today;
            entry.count = 0;
        }

        let decision = RateLimitDecision::Allowed { remaining: 0 };
        // Use the pure `check` helper for the limit comparison; we
        // already have the entry in hand so we don't need the full
        // WindowCounter struct here.
        if entry.count >= limit {
            return Err(RateLimitError::RateLimited {
                kind: "ratings_per_day",
                retry_after_secs: seconds_until_next_utc_midnight(now),
            });
        }
        let _ = decision; // satisfy unused-warning for the alias
        entry.count += 1;
        Ok(())
    }

    /// Clear all counters. Used by tests.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    /// Read-only: how many submissions this account has made today.
    /// Returns 0 for unknown accounts. Used by tests.
    pub fn count_today(&self, account_id: uuid::Uuid) -> u32 {
        let map = self.inner.lock().unwrap();
        map.get(&account_id)
            .filter(|e| e.day_key == day_key(Utc::now()))
            .map(|e| e.count)
            .unwrap_or(0)
    }
}

impl Default for RatingRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use uuid::Uuid;

    #[test]
    fn first_call_allowed() {
        let l = RatingRateLimiter::new();
        let id = Uuid::new_v4();
        assert!(l.check_and_record(id, "basic").is_ok());
        assert_eq!(l.count_today(id), 1);
    }

    #[test]
    fn blocks_after_basic_limit() {
        let l = RatingRateLimiter::new();
        let id = Uuid::new_v4();
        for _ in 0..10 {
            assert!(l.check_and_record(id, "basic").is_ok());
        }
        // 11th call → blocked
        match l.check_and_record(id, "basic") {
            Err(RateLimitError::RateLimited {
                kind,
                retry_after_secs,
            }) => {
                assert_eq!(kind, "ratings_per_day");
                assert!(retry_after_secs > 0);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[test]
    fn member_tier_has_higher_limit() {
        let l = RatingRateLimiter::new();
        let id = Uuid::new_v4();
        for _ in 0..10 {
            assert!(l.check_and_record(id, "member").is_ok());
        }
        // basic would be at 10/10 here; member should still be at 10/30.
        assert_eq!(l.count_today(id), 10);
        // Continue up to 30
        for _ in 0..20 {
            assert!(l.check_and_record(id, "member").is_ok());
        }
        // 31st → blocked
        assert!(l.check_and_record(id, "member").is_err());
    }

    #[test]
    fn unknown_tier_falls_back_to_basic() {
        let l = RatingRateLimiter::new();
        let id = Uuid::new_v4();
        for _ in 0..10 {
            assert!(l.check_and_record(id, "bogus_tier").is_ok());
        }
        assert!(l.check_and_record(id, "bogus_tier").is_err());
    }

    #[test]
    fn different_accounts_have_independent_counters() {
        let l = RatingRateLimiter::new();
        let a = Uuid::new_v4();
        let b = Uuid::new_v4();
        for _ in 0..10 {
            assert!(l.check_and_record(a, "basic").is_ok());
        }
        // A is at 10/10, B is at 0/10.
        assert_eq!(l.count_today(a), 10);
        assert_eq!(l.count_today(b), 0);
        assert!(l.check_and_record(b, "basic").is_ok());
    }

    #[test]
    fn clear_resets_all_counters() {
        let l = RatingRateLimiter::new();
        let id = Uuid::new_v4();
        for _ in 0..5 {
            assert!(l.check_and_record(id, "basic").is_ok());
        }
        assert_eq!(l.count_today(id), 5);
        l.clear();
        assert_eq!(l.count_today(id), 0);
    }
}
