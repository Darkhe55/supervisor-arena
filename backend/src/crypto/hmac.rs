//! HMAC-SHA256 one-way hashing for P1 fields (school, discipline, IP, etc.)
//!
//! **Deterministic**: same input + same key → same output. This is what lets
//! the DB use the hash as a lookup index for "does an account with this email
//! already exist?" without storing the plaintext.
//!
//! **Not reversible**: the output does not leak the input. (You cannot
//! decrypt an HMAC; the best attack is brute-forcing plausible inputs.)
//!
//! **Site salt / pepper**: the HMAC key itself is the site-wide secret. All
//! P1 hashes in the system share the same key. A future improvement is
//! per-field salts (e.g. `key = site_key ‖ "email"`) to scope key rotation
//! costs — see DECISIONS.md M6 follow-ups.

use hmac::{Hmac, Mac};
use sha2::Sha256;

use super::{CryptoError, HMAC_OUT_LEN, KEY_LEN};

type HmacSha256 = Hmac<Sha256>;

/// Hash `data` with the given key. Returns 32 raw bytes.
pub fn hash_raw(key: &[u8; KEY_LEN], data: &[u8]) -> Result<[u8; HMAC_OUT_LEN], CryptoError> {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(key).map_err(|e| CryptoError::Rng(e.to_string()))?;
    mac.update(data);
    let out = mac.finalize().into_bytes();
    let mut arr = [0u8; HMAC_OUT_LEN];
    arr.copy_from_slice(&out);
    Ok(arr)
}

/// Hash `data` and return lowercase hex (64 chars). This is the canonical
/// storage form for `*_hash` columns (VARCHAR(64) or BYTEA — both work).
pub fn hash_hex(key: &[u8; KEY_LEN], data: &[u8]) -> Result<String, CryptoError> {
    let bytes = hash_raw(key, data)?;
    Ok(hex::encode(bytes))
}

/// Hash a UTF-8 string. Convenience for `email_hash`, `discipline_hash`, etc.
pub fn hash_str(key: &[u8; KEY_LEN], data: &str) -> Result<String, CryptoError> {
    hash_hex(key, data.as_bytes())
}

/// Hash with an extra per-record salt mixed in. The salt is NOT secret (it
/// can be stored in the DB row), but it ensures two users with the same
/// discipline / IP don't collide in the hash column.
pub fn hash_str_with_salt(
    key: &[u8; KEY_LEN],
    data: &str,
    salt: &[u8],
) -> Result<String, CryptoError> {
    let mut mac =
        <HmacSha256 as Mac>::new_from_slice(key).map_err(|e| CryptoError::Rng(e.to_string()))?;
    mac.update(salt);
    mac.update(data.as_bytes());
    let out = mac.finalize().into_bytes();
    Ok(hex::encode(out))
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; KEY_LEN] = [0x11_u8; KEY_LEN];

    #[test]
    fn deterministic_for_same_input() {
        let a = hash_str(&KEY, "computer science").unwrap();
        let b = hash_str(&KEY, "computer science").unwrap();
        assert_eq!(a, b);
    }

    #[test]
    fn distinct_for_different_input() {
        let a = hash_str(&KEY, "computer science").unwrap();
        let b = hash_str(&KEY, "Computer Science").unwrap();
        assert_ne!(a, b, "case must affect hash");
        let c = hash_str(&KEY, "computer_science").unwrap();
        assert_ne!(a, c, "underscore must affect hash");
    }

    #[test]
    fn different_key_yields_different_hash() {
        let a = hash_str(&KEY, "x").unwrap();
        let other = [0x22_u8; KEY_LEN];
        let b = hash_str(&other, "x").unwrap();
        assert_ne!(a, b);
    }

    #[test]
    fn hex_output_is_64_chars_lowercase() {
        let h = hash_str(&KEY, "anything").unwrap();
        assert_eq!(h.len(), 64);
        assert!(h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    #[test]
    fn salt_changes_output_but_stays_deterministic() {
        let plain = "1.2.3.4";
        let h1a = hash_str_with_salt(&KEY, plain, b"row-1").unwrap();
        let h1b = hash_str_with_salt(&KEY, plain, b"row-1").unwrap();
        let h2 = hash_str_with_salt(&KEY, plain, b"row-2").unwrap();
        assert_eq!(h1a, h1b);
        assert_ne!(h1a, h2);
    }
}
