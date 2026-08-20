//! Argon2id password hashing
//!
//! - Output format: PHC string (includes algorithm, version, parameters, salt,
//!   hash) — the standard `$argon2id$v=19$m=...$t=...$p=...$salt$hash` form.
//! - The PHC string can be stored in a single `VARCHAR(255)` column.
//! - The default `Argon2::default()` parameters follow the `argon2` crate's
//!   recommended values (Argon2id, m=19456 KiB, t=2, p=1 as of writing), which
//!   are within the OWASP 2024 Password Storage Cheat Sheet guidance.
//! - Verification is constant-time via the underlying `argon2` crate.

use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};

use super::CryptoError;

/// Hash a plaintext password. The output is a PHC-format string suitable for
/// direct DB storage.
pub fn hash_password(password: &str) -> Result<String, CryptoError> {
    // SaltString::generate panics on RNG failure; we map that to an error.
    let salt = SaltString::generate(&mut argon2::password_hash::rand_core::OsRng);

    let argon2 = Argon2::default();
    let hash = argon2
        .hash_password(password.as_bytes(), &salt)
        .map_err(|e| CryptoError::Argon2Hash(e.to_string()))?
        .to_string();
    Ok(hash)
}

/// Verify a password against a stored PHC string.
///
/// Returns:
/// - `Ok(true)` — password matches
/// - `Ok(false)` — password does not match (this is NOT an error, just auth fail)
/// - `Err(...)` — the stored hash is malformed, or verification itself failed
pub fn verify_password(password: &str, phc: &str) -> Result<bool, CryptoError> {
    let parsed = PasswordHash::new(phc).map_err(|e| CryptoError::MalformedPasswordHash(e.to_string()))?;
    Ok(Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hash_then_verify_succeeds() {
        let h = hash_password("correct horse battery staple").unwrap();
        assert!(verify_password("correct horse battery staple", &h).unwrap());
    }

    #[test]
    fn wrong_password_fails() {
        let h = hash_password("password123").unwrap();
        assert!(!verify_password("password124", &h).unwrap());
    }

    #[test]
    fn phc_string_format() {
        let h = hash_password("x").unwrap();
        // PHC strings start with $argon2 and include $argon2id$ (since Argon2::default uses Argon2id).
        assert!(h.starts_with("$argon2"), "got: {h}");
        assert!(h.contains("$argon2id$"), "got: {h}");
        // Argon2id PHC is bounded in size.
        assert!(h.len() < 256, "PHC too long: {} bytes", h.len());
    }

    #[test]
    fn two_hashes_of_same_password_differ() {
        // Salt is random — same plaintext must produce different PHC strings.
        let a = hash_password("same").unwrap();
        let b = hash_password("same").unwrap();
        assert_ne!(a, b);
        // But both verify.
        assert!(verify_password("same", &a).unwrap());
        assert!(verify_password("same", &b).unwrap());
    }

    #[test]
    fn malformed_hash_returns_error_not_panic() {
        let r = verify_password("anything", "not-a-phc-string");
        assert!(r.is_err());
    }

    #[test]
    fn empty_password_still_hashes() {
        // We do NOT silently accept empty passwords — but Argon2 itself will
        // happily hash "" and verify "" against it. Callers must enforce
        // non-empty passwords at the validator / API layer.
        let h = hash_password("").unwrap();
        assert!(verify_password("", &h).unwrap());
        assert!(!verify_password("nonempty", &h).unwrap());
    }
}
