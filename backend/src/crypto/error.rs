//! Error types for the crypto module
//!
//! All errors are `#[non_exhaustive]` so future variants don't break callers.
//! The `Display` impl never includes plaintext, keys, or any P0/P1/P2 data —
//! see G-8 §"错误信息不暴露加密字段" in `docs/OUTLINE.md`.

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum CryptoError {
    /// AES-GCM authentication failed (wrong key, tampered ciphertext, or wrong AAD).
    #[error("decryption failed: authentication tag mismatch")]
    DecryptionFailed,

    /// Random nonce generation failed.
    #[error("RNG failure: {0}")]
    Rng(String),

    /// Hex decoding of a configured key failed.
    #[error("invalid hex key: {0}")]
    InvalidHexKey(String),

    /// A configured key has the wrong length (must be 32 bytes = 64 hex chars).
    #[error("invalid key length: expected {expected} bytes, got {actual}")]
    InvalidKeyLength { expected: usize, actual: usize },

    /// Argon2 password hashing failed.
    #[error("password hash failure: {0}")]
    Argon2Hash(String),

    /// Argon2 password verification failed (not a wrong-password — actual error).
    #[error("password verify failure: {0}")]
    Argon2Verify(String),

    /// PHC-format hash string is malformed.
    #[error("malformed password hash: {0}")]
    MalformedPasswordHash(String),

    /// M6 KMS stub: the key store was configured to use a KMS
    /// backend but the actual KMS integration is not yet wired
    /// in. Failures are loud (this error) rather than silent
    /// (returning a placeholder key) so a misconfigured prod
    /// deployment is caught at first use.
    #[error("KMS backend '{key_id}' is not yet wired in (M6 stub)")]
    KmsUnavailable { key_id: String },
}

impl From<getrandom::Error> for CryptoError {
    fn from(e: getrandom::Error) -> Self {
        CryptoError::Rng(e.to_string())
    }
}

impl From<hex::FromHexError> for CryptoError {
    fn from(e: hex::FromHexError) -> Self {
        CryptoError::InvalidHexKey(e.to_string())
    }
}
