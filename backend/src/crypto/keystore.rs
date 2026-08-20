//! Local key store — loads encryption keys from configuration and exposes
//! scoped accessors to the rest of the crypto module.
//!
//! M3: `LocalKeyStore` reads keys from `EncryptionConfig` (env-driven). The
//! hex strings are parsed once at startup; raw bytes live in `Zeroizing`
//! wrappers and are zeroed on drop.
//!
//! M6 (security hardening milestone) will replace this with a KMS-backed
//! store (`KmsKeyStore`) that wraps AWS KMS / Aliyun KMS / Vault. The trait
//! surface and call sites should not change.

use zeroize::{Zeroize, Zeroizing};

use super::error::CryptoError;
use crate::config::EncryptionConfig;

/// Local key store backed by in-process memory.
///
/// Holds two 32-byte keys:
/// - `field_key` — used by [`super::aes`] for AES-256-GCM (P0/P2 fields)
/// - `hmac_key` — used by [`super::hmac`] for HMAC-SHA256 (P1 fields)
///
/// Both keys are zeroized when the store is dropped.
#[derive(Clone)]
pub struct LocalKeyStore {
    field_key: Zeroizing<[u8; super::KEY_LEN]>,
    hmac_key: Zeroizing<[u8; super::KEY_LEN]>,
    // Track where the key material came from so we can warn at startup if a
    // dev placeholder is still in use.
    key_id: String,
}

// We don't `derive(Zeroize)` because `[u8; N]` is fine and we want explicit
// control: `Zeroizing<[u8; 32]>` zeroes the inner array on drop. (We get
// `Drop` for free from `Zeroizing`.)
//
// `Clone` is a shallow copy of the *wrappers* — the underlying bytes are
// also cloned (since `[u8; 32]` is `Copy`), but `Zeroizing` ensures the
// drop semantics. For a KeyStore, a single instance should be created and
// `Arc`'d via `AppState`; cloning the KeyStore itself is only useful for
// tests.

impl LocalKeyStore {
    /// Build a KeyStore from hex-encoded key strings.
    ///
    /// Both keys must be 64 hex characters (= 32 bytes). The key_id is set
    /// to a short string for logging / startup banner.
    pub fn from_config(config: &EncryptionConfig) -> Result<Self, CryptoError> {
        let field_key = Self::decode_hex_key(&config.field_key, "field_key")?;
        let hmac_key = Self::decode_hex_key(&config.hmac_salt_key, "hmac_salt_key")?;

        // Detect the "dev placeholder" so we can warn the operator.
        // The convention is: any key starting with `deadbeef` (the classic
        // "this is a placeholder" sentinel in hex) is treated as dev-only.
        // Production keys should be generated with `openssl rand -hex 32` and
        // will not start with this prefix by chance.
        let key_id = if config.field_key.starts_with("deadbeef")
            || config.hmac_salt_key.starts_with("deadbeef")
        {
            tracing::warn!(
                "LocalKeyStore is using dev placeholder keys (deadbeef* prefix) — DO NOT deploy to production"
            );
            "dev-placeholder".to_string()
        } else {
            // Production: use a short fingerprint of the field key as the id
            // (first 4 bytes hex) so logs can correlate "which key version
            // encrypted this row" without leaking the key.
            let fp = hex::encode(&field_key[..4]);
            format!("local:{fp}")
        };

        Ok(Self {
            field_key: Zeroizing::new(field_key),
            hmac_key: Zeroizing::new(hmac_key),
            key_id,
        })
    }

    /// Build a KeyStore directly from raw 32-byte keys. Used in tests.
    pub fn from_raw(field_key: [u8; super::KEY_LEN], hmac_key: [u8; super::KEY_LEN]) -> Self {
        Self {
            field_key: Zeroizing::new(field_key),
            hmac_key: Zeroizing::new(hmac_key),
            key_id: "raw".to_string(),
        }
    }

    /// Borrow the AES field key.
    pub fn field_key(&self) -> &[u8; super::KEY_LEN] {
        &self.field_key
    }

    /// Borrow the HMAC key.
    pub fn hmac_key(&self) -> &[u8; super::KEY_LEN] {
        &self.hmac_key
    }

    /// A short identifier for this key material (suitable for logging /
    /// tagging audit-log rows). Does NOT leak the key.
    pub fn key_id(&self) -> &str {
        &self.key_id
    }

    fn decode_hex_key(hex_str: &str, name: &str) -> Result<[u8; super::KEY_LEN], CryptoError> {
        if hex_str.len() != super::KEY_LEN * 2 {
            return Err(CryptoError::InvalidKeyLength {
                expected: super::KEY_LEN,
                actual: hex_str.len() / 2,
            });
        }
        let bytes = hex::decode(hex_str)?;
        if bytes.len() != super::KEY_LEN {
            return Err(CryptoError::InvalidKeyLength {
                expected: super::KEY_LEN,
                actual: bytes.len(),
            });
        }
        let mut arr = [0u8; super::KEY_LEN];
        arr.copy_from_slice(&bytes);
        // Best-effort: scrub the intermediate Vec<u8> from `hex::decode`.
        // We can't actually do this — `hex::decode` already returned owned
        // bytes, and we copied them out. Documenting that limitation: the
        // transient allocation of `bytes` will be zeroed by the allocator on
        // reuse, but not deterministically. For true defense-in-depth, use
        // a zeroize-aware hex crate (e.g. `subtle` + manual parsing).
        let _ = name; // currently used only for error context (future)
        Ok(arr)
    }
}

// `Zeroizing<[u8; 32]>` zeroes on drop. We also implement `Drop` explicitly
// to scrub a second time in case the optimizer elided the inline zeroize.
impl Drop for LocalKeyStore {
    fn drop(&mut self) {
        self.field_key.zeroize();
        self.hmac_key.zeroize();
    }
}

impl std::fmt::Debug for LocalKeyStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LocalKeyStore")
            .field("key_id", &self.key_id)
            .finish_non_exhaustive()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_config() -> EncryptionConfig {
        EncryptionConfig {
            field_key: "a".repeat(64),
            hmac_salt_key: "b".repeat(64),
            key_rotation_days: 90,
        }
    }

    #[test]
    fn from_config_succeeds_with_valid_hex() {
        let ks = LocalKeyStore::from_config(&sample_config()).unwrap();
        assert_eq!(ks.field_key()[0], 0xAA);
        assert_eq!(ks.hmac_key()[0], 0xBB);
    }

    #[test]
    fn rejects_wrong_length_hex() {
        let bad = EncryptionConfig {
            field_key: "abcd".into(),
            hmac_salt_key: "c".repeat(64),
            key_rotation_days: 90,
        };
        let err = LocalKeyStore::from_config(&bad).unwrap_err();
        match err {
            CryptoError::InvalidKeyLength { expected: 32, actual: 2 } => {}
            other => panic!("expected InvalidKeyLength, got {other:?}"),
        }
    }

    #[test]
    fn rejects_non_hex_string() {
        let bad = EncryptionConfig {
            field_key: "z".repeat(64), // 'z' is not hex
            hmac_salt_key: "0".repeat(64),
            key_rotation_days: 90,
        };
        let err = LocalKeyStore::from_config(&bad).unwrap_err();
        assert!(matches!(err, CryptoError::InvalidHexKey(_)));
    }

    #[test]
    fn dev_placeholder_sets_key_id() {
        // 64-hex-char string starting with `deadbeef` — universal placeholder
        // convention. Valid hex chars are 0-9 a-f, so `deadbeef` itself is
        // valid and recognizable.
        let placeholder = "deadbeef".repeat(8); // 64 chars, all valid hex
        assert_eq!(placeholder.len(), 64);
        let cfg = EncryptionConfig {
            field_key: placeholder.clone(),
            hmac_salt_key: "0".repeat(64),
            key_rotation_days: 90,
        };
        let ks = LocalKeyStore::from_config(&cfg).unwrap();
        assert_eq!(ks.key_id(), "dev-placeholder");
    }

    #[test]
    fn production_keys_get_fingerprint_key_id() {
        // Random-looking 64-hex-char key, doesn't start with "dev_".
        let cfg = EncryptionConfig {
            field_key: "f".repeat(64),
            hmac_salt_key: "0".repeat(64),
            key_rotation_days: 90,
        };
        let ks = LocalKeyStore::from_config(&cfg).unwrap();
        assert!(ks.key_id().starts_with("local:"));
        assert_eq!(ks.key_id().len(), "local:".len() + 8); // 4 bytes hex
    }

    #[test]
    fn debug_redacts_keys() {
        let ks = LocalKeyStore::from_raw([0xAB; 32], [0xCD; 32]);
        let dbg = format!("{ks:?}");
        // Must NOT contain raw key bytes.
        assert!(!dbg.contains("ab"));
        assert!(!dbg.contains("ABABABAB"));
        // Should show key_id and the struct name.
        assert!(dbg.contains("LocalKeyStore"));
    }
}
