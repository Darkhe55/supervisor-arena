//! Invitation service: code generation, lookup, and redemption.

use chrono::{DateTime, Duration, Utc};
use uuid::Uuid;

use super::error::InvitationError;
use super::repo::{InvitationRepo, InvitationRow};
use crate::crypto::hmac;

/// Default validity window for a generated code (no expires_at →
/// the row is created with this expiry; callers can override).
const DEFAULT_VALIDITY_HOURS: i64 = 24 * 30; // 30 days

/// Code length (excludes dashes). 12 hex chars = 48 bits of
/// entropy — collision probability for a few thousand codes is
/// negligible. We display the code with dashes for legibility.
#[allow(dead_code)]
const CODE_RAW_LEN: usize = 12;

#[derive(Clone)]
pub struct InvitationService {
    repo: InvitationRepo,
    /// HMAC key for deriving the random code. We need a
    /// `&[u8; 32]` HMAC key (any 32 bytes; using the same key as
    /// the rest of the app is fine and gives consistent entropy
    /// across processes). The caller passes it in via `new` so
    /// the service is decoupled from `AppState`.
    rng_hmac_key: [u8; 32],
}

impl InvitationService {
    pub fn new(repo: InvitationRepo, rng_hmac_key: [u8; 32]) -> Self {
        Self { repo, rng_hmac_key }
    }

    // ---- Pure helpers (unit-tested below) ----

    /// Pure: format a raw 12-char code as "XXXX-XXXX-XXXX" for
    /// display. The DB stores the un-dashed uppercase form.
    pub fn format_code(raw: &str) -> String {
        let raw = raw.to_ascii_uppercase();
        let mut out = String::with_capacity(14);
        for (i, ch) in raw.chars().enumerate() {
            if i > 0 && i % 4 == 0 {
                out.push('-');
            }
            out.push(ch);
        }
        out
    }

    /// Pure: generate a 12-hex-char code from a UUID. Uses
    /// HMAC-SHA256(hmac_key, uuid) and takes the first 12 hex
    /// chars. The HMAC means codes aren't predictable from the
    /// UUID alone.
    pub fn generate_code(hmac_key: &[u8; 32], seed: Uuid) -> String {
        // HMAC a stable string representation of the UUID.
        let s = seed.to_string();
        let mac = hmac::hash_str(hmac_key, &s).expect("hmac never fails");
        // mac is a 64-char hex string; take the first 12 chars.
        mac[..12].to_ascii_uppercase()
    }

    /// Pure: derive the redemption validity window. Defaults to
    /// 30 days from `now` if `expires_at` is None.
    pub fn default_expiry(now: DateTime<Utc>) -> DateTime<Utc> {
        now + Duration::hours(DEFAULT_VALIDITY_HOURS)
    }

    // ---- I/O-backed operations ----

    /// Generate a new code. Returns the formatted display string
    /// (with dashes) — the raw form is what the DB stores.
    pub async fn create(
        &self,
        created_by: Option<Uuid>,
        max_uses: i32,
        expires_at: Option<DateTime<Utc>>,
        note: Option<&str>,
    ) -> Result<(String, InvitationRow), InvitationError> {
        // Generate a candidate code and retry on collision
        // (negligible at our scale, but cheap to be safe).
        let mut last_err: Option<InvitationError> = None;
        for _ in 0..8 {
            let seed = Uuid::new_v4();
            let code = Self::generate_code(&self.rng_hmac_key, seed);
            match self
                .repo
                .insert(&code, created_by, max_uses, expires_at, note)
                .await
            {
                Ok(row) => return Ok((Self::format_code(&row.code), row)),
                Err(e @ InvitationError::Database(_)) if is_collision(&e) => {
                    last_err = Some(e);
                    continue;
                }
                Err(e) => return Err(e),
            }
        }
        Err(last_err.unwrap_or(InvitationError::FullyUsed))
    }

    /// Look up a code (case-insensitive). Pure read.
    pub async fn lookup(&self, code: &str) -> Result<Option<InvitationRow>, InvitationError> {
        self.repo.find_by_code(code).await
    }

    /// Validate a code is redeemable. Returns the row on success
    /// or a precise error (CodeNotFound / Expired / Revoked /
    /// FullyUsed).
    pub async fn validate_redeemable(
        &self,
        code: &str,
        now: DateTime<Utc>,
    ) -> Result<InvitationRow, InvitationError> {
        let row = self
            .repo
            .find_by_code(code)
            .await?
            .ok_or_else(|| InvitationError::CodeNotFound(code.to_string()))?;
        if let Some(ts) = row.revoked_at {
            return Err(InvitationError::Revoked(ts));
        }
        if row.use_count >= row.max_uses {
            return Err(InvitationError::FullyUsed);
        }
        if let Some(exp) = row.expires_at {
            if now > exp {
                return Err(InvitationError::Expired(exp));
            }
        }
        Ok(row)
    }

    /// Atomically redeem one use of a code. The caller is
    /// responsible for inserting the new account row (in the
    /// same transaction) and recording the inviter in the
    /// `used_by` / `used_at` columns of the invitation.
    pub async fn redeem(
        &self,
        code: &str,
    ) -> Result<InvitationRow, InvitationError> {
        let row = self.validate_redeemable(code, Utc::now()).await?;
        // Atomically bump use_count. The WHERE clause re-checks
        // the redeemable conditions in case of a race.
        self.repo.redeem(row.id).await
    }

    /// List codes created by a specific account. Used by the
    /// "my invites" UI.
    pub async fn list_by_creator(
        &self,
        account_id: Uuid,
    ) -> Result<Vec<InvitationRow>, InvitationError> {
        self.repo.list_by_creator(account_id).await
    }
}

fn is_collision(e: &InvitationError) -> bool {
    matches!(e, InvitationError::Database(msg) if msg.to_string().contains("collision"))
}

/// What the registration flow did with the invite code. Used
/// by the API to tell the frontend whether the redemption
/// succeeded (so it can show a "thanks for joining" UI).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RedemptionOutcome {
    /// No invite code was provided at registration.
    NotProvided,
    /// The code was redeemed successfully.
    Redeemed { inviter: Option<Uuid> },
    /// The code was provided but not redeemable. We don't
    /// surface the specific reason to the client (avoid
    /// enumeration) — the registration still succeeds, just
    /// without the "invited" tag.
    Invalid { reason: String },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key() -> [u8; 32] {
        [0x42u8; 32]
    }

    // ---- format_code ----

    #[test]
    fn format_code_inserts_dashes_every_4_chars() {
        assert_eq!(InvitationService::format_code("abcd1234ef56"), "ABCD-1234-EF56");
    }

    #[test]
    fn format_code_uppercases() {
        assert_eq!(InvitationService::format_code("abcdefghijkl"), "ABCD-EFGH-IJKL");
    }

    #[test]
    fn format_code_handles_short_codes() {
        // < 4 chars: no dashes
        assert_eq!(InvitationService::format_code("ab"), "AB");
        // Exactly 4: no dashes (the rule is "i > 0 AND i % 4 == 0")
        assert_eq!(InvitationService::format_code("abcd"), "ABCD");
    }

    // ---- generate_code ----

    #[test]
    fn generate_code_returns_12_hex_chars() {
        let seed = Uuid::new_v4();
        let code = InvitationService::generate_code(&key(), seed);
        assert_eq!(code.len(), 12);
        assert!(code.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn generate_code_is_deterministic() {
        let seed = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let a = InvitationService::generate_code(&key(), seed);
        let b = InvitationService::generate_code(&key(), seed);
        assert_eq!(a, b);
    }

    #[test]
    fn generate_code_changes_with_key() {
        let seed = Uuid::new_v4();
        let a = InvitationService::generate_code(&key(), seed);
        let b = InvitationService::generate_code(&[0x99u8; 32], seed);
        assert_ne!(a, b);
    }

    // ---- default_expiry ----

    #[test]
    fn default_expiry_is_30_days_out() {
        let now = Utc::now();
        let exp = InvitationService::default_expiry(now);
        let diff = (exp - now).num_hours();
        assert!((diff - 30 * 24).abs() < 2, "expected ~30 days, got {} hours", diff);
    }
}
