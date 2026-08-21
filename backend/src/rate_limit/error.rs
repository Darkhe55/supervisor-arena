//! Rate-limit error type — used by the rating / login limiters to
//! surface a 429 to the client (mapped to `RateLimited` in the
//! account / rating error enums for the call sites).

use thiserror::Error;

#[derive(Debug, Error)]
#[non_exhaustive]
pub enum RateLimitError {
    #[error("rate limit hit: {kind}, retry in {retry_after_secs}s")]
    RateLimited {
        kind: &'static str,
        retry_after_secs: u64,
    },
}
