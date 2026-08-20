//! JWT issuing and verification (HS256, single secret)
//!
//! - Algorithm: HS256 (HMAC-SHA256). Symmetric — fine for a single-issuer
//!   backend. Switch to RS256/EdDSA when a separate auth service appears.
//! - Lifetime: 15 minutes (from `AuthConfig::jwt_access_ttl_secs`).
//! - Claims: `sub` = account UUID, `tier` = "basic" | "member" (so middleware
//!   can do tier checks without an extra DB round-trip), `iat`, `exp`,
//!   `iss` (configurable later).
//!
//! Refresh tokens are NOT issued in M4. See `account::mod` for the rationale.

use anyhow::Context;
use chrono::Utc;
use jsonwebtoken::{decode, encode, DecodingKey, EncodingKey, Header, Validation};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::error::AccountError;
use crate::config::AuthConfig;

/// JWT claims payload.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Claims {
    /// Subject — the account UUID.
    pub sub: String,
    /// Account tier at issue time. Snapshotted, not live.
    pub tier: String,
    /// Issued-at (Unix seconds).
    pub iat: i64,
    /// Expiry (Unix seconds).
    pub exp: i64,
}

/// Thin wrapper around the HS256 secret + TTL.
#[derive(Clone)]
pub struct JwtService {
    encoding: EncodingKey,
    decoding: DecodingKey,
    validation: Validation,
    access_ttl: i64,
}

// Manual Debug that does NOT leak the secret bytes.
impl std::fmt::Debug for JwtService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtService")
            .field("algorithm", &"HS256")
            .field("access_ttl_secs", &self.access_ttl)
            .finish_non_exhaustive()
    }
}

impl JwtService {
    pub fn from_config(config: &AuthConfig) -> Result<Self, anyhow::Error> {
        if config.jwt_secret.len() < 32 {
            anyhow::bail!(
                "jwt_secret is too short ({} bytes, need >= 32)",
                config.jwt_secret.len()
            );
        }
        Ok(Self {
            encoding: EncodingKey::from_secret(config.jwt_secret.as_bytes()),
            decoding: DecodingKey::from_secret(config.jwt_secret.as_bytes()),
            validation: Validation::new(jsonwebtoken::Algorithm::HS256),
            access_ttl: config.access_ttl().as_secs() as i64,
        })
    }

    /// Issue a new access token for the given account.
    pub fn issue(&self, account_id: Uuid, tier: &str) -> Result<(String, i64), AccountError> {
        let now = Utc::now().timestamp();
        let exp = now + self.access_ttl;
        let claims = Claims {
            sub: account_id.to_string(),
            tier: tier.to_string(),
            iat: now,
            exp,
        };
        let token = encode(&Header::new(jsonwebtoken::Algorithm::HS256), &claims, &self.encoding)
            .context("jwt encode")
            .map_err(AccountError::from)?;
        Ok((token, exp - now))
    }

    /// Verify a token, return its claims.
    pub fn verify(&self, token: &str) -> Result<Claims, AccountError> {
        let data = decode::<Claims>(token, &self.decoding, &self.validation)
            .context("jwt decode")
            .map_err(AccountError::from)?;
        Ok(data.claims)
    }

    /// Verify and return the account UUID.
    pub fn verify_account_id(&self, token: &str) -> Result<Uuid, AccountError> {
        let claims = self.verify(token)?;
        Uuid::parse_str(&claims.sub).map_err(|_| AccountError::MalformedSubject)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_config() -> AuthConfig {
        AuthConfig {
            jwt_secret: "x".repeat(64), // 64 chars >= 32 minimum
            jwt_access_ttl_secs: 900,
            jwt_refresh_ttl_secs: 604800,
        }
    }

    #[test]
    fn roundtrip_issue_verify() {
        let svc = JwtService::from_config(&test_config()).unwrap();
        let acc = Uuid::new_v4();
        let (token, ttl) = svc.issue(acc, "basic").unwrap();
        assert!(ttl > 0);
        let claims = svc.verify(&token).unwrap();
        assert_eq!(claims.sub, acc.to_string());
        assert_eq!(claims.tier, "basic");
    }

    #[test]
    fn verify_account_id_parses_uuid() {
        let svc = JwtService::from_config(&test_config()).unwrap();
        let acc = Uuid::new_v4();
        let (token, _) = svc.issue(acc, "member").unwrap();
        let parsed = svc.verify_account_id(&token).unwrap();
        assert_eq!(parsed, acc);
    }

    #[test]
    fn reject_short_secret() {
        let bad = AuthConfig {
            jwt_secret: "too_short".into(),
            jwt_access_ttl_secs: 900,
            jwt_refresh_ttl_secs: 604800,
        };
        let err = JwtService::from_config(&bad).unwrap_err();
        assert!(err.to_string().contains("too short"));
    }

    #[test]
    fn reject_tampered_token() {
        let svc = JwtService::from_config(&test_config()).unwrap();
        let (token, _) = svc.issue(Uuid::new_v4(), "basic").unwrap();
        // Flip a character in the signature segment.
        let mut tampered = token.clone();
        let last = tampered.pop().unwrap();
        tampered.push(if last == 'A' { 'B' } else { 'A' });
        assert!(svc.verify(&tampered).is_err());
    }
}
