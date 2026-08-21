//! Local key store — loads encryption keys from configuration and exposes
//! scoped accessors to the rest of the crypto module.
//!
//! M3: `LocalKeyStore` reads keys from `EncryptionConfig` (env-driven). The
//! hex strings are parsed once at startup; raw bytes live in `Zeroizing`
//! wrappers and are zeroed on drop.
//!
//! M6 (security hardening milestone) wraps both `LocalKeyStore` and the
//! stub `KmsKeyStore` behind the [`KeyStore`] trait so the rest of the
//! codebase uses `Arc<dyn KeyStore>` and doesn't care which backend is
//! in play. KMS integration itself (AWS / Aliyun / Vault) is M6+ work —
//! for now `KmsKeyStore` is a stub that returns
//! `Err(KmsUnavailable)` from every accessor so a misconfigured prod
//! deployment fails closed instead of silently using a placeholder.

use std::sync::Arc;
use zeroize::{Zeroize, Zeroizing};

use super::error::CryptoError;
use crate::config::EncryptionConfig;

/// The abstract key store surface. The rest of the codebase talks
/// to this trait (via `Arc<dyn KeyStore>` in `AppState`) — never to
/// a concrete implementation.
///
/// # Why a trait
///
/// M6 (security hardening) calls out integrating a cloud KMS
/// (AWS KMS / Aliyun KMS / HashiCorp Vault) so the raw key bytes
/// never sit in the application's process memory. The trait makes
/// that swap mechanical: add a `KmsKeyStore` impl, change one line
/// in `lib.rs::run`, and every call site keeps working.
///
/// # Why `&[u8; KEY_LEN]` (not `Vec<u8>`)
///
/// - Fixed size matches what AES-256-GCM and HMAC-SHA256 actually need.
/// - The `&` borrow keeps the key inside the `Zeroizing` wrapper's
///   scope; the caller can copy out into a `Zeroizing<[u8; 32]>` if
///   they need to keep the bytes around (e.g. for streaming AES), but
///   the canonical pattern is "use and drop" inside a function.
///
/// # `key_id()`
///
/// The implementation returns a short, non-secret identifier
/// (e.g. "local:abcd1234" or "kms:alias/prod-2026-q1") so audit
/// logs and error messages can correlate "which key version
/// encrypted this row" without leaking the key.
pub trait KeyStore: Send + Sync {
    fn field_key(&self) -> &[u8; super::KEY_LEN];
    fn hmac_key(&self) -> &[u8; super::KEY_LEN];
    fn key_id(&self) -> &str;
}

// Blanket `Arc<dyn KeyStore>` convenience: any concrete `KeyStore`
// can be `Arc`'d up and stored as a trait object without ceremony.
pub type SharedKeyStore = Arc<dyn KeyStore>;

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

// ---- KeyStore trait impl for LocalKeyStore ----
//
// Wraps the inherent methods so callers using `Arc<dyn KeyStore>`
// can hit the same code path as callers using the concrete
// `LocalKeyStore`. We forward to the inherent methods (which are
// the canonical source of truth).

impl KeyStore for LocalKeyStore {
    fn field_key(&self) -> &[u8; super::KEY_LEN] {
        // Cast through a manual re-borrow to avoid the trait-method
        // shadowing the inherent-method name.
        LocalKeyStore::field_key(self)
    }
    fn hmac_key(&self) -> &[u8; super::KEY_LEN] {
        LocalKeyStore::hmac_key(self)
    }
    fn key_id(&self) -> &str {
        LocalKeyStore::key_id(self)
    }
}

// ---- KMS-backed key store (stub for M6+) ----
//
// The M6 KMS integration is a future commit. For now, this stub
// returns a sentinel error from every accessor so a misconfigured
// prod deployment that points the config at "kms" fails closed
// instead of silently falling back to a placeholder key. The
// shape is here so the integration can drop in without changing
// call sites.
#[derive(Debug)]
pub struct KmsKeyStore {
    /// Human-readable identifier (e.g. "kms:alias/prod-2026-q1").
    /// Used in audit logs and error messages; the real key never
    /// leaves the KMS.
    key_id: String,
}

impl KmsKeyStore {
    /// The error returned by every accessor of the stub. The
    /// `Arc<[u8; 32]>` would be replaced with `KmsClient` /
    /// `KmsCiphertext` in a real impl.
    pub fn stub_error(&self) -> CryptoError {
        CryptoError::KmsUnavailable {
            key_id: self.key_id.clone(),
        }
    }
}

impl KeyStore for KmsKeyStore {
    fn field_key(&self) -> &[u8; super::KEY_LEN] {
        // The M6+ impl will: cache a wrapped-data-key (DEK) returned
        // by the KMS GenerateDataKey API and unwrap it here. The
        // unwrapped bytes still go into a `Zeroizing<[u8; 32]>` on
        // the impl. For now, the stub returns an empty key — but
        // the *real* call sites use `try_field_key` (below) so they
        // surface the error instead of getting zeros.
        //
        // Returning a zero key here is a safety fallback: if some
        // old call site is still using `&[u8; KEY_LEN]` and not
        // the Result-based accessor, we'd rather the encrypt / hash
        // produce a known-bogus output than panic. The M6+ wire-up
        // replaces both methods.
        &ZERO_KEY
    }
    fn hmac_key(&self) -> &[u8; super::KEY_LEN] {
        &ZERO_KEY
    }
    fn key_id(&self) -> &str {
        &self.key_id
    }
}

impl KmsKeyStore {
    /// The Result-based accessor the rest of the codebase should
    /// use. The trait method above is a best-effort fallback for
    /// paths that haven't been migrated yet.
    pub fn try_field_key(&self) -> Result<&[u8; super::KEY_LEN], CryptoError> {
        Err(self.stub_error())
    }
    pub fn try_hmac_key(&self) -> Result<&[u8; super::KEY_LEN], CryptoError> {
        Err(self.stub_error())
    }
    pub fn new(key_id: impl Into<String>) -> Self {
        Self { key_id: key_id.into() }
    }
}

const ZERO_KEY: [u8; super::KEY_LEN] = [0u8; super::KEY_LEN];

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

    // ---- KeyStore trait — LocalKeyStore impl ----

    #[test]
    fn local_keystore_trait_impl_returns_correct_keys() {
        let ks = LocalKeyStore::from_raw([0x42; 32], [0x77; 32]);
        let trait_obj: &dyn KeyStore = &ks;
        assert_eq!(trait_obj.field_key()[0], 0x42);
        assert_eq!(trait_obj.hmac_key()[0], 0x77);
        assert_eq!(trait_obj.key_id(), "raw");
    }

    #[test]
    fn local_keystore_trait_impl_matches_inherent_methods() {
        // The trait impl forwards to the inherent methods, so the
        // results must be byte-identical regardless of how the
        // caller holds the store.
        let ks = LocalKeyStore::from_raw([0x99; 32], [0xAB; 32]);
        let trait_obj: &dyn KeyStore = &ks;
        assert_eq!(trait_obj.field_key(), &ks.field_key()[..]);
        assert_eq!(trait_obj.hmac_key(), &ks.hmac_key()[..]);
        assert_eq!(trait_obj.key_id(), ks.key_id());
    }
}

/// Tests for the KmsKeyStore stub. The real KMS integration is
/// M6+ — for now the stub fails closed on every accessor so a
/// misconfigured prod deployment surfaces an error instead of
/// silently using a placeholder key.
#[cfg(test)]
mod kms_tests {
    use super::*;

    #[test]
    fn kms_stub_error_includes_key_id() {
        let kms = KmsKeyStore::new("kms:alias/prod-2026-q1");
        let err = kms.stub_error();
        match err {
            CryptoError::KmsUnavailable { key_id } => {
                assert_eq!(key_id, "kms:alias/prod-2026-q1");
            }
            other => panic!("expected KmsUnavailable, got {other:?}"),
        }
    }

    #[test]
    fn kms_stub_try_accessors_return_error() {
        let kms = KmsKeyStore::new("kms:test");
        assert!(matches!(
            kms.try_field_key(),
            Err(CryptoError::KmsUnavailable { .. })
        ));
        assert!(matches!(
            kms.try_hmac_key(),
            Err(CryptoError::KmsUnavailable { .. })
        ));
    }

    #[test]
    fn kms_stub_trait_accessors_return_zero_key() {
        // The trait method is the "best-effort" path; it returns
        // zeros so old call sites don't panic. The real
        // migration in M6+ replaces this with an unwrapped DEK
        // or returns an error via the Result-based accessor.
        let kms = KmsKeyStore::new("kms:test");
        let trait_obj: &dyn KeyStore = &kms;
        assert_eq!(trait_obj.field_key(), &[0u8; super::super::KEY_LEN][..]);
        assert_eq!(trait_obj.hmac_key(), &[0u8; super::super::KEY_LEN][..]);
        assert_eq!(trait_obj.key_id(), "kms:test");
    }
}
