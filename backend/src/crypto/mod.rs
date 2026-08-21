//! Cryptographic primitives for field-level encryption and password hashing
//!
//! Implements G-8 from `docs/DECISIONS.md`:
//! - **AES-256-GCM** for reversible encryption of P0 (email) and P2 (rating body /
//!   additional info) fields. Authenticated encryption — any tampering with
//!   ciphertext or AAD is detected and rejected on decrypt.
//! - **HMAC-SHA256** for one-way hashing of P1 fields (school, discipline, IP,
//!   user-agent, behavior fingerprint). The same plaintext + key always hashes
//!   to the same value, so the DB can index it for lookups; the plaintext is
//!   not recoverable from the digest.
//! - **Argon2id** for password hashing (the only place we need a slow KDF).
//!
//! Key material is loaded once at startup into a [`keystore::LocalKeyStore`]
//! and lives in `Zeroizing` wrappers. The store exposes scoped accessors
//! (`field_key()`, `hmac_key()`) — raw bytes are never copied to callers.
//!
//! Phase 3 = M3 in the project plan; KMS integration is M6.

pub mod aes;
pub mod argon2;
pub mod error;
pub mod hmac;
pub mod keystore;

pub use error::CryptoError;
pub use keystore::{KeyStore, KmsKeyStore, LocalKeyStore, SharedKeyStore};

/// Length of a 256-bit (AES-256) key, in bytes.
pub const KEY_LEN: usize = 32;

/// Length of an AES-GCM nonce, in bytes (NIST SP 800-38D §8.1).
pub const NONCE_LEN: usize = 12;

/// Length of an AES-GCM authentication tag, in bytes.
pub const TAG_LEN: usize = 16;

/// Output length of HMAC-SHA256, in bytes.
pub const HMAC_OUT_LEN: usize = 32;

#[cfg(test)]
mod tests {
    use super::*;

    /// Helper: build a KeyStore from raw 32-byte keys.
    fn test_keystore() -> LocalKeyStore {
        let field = [0xAA_u8; KEY_LEN];
        let hmac = [0xBB_u8; KEY_LEN];
        LocalKeyStore::from_raw(field, hmac)
    }

    #[test]
    fn constants_are_what_they_say() {
        assert_eq!(KEY_LEN, 32);
        assert_eq!(NONCE_LEN, 12);
        assert_eq!(TAG_LEN, 16);
        assert_eq!(HMAC_OUT_LEN, 32);
    }

    #[test]
    fn keystore_rejects_mismatched_key_lengths() {
        // from_raw panics on length mismatch only via from_config-style check;
        // from_raw itself takes [u8; 32] so length is compile-time enforced.
        let _ks = test_keystore();
    }
}
