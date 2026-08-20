//! Account business logic — orchestrates validation, crypto, repo, and JWT
//!
//! The service holds the dependencies it needs (`AccountRepo`, `JwtService`,
//! `LocalKeyStore`) and exposes one method per business operation. Handlers
//! are thin — they translate HTTP <-> service calls and map errors to status
//! codes.

use std::sync::Arc;
use uuid::Uuid;

use crate::config::AuthConfig;
use crate::crypto::{aes, argon2, hmac, LocalKeyStore};

use super::dto::{AuthResponse, LoginRequest, RegisterRequest};
use super::error::AccountError;
use super::jwt::JwtService;
use super::repo::AccountRepo;
use super::validation::{
    validate_discipline, validate_email, validate_grade, validate_institution, validate_password,
};

/// Internal: data to insert into `accounts` (already encrypted / hashed).
pub struct NewAccount {
    pub email_enc: Vec<u8>,
    pub email_hash: Vec<u8>,
    pub password_hash: String,
    pub discipline_hash: Vec<u8>,
    pub institution_hash: Vec<u8>,
    pub grade_enc: Option<Vec<u8>>,
}

/// Internal: columns we read back from `accounts`.
pub struct StoredAccount {
    pub id: Uuid,
    pub email_hash: Vec<u8>,
    pub password_hash: String,
    pub tier: String,
    pub joined_at: chrono::DateTime<chrono::Utc>,
    pub soft_removed: bool,
    pub is_banned: bool,
}

#[derive(Clone)]
pub struct AccountService {
    repo: AccountRepo,
    keys: Arc<LocalKeyStore>,
    jwt: JwtService,
}

impl AccountService {
    pub fn new(repo: AccountRepo, keys: Arc<LocalKeyStore>, auth: &AuthConfig) -> Result<Self, anyhow::Error> {
        let jwt = JwtService::from_config(auth)?;
        Ok(Self { repo, keys, jwt })
    }

    /// Register a new account.
    ///
    /// Steps:
    /// 1. Validate inputs.
    /// 2. Encrypt email (AES), hash email/discipline/institution (HMAC).
    /// 3. Hash password (Argon2id).
    /// 4. Insert. Email-unique constraint maps to `EmailTaken`.
    /// 5. Issue access token.
    pub async fn register(&self, req: RegisterRequest) -> Result<AuthResponse, AccountError> {
        // 1. Validate.
        validate_email(&req.email)?;
        validate_password(&req.password)?;
        validate_discipline(&req.discipline)?;
        validate_institution(&req.institution)?;
        if let Some(g) = &req.grade {
            validate_grade(g)?;
        }

        // 2. Crypto.
        let field_key = self.keys.field_key();
        let hmac_key = self.keys.hmac_key();

        let email_enc = aes::encrypt_str(field_key, &req.email, Some(b"accounts.email_enc"))?;
        let email_hash = hmac::hash_str(hmac_key, &req.email)?.into_bytes();
        let discipline_hash = hmac::hash_str(hmac_key, &req.discipline)?.into_bytes();
        let institution_hash = hmac::hash_str(hmac_key, &req.institution)?.into_bytes();
        let grade_enc = match &req.grade {
            Some(g) => Some(aes::encrypt_str(field_key, g, Some(b"accounts.grade_enc"))?),
            None => None,
        };

        // 3. Password.
        let password_hash = argon2::hash_password(&req.password)?;

        // 4. Insert.
        let new_acct = NewAccount {
            email_enc,
            email_hash,
            password_hash,
            discipline_hash,
            institution_hash,
            grade_enc,
        };
        let id = self.repo.insert(&new_acct).await?;

        // 5. Issue token.
        let (token, expires_in) = self.jwt.issue(id, "basic")?;
        Ok(AuthResponse {
            account_id: id,
            access_token: token,
            expires_in,
            tier: "basic".into(),
        })
    }

    /// Login: email + password -> access token.
    ///
    /// Returns `InvalidCredentials` for both "no such user" and "wrong
    /// password" to prevent account enumeration. Banned / soft-removed
    /// accounts get `AccountUnavailable` so we can return a distinct 403.
    pub async fn login(&self, req: LoginRequest) -> Result<AuthResponse, AccountError> {
        let hmac_key = self.keys.hmac_key();
        let email_hash = hmac::hash_str(hmac_key, &req.email)?.into_bytes();

        let acct = self
            .repo
            .find_by_email_hash(&email_hash)
            .await?
            .ok_or(AccountError::InvalidCredentials)?;

        if acct.is_banned {
            return Err(AccountError::AccountUnavailable);
        }
        // soft_removed: still allow login (per OUTLINE §7.1 — the user can
        // see the site but their votes are silently dropped). We do NOT
        // return AccountUnavailable here.

        // Constant-time password verify via Argon2id.
        let ok = argon2::verify_password(&req.password, &acct.password_hash)
            .map_err(|_| AccountError::InvalidCredentials)?;
        if !ok {
            return Err(AccountError::InvalidCredentials);
        }

        // Best-effort touch.
        let _ = self.repo.touch_active(acct.id).await;

        let (token, expires_in) = self.jwt.issue(acct.id, &acct.tier)?;
        Ok(AuthResponse {
            account_id: acct.id,
            access_token: token,
            expires_in,
            tier: acct.tier,
        })
    }

    /// Look up an account by id (for /auth/me).
    pub async fn get(&self, id: Uuid) -> Result<super::dto::AccountResponse, AccountError> {
        let acct = self
            .repo
            .find_by_id(id)
            .await?
            .ok_or(AccountError::InvalidCredentials)?;
        Ok(super::dto::AccountResponse {
            account_id: acct.id,
            tier: acct.tier,
            joined_at: acct.joined_at,
        })
    }

    /// Expose the JWT service for the auth extractor.
    pub fn jwt(&self) -> &JwtService {
        &self.jwt
    }
}
