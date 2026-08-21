//! Helpers for building the optional `ip_hash` field of an audit
//! access event. The M6 MVP doesn't plumb the IP into every
//! handler (would require passing `ConnectInfo<SocketAddr>` and
//! `HeaderMap` through the service layer), so the `ip_hash` is
//! almost always `None` for now. The helper is here so a future
//! commit can drop it in.

use std::net::SocketAddr;

use crate::crypto::hmac;

/// Compute the IP hash for an audit log entry, or `None` if the
/// IP cannot be determined (no XFF header, no peer address).
pub fn ip_hash_from(
    xff: Option<&str>,
    peer: Option<SocketAddr>,
    hmac_key: &[u8; 32],
) -> Option<Vec<u8>> {
    // XFF first hop wins; fall through to peer if XFF is missing
    // OR the first hop is empty after trim.
    let ip = xff
        .and_then(|h| h.split(',').next())
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
        .or_else(|| peer.map(|a| a.ip().to_string()))?;
    hmac::hash_str(hmac_key, &ip).ok().map(|s| s.into_bytes())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    fn key() -> [u8; 32] {
        [0x42u8; 32]
    }

    #[test]
    fn returns_none_when_no_inputs() {
        assert!(ip_hash_from(None, None, &key()).is_none());
    }

    #[test]
    fn extracts_first_xff_hop() {
        let h = ip_hash_from(Some("203.0.113.5, 10.0.0.2"), None, &key());
        assert!(h.is_some());
        // Same input → same hash.
        let h2 = ip_hash_from(Some("203.0.113.5, 10.0.0.2"), None, &key());
        assert_eq!(h, h2);
    }

    #[test]
    fn different_xff_first_hop_yields_different_hash() {
        let a = ip_hash_from(Some("203.0.113.5"), None, &key()).unwrap();
        let b = ip_hash_from(Some("198.51.100.7"), None, &key()).unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn falls_back_to_peer_when_xff_missing() {
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 1234);
        let from_xff = ip_hash_from(Some("203.0.113.5"), Some(peer), &key());
        let from_peer = ip_hash_from(None, Some(peer), &key());
        // XFF wins (first source).
        assert_eq!(from_xff, ip_hash_from(Some("203.0.113.5"), None, &key()));
        // No XFF → peer.
        assert_eq!(from_peer, ip_hash_from(None, Some(peer), &key()));
    }

    #[test]
    fn empty_xff_falls_through_to_peer() {
        let peer = SocketAddr::new(IpAddr::V4(Ipv4Addr::new(10, 0, 0, 1)), 1234);
        let h = ip_hash_from(Some(""), Some(peer), &key());
        assert_eq!(h, ip_hash_from(None, Some(peer), &key()));
    }
}

