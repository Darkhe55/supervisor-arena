//! Per-IP login rate limiter.
//!
//! Per-minute window keyed by the IP address. Limit: 5/min (E-2 /
//! OUTLINE §7.6). Uses the same `WindowCounter` math as the rating
//! limiter.
//!
//! # IP source
//!
//! - First hop from `X-Forwarded-For` (we trust the load balancer
//!   in front of us, not the client). If the header is missing or
//!   malformed, fall back to the connection peer address.
//! - The M3 MVP trusts `X-Forwarded-For` as-is. In a hostile
//!   deployment you'd validate the header against a list of known
//!   proxy IPs; M5+ will add that check.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Mutex;

use chrono::{DateTime, Duration, Utc};

use super::error::RateLimitError;

const LIMIT_PER_MIN: u32 = 5;
const WINDOW: Duration = Duration::minutes(1);

#[derive(Debug, Clone, Copy)]
struct Entry {
    window_start: DateTime<Utc>,
    count: u32,
}

#[derive(Clone)]
pub struct LoginRateLimiter {
    inner: std::sync::Arc<Mutex<HashMap<String, Entry>>>,
}

impl LoginRateLimiter {
    pub fn new() -> Self {
        Self {
            inner: std::sync::Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Extract the client IP from request headers. Returns "unknown"
    /// if neither header nor peer address is parseable.
    pub fn extract_ip(
        xff: Option<&str>,
        peer: Option<SocketAddr>,
    ) -> String {
        if let Some(x) = xff {
            // Take the first hop, trim whitespace.
            let first = x.split(',').next().unwrap_or("").trim();
            if !first.is_empty() {
                return first.to_string();
            }
        }
        peer.map(|a| a.ip().to_string()).unwrap_or_else(|| "unknown".to_string())
    }

    /// Check + record. Returns `Ok(())` if allowed, `Err(RateLimited)`
    /// if the IP has exceeded the per-minute quota.
    pub fn check_and_record(&self, ip: &str) -> Result<(), RateLimitError> {
        let now = Utc::now();
        let mut map = self.inner.lock().unwrap();
        let entry = map.entry(ip.to_string()).or_insert(Entry {
            window_start: now,
            count: 0,
        });

        // Window expired → reset.
        if now - entry.window_start >= WINDOW {
            entry.window_start = now;
            entry.count = 0;
        }

        if entry.count >= LIMIT_PER_MIN {
            let elapsed = now - entry.window_start;
            let retry = (WINDOW - elapsed).num_seconds().max(1) as u64;
            return Err(RateLimitError::RateLimited {
                kind: "login_per_min",
                retry_after_secs: retry,
            });
        }
        entry.count += 1;
        Ok(())
    }

    /// Clear all counters. Used by tests.
    pub fn clear(&self) {
        self.inner.lock().unwrap().clear();
    }

    /// Read-only: how many logins from this IP in the current window.
    pub fn count_in_window(&self, ip: &str) -> u32 {
        let map = self.inner.lock().unwrap();
        let now = Utc::now();
        map.get(ip)
            .filter(|e| now - e.window_start < WINDOW)
            .map(|e| e.count)
            .unwrap_or(0)
    }
}

impl Default for LoginRateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr};

    #[test]
    fn first_call_allowed() {
        let l = LoginRateLimiter::new();
        assert!(l.check_and_record("1.2.3.4").is_ok());
    }

    #[test]
    fn blocks_after_five_per_minute() {
        let l = LoginRateLimiter::new();
        for _ in 0..5 {
            assert!(l.check_and_record("1.2.3.4").is_ok());
        }
        match l.check_and_record("1.2.3.4") {
            Err(RateLimitError::RateLimited { kind, retry_after_secs }) => {
                assert_eq!(kind, "login_per_min");
                assert!(retry_after_secs > 0);
            }
            other => panic!("expected RateLimited, got {other:?}"),
        }
    }

    #[test]
    fn different_ips_have_independent_counters() {
        let l = LoginRateLimiter::new();
        for _ in 0..5 {
            assert!(l.check_and_record("1.2.3.4").is_ok());
        }
        assert!(l.check_and_record("5.6.7.8").is_ok());
    }

    #[test]
    fn extract_ip_prefers_xff_first_hop() {
        let peer = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 1234));
        assert_eq!(
            LoginRateLimiter::extract_ip(Some("203.0.113.5, 10.0.0.1"), peer),
            "203.0.113.5"
        );
    }

    #[test]
    fn extract_ip_falls_back_to_peer() {
        let peer = Some(SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 1234));
        assert_eq!(
            LoginRateLimiter::extract_ip(None, peer),
            "10.0.0.1"
        );
    }

    #[test]
    fn extract_ip_returns_unknown_when_nothing() {
        assert_eq!(LoginRateLimiter::extract_ip(None, None), "unknown");
    }

    #[test]
    fn clear_resets_all_counters() {
        let l = LoginRateLimiter::new();
        for _ in 0..5 {
            assert!(l.check_and_record("1.2.3.4").is_ok());
        }
        assert_eq!(l.count_in_window("1.2.3.4"), 5);
        l.clear();
        assert_eq!(l.count_in_window("1.2.3.4"), 0);
    }
}
