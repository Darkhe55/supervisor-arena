//! AES-256-GCM authenticated encryption
//!
//! - Key length: 32 bytes (256 bits) — see [`super::KEY_LEN`]
//! - Nonce length: 12 bytes — see [`super::NONCE_LEN`]
//! - Tag length: 16 bytes — see [`super::TAG_LEN`]
//!
//! **Output format** (binary blob, suitable for `BYTEA` storage):
//! ```text
//! [ nonce (12) | ciphertext (N) | tag (16) ]
//! ```
//! The tag is appended automatically by `aes-gcm`. Total overhead per blob is
//! 12 + 16 = 28 bytes regardless of plaintext length.
//!
//! **AAD (Additional Authenticated Data)** is optional but recommended. It is
//! mixed into the authentication tag but NOT into the ciphertext. Use it to
//! bind the ciphertext to its column name / record id, so a value stolen from
//! column A cannot be replayed into column B. See NIST SP 800-38D §5.2.1.

use aead::{Aead, KeyInit, Payload};
use aes_gcm::Aes256Gcm;
use rand_core::{OsRng, RngCore};
use zeroize::Zeroizing;

use super::{CryptoError, KEY_LEN, NONCE_LEN, TAG_LEN};

/// Build an `Aes256Gcm` cipher from a 32-byte key.
fn cipher_from_key(key: &[u8; KEY_LEN]) -> Aes256Gcm {
    Aes256Gcm::new(key.into())
}

/// Encrypt `plaintext` with the given key and optional AAD.
///
/// Returns a binary blob: `nonce || ciphertext || tag`. The nonce is freshly
/// generated from the OS RNG; never reuse a (key, nonce) pair — GCM security
/// collapses if you do.
pub fn encrypt(
    key: &[u8; KEY_LEN],
    plaintext: &[u8],
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, CryptoError> {
    let cipher = cipher_from_key(key);
    let mut nonce_bytes = [0u8; NONCE_LEN];
    OsRng.fill_bytes(&mut nonce_bytes);

    let payload = match aad {
        Some(a) => Payload { msg: plaintext, aad: a },
        None => Payload { msg: plaintext, aad: &[] },
    };

    let ciphertext = cipher
        .encrypt((&nonce_bytes).into(), payload)
        .map_err(|_| CryptoError::Rng("AEAD encryption failed (should be unreachable)".into()))?;

    // blob = nonce || ciphertext (tag is appended inside `ciphertext` by aes-gcm)
    let mut blob = Vec::with_capacity(NONCE_LEN + ciphertext.len());
    blob.extend_from_slice(&nonce_bytes);
    blob.extend_from_slice(&ciphertext);
    Ok(blob)
}

/// Decrypt a blob produced by [`encrypt`].
///
/// Verifies the authentication tag; returns [`CryptoError::DecryptionFailed`]
/// on wrong key, tampered ciphertext, wrong AAD, or truncated input. The error
/// is intentionally opaque — we do not distinguish "wrong key" from
/// "tampered ciphertext" because the difference is not useful to an attacker
/// and could leak info to a legitimate caller.
pub fn decrypt(
    key: &[u8; KEY_LEN],
    blob: &[u8],
    aad: Option<&[u8]>,
) -> Result<Zeroizing<Vec<u8>>, CryptoError> {
    if blob.len() < NONCE_LEN + TAG_LEN {
        return Err(CryptoError::DecryptionFailed);
    }

    let (nonce_bytes, ciphertext_and_tag) = blob.split_at(NONCE_LEN);
    let cipher = cipher_from_key(key);

    let payload = match aad {
        Some(a) => Payload { msg: ciphertext_and_tag, aad: a },
        None => Payload { msg: ciphertext_and_tag, aad: &[] },
    };

    cipher
        .decrypt(nonce_bytes.into(), payload)
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::DecryptionFailed)
}

/// Encrypt a UTF-8 string and return the raw blob. Convenience wrapper
/// for the common "encrypt this text field" case.
pub fn encrypt_str(
    key: &[u8; KEY_LEN],
    plaintext: &str,
    aad: Option<&[u8]>,
) -> Result<Vec<u8>, CryptoError> {
    encrypt(key, plaintext.as_bytes(), aad)
}

/// Decrypt a blob into a UTF-8 string. Returns [`CryptoError::DecryptionFailed`]
/// if the decrypted bytes are not valid UTF-8 (also deliberately opaque —
/// either the key was wrong or the data is corrupt, and the caller should
/// not be told which).
pub fn decrypt_str(
    key: &[u8; KEY_LEN],
    blob: &[u8],
    aad: Option<&[u8]>,
) -> Result<Zeroizing<String>, CryptoError> {
    let bytes = decrypt(key, blob, aad)?;
    String::from_utf8(bytes.to_vec())
        .map(Zeroizing::new)
        .map_err(|_| CryptoError::DecryptionFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    const KEY: [u8; KEY_LEN] = [0x42_u8; KEY_LEN];

    #[test]
    fn roundtrip_no_aad() {
        let pt = b"hello, supervisor-arena";
        let blob = encrypt(&KEY, pt, None).unwrap();
        let recovered = decrypt(&KEY, &blob, None).unwrap();
        assert_eq!(recovered.as_slice(), pt);
    }

    #[test]
    fn roundtrip_with_aad() {
        let pt = b"secret data";
        let aad = b"accounts.email";
        let blob = encrypt(&KEY, pt, Some(aad)).unwrap();
        let recovered = decrypt(&KEY, &blob, Some(aad)).unwrap();
        assert_eq!(recovered.as_slice(), pt);
    }

    #[test]
    fn wrong_aad_fails() {
        let pt = b"x";
        let blob = encrypt(&KEY, pt, Some(b"col_a")).unwrap();
        assert!(decrypt(&KEY, &blob, Some(b"col_b")).is_err());
        // No AAD on decrypt side also fails
        assert!(decrypt(&KEY, &blob, None).is_err());
    }

    #[test]
    fn wrong_key_fails() {
        let pt = b"x";
        let blob = encrypt(&KEY, pt, None).unwrap();
        let other_key = [0x99_u8; KEY_LEN];
        assert!(decrypt(&other_key, &blob, None).is_err());
    }

    #[test]
    fn tampered_ciphertext_fails() {
        let blob = encrypt(&KEY, b"important", None).unwrap();
        let mut tampered = blob.clone();
        // Flip a bit somewhere after the nonce.
        let idx = NONCE_LEN + 2;
        tampered[idx] ^= 0x01;
        assert!(decrypt(&KEY, &tampered, None).is_err());
    }

    #[test]
    fn truncated_blob_fails() {
        let blob = encrypt(&KEY, b"x", None).unwrap();
        assert!(decrypt(&KEY, &blob[..NONCE_LEN + TAG_LEN - 1], None).is_err());
    }

    #[test]
    fn nonce_is_unique_per_call() {
        // 100 encrypts of the same plaintext + same key must yield 100 distinct blobs.
        let pt = b"same input";
        let mut blobs: Vec<Vec<u8>> = (0..100)
            .map(|_| encrypt(&KEY, pt, None).unwrap())
            .collect();
        blobs.dedup();
        assert_eq!(blobs.len(), 100, "nonce reuse detected — RNG broken");
    }

    #[test]
    fn roundtrip_str_happy_path() {
        let blob = encrypt_str(&KEY, "中文 + emoji 🔒", None).unwrap();
        let recovered = decrypt_str(&KEY, &blob, None).unwrap();
        assert_eq!(recovered.as_str(), "中文 + emoji 🔒");
    }

    #[test]
    fn decrypt_str_rejects_non_utf8() {
        // Force a non-UTF-8 plaintext by encrypting raw bytes.
        let blob = encrypt(&KEY, &[0xFF, 0xFE, 0xFD], None).unwrap();
        assert!(decrypt_str(&KEY, &blob, None).is_err());
    }
}
