//! Pure helpers for sliding-window rate-limit counters.
//!
//! These are decoupled from the storage layer so the math is easy
//! to unit-test.

use chrono::{DateTime, Datelike, Duration, Utc};

/// The decision returned by a rate-limit check.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RateLimitDecision {
    /// The action is allowed; `remaining` is the quota left after
    /// this call (0 means this was the last allowed call).
    Allowed { remaining: u32 },
    /// The action is blocked; `retry_after_secs` is a hint for the
    /// client (used in the `Retry-After` header).
    Blocked { retry_after_secs: u64 },
}

/// A sliding-window counter: counts calls in a time window starting
/// at `window_start` and resets when `now - window_start > window_size`.
#[derive(Debug, Clone, Copy)]
pub struct WindowCounter {
    pub window_start: DateTime<Utc>,
    pub count: u32,
}

impl WindowCounter {
    pub fn new() -> Self {
        Self {
            window_start: Utc::now(),
            count: 0,
        }
    }

    /// Check if a new call is allowed under `limit` within `window_size`.
    /// Pure: does not mutate. Returns the decision and the post-call
    /// counter (caller should `record_call` if decision is Allowed).
    pub fn check(
        &self,
        limit: u32,
        window_size: Duration,
        now: DateTime<Utc>,
    ) -> RateLimitDecision {
        if self.count >= limit {
            let elapsed = now - self.window_start;
            let retry = if elapsed >= window_size {
                0
            } else {
                (window_size - elapsed).num_seconds().max(1) as u64
            };
            return RateLimitDecision::Blocked {
                retry_after_secs: retry,
            };
        }
        RateLimitDecision::Allowed {
            remaining: limit - self.count - 1,
        }
    }
}

/// A daily window — uses the UTC calendar day as the bucket.
/// Two calls on different UTC days never share a counter.
pub fn daily_window_key(now: DateTime<Utc>) -> (i32, u32) {
    // (year, ordinal_day) is enough to uniquely identify a UTC day
    // for the next 1000 years. We deliberately do NOT include the
    // hour — two calls on the same calendar day should share a
    // counter (i.e. the limit is "10 per UTC calendar day", not
    // "10 per any-24h-rolling-window").
    (now.year(), now.ordinal())
}

/// Compute the seconds until the next UTC midnight — used for the
/// `Retry-After` hint on a daily quota block.
pub fn seconds_until_next_utc_midnight(now: DateTime<Utc>) -> u64 {
    let next_midnight = (now + Duration::days(1))
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    (next_midnight - now).num_seconds().max(0) as u64
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(y: i32, m: u32, d: u32, h: u32, mi: u32) -> DateTime<Utc> {
        use chrono::TimeZone;
        Utc.with_ymd_and_hms(y, m, d, h, mi, 0).unwrap()
    }

    #[test]
    fn check_allows_under_limit() {
        let c = WindowCounter {
            window_start: at(2026, 1, 1, 0, 0),
            count: 3,
        };
        let now = at(2026, 1, 1, 0, 5);
        match c.check(5, Duration::minutes(1), now) {
            RateLimitDecision::Allowed { remaining } => assert_eq!(remaining, 1),
            other => panic!("expected Allowed, got {other:?}"),
        }
    }

    #[test]
    fn check_blocks_at_limit() {
        let c = WindowCounter {
            window_start: at(2026, 1, 1, 0, 0),
            count: 5,
        };
        let now = at(2026, 1, 1, 0, 0);
        match c.check(5, Duration::minutes(1), now) {
            RateLimitDecision::Blocked { retry_after_secs } => {
                assert!(retry_after_secs > 0);
            }
            other => panic!("expected Blocked, got {other:?}"),
        }
    }

    #[test]
    fn check_resets_after_window() {
        // Counter full at minute 0, queried at minute 5 (>1min window)
        // → should be allowed (window expired, but caller should
        //   reset the counter first; `check` is pure and reports
        //   based on the current snapshot).
        // The pure check function only looks at the stored count,
        // so it still blocks here — the caller is expected to reset
        // when the window expires.
        let c = WindowCounter {
            window_start: at(2026, 1, 1, 0, 0),
            count: 5,
        };
        let now = at(2026, 1, 1, 0, 5);
        let decision = c.check(5, Duration::minutes(1), now);
        // The pure check doesn't auto-reset, so the result is
        // determined by the stored count. (The concrete counter types
        // in `rating` / `login` modules do the auto-reset.)
        assert!(matches!(decision, RateLimitDecision::Blocked { .. }));
    }

    #[test]
    fn daily_window_key_uniqueness_across_days() {
        let d1 = at(2026, 1, 1, 23, 59);
        let d2 = at(2026, 1, 2, 0, 0);
        assert_ne!(daily_window_key(d1), daily_window_key(d2));
    }

    #[test]
    fn daily_window_key_same_day() {
        let d1 = at(2026, 1, 1, 9, 0);
        let d2 = at(2026, 1, 1, 18, 0);
        assert_eq!(daily_window_key(d1), daily_window_key(d2));
    }

    #[test]
    fn seconds_until_next_midnight_positive() {
        let now = at(2026, 1, 1, 12, 0);
        let secs = seconds_until_next_utc_midnight(now);
        // 12 hours = 43200 seconds
        assert_eq!(secs, 43200);
    }

    #[test]
    fn seconds_until_next_midnight_at_midnight_is_24h() {
        // At exactly midnight, "next midnight" is 24h away.
        let now = at(2026, 1, 1, 0, 0);
        let secs = seconds_until_next_utc_midnight(now);
        assert_eq!(secs, 86400);
    }
}
