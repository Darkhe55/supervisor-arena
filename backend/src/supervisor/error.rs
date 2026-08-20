//! Errors for the alias generator

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum AliasError {
    /// Underlying HMAC computation failed (should be unreachable).
    #[error("hash failure: {0}")]
    Hash(String),

    /// We tried N times to produce a whitelist-clean alias and failed.
    /// The 1-to-1 DB UNIQUE constraint will catch any remaining collision.
    #[error("could not produce whitelist-clean alias after {0} retries")]
    WhitelistExhausted(u32),

    /// The discipline key is empty or otherwise unusable.
    #[error("invalid discipline: {0}")]
    InvalidDiscipline(String),
}
