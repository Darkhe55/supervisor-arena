//! Database layer for accounts
//!
//! All queries go through `deadpool_postgres::Pool`. We do NOT use an ORM
//! — the table has 11 columns and a few hand-written queries are clearer
//! than mapping code.

use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use tokio_postgres::types::Type;
use tokio_postgres::Row;
use uuid::Uuid;

use super::error::AccountError;
use super::service::{NewAccount, StoredAccount};

/// Repository for the `accounts` table.
#[derive(Clone)]
pub struct AccountRepo {
    pool: Pool,
}

impl AccountRepo {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Insert a new account row. Returns `EmailTaken` if the unique
    /// `email_hash` constraint fires.
    pub async fn insert(&self, acct: &NewAccount) -> Result<Uuid, AccountError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("pool: {e}")))?;

        // sqlx-style param binding via tokio-postgres is via the
        // `client.query_one` / `client.execute` API. We pass values as
        // a slice of `&(dyn ToSql + Sync)`.
        let stmt = client
            .prepare_cached(
                "INSERT INTO accounts (
                    email_enc, email_hash, password_hash,
                    discipline_hash, institution_hash, grade_enc
                 ) VALUES ($1, $2, $3, $4, $5, $6)
                 RETURNING id",
            )
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("prepare: {e}")))?;

        let row = client
            .query_one(
                &stmt,
                &[
                    &acct.email_enc,
                    &acct.email_hash,
                    &acct.password_hash,
                    &acct.discipline_hash,
                    &acct.institution_hash,
                    &acct.grade_enc,
                ],
            )
            .await
            .map_err(|e| {
                if let Some(db_err) = e.as_db_error() {
                    if db_err.code() == &tokio_postgres::error::SqlState::UNIQUE_VIOLATION {
                        return AccountError::EmailTaken;
                    }
                }
                AccountError::Database(anyhow::anyhow!("insert: {e}"))
            })?;
        Ok(row.get::<_, Uuid>(0))
    }

    /// Find an account by its email HMAC.
    ///
    /// Returns `Ok(None)` if no row matches (so the service can return
    /// `InvalidCredentials` without distinguishing "no such user" from
    /// "wrong password").
    pub async fn find_by_email_hash(
        &self,
        email_hash: &[u8],
    ) -> Result<Option<StoredAccount>, AccountError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("pool: {e}")))?;
        let stmt = client
            .prepare_cached(
                "SELECT id, email_hash, password_hash, tier, joined_at, soft_removed, is_banned, is_cancelled
                 FROM accounts
                 WHERE email_hash = $1
                 LIMIT 1",
            )
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("prepare: {e}")))?;

        let row_opt = client
            .query_opt(&stmt, &[&email_hash])
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("query: {e}")))?;
        Ok(row_opt.map(row_to_stored))
    }

    /// Find by id (used by /auth/me).
    pub async fn find_by_id(&self, id: Uuid) -> Result<Option<StoredAccount>, AccountError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("pool: {e}")))?;
        let stmt = client
            .prepare_cached(
                "SELECT id, email_hash, password_hash, tier, joined_at, soft_removed, is_banned, is_cancelled
                 FROM accounts
                 WHERE id = $1
                 LIMIT 1",
            )
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("prepare: {e}")))?;

        let row_opt = client
            .query_opt(&stmt, &[&id])
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("query: {e}")))?;
        Ok(row_opt.map(row_to_stored))
    }

    /// Look up just the discipline_hash (P1) for an account. Used by the
    /// rating module to snapshot the rater's discipline at submission time
    /// (OUTLINE §5 dynamic relative correction needs the snapshot to stay
    /// stable even if the user later changes their declared discipline).
    ///
    /// Returns `Ok(None)` if the account doesn't exist (e.g. account was
    /// hard-deleted). Distinct from `Ok(Some(empty_vec))` which would
    /// indicate a row with NULL discipline_hash (shouldn't happen — schema
    /// marks the column NOT NULL).
    pub async fn find_discipline_hash(
        &self,
        id: Uuid,
    ) -> Result<Option<Vec<u8>>, AccountError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("pool: {e}")))?;
        let row_opt = client
            .query_opt(
                "SELECT discipline_hash FROM accounts WHERE id = $1::uuid LIMIT 1",
                &[&id],
            )
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("query: {e}")))?;
        Ok(row_opt.map(|r| r.get(0)))
    }

    /// Update `last_active_at` to now. Best-effort — log but don't error.
    pub async fn touch_active(&self, id: Uuid) -> Result<(), AccountError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("pool: {e}")))?;
        let stmt = client
            .prepare_cached("UPDATE accounts SET last_active_at = NOW() WHERE id = $1")
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("prepare: {e}")))?;
        client
            .execute(&stmt, &[&id])
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("touch: {e}")))?;
        Ok(())
    }

    /// Set the `soft_removed` flag on an account. Used by the admin
    /// endpoint to silently drop a teacher's votes (H-48 / OUTLINE
    /// §7.1). The user's existing ratings are kept on disk (audit
    /// trail); the aggregation query filters them out via JOIN.
    pub async fn set_soft_removed(
        &self,
        id: Uuid,
        soft_removed: bool,
    ) -> Result<(), AccountError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("pool: {e}")))?;
        client
            .execute(
                "UPDATE accounts SET soft_removed = $2::bool WHERE id = $1::uuid",
                &[&id, &soft_removed],
            )
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("set_soft_removed: {e}")))?;
        Ok(())
    }

    /// M5 邀请试用 — link a freshly-registered account to the
    /// inviter (the creator of the redeemed invitation code).
    pub async fn set_invited_by(
        &self,
        account_id: Uuid,
        inviter_id: Uuid,
    ) -> Result<(), AccountError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("pool: {e}")))?;
        client
            .execute(
                "UPDATE accounts SET invited_by_account_id = $2::uuid WHERE id = $1::uuid",
                &[&account_id, &inviter_id],
            )
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("set_invited_by: {e}")))?;
        Ok(())
    }

    /// Set the `is_banned` flag on an account. Banned accounts are
    /// excluded from aggregation AND cannot log in (H-48).
    pub async fn set_banned(
        &self,
        id: Uuid,
        is_banned: bool,
    ) -> Result<(), AccountError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("pool: {e}")))?;
        client
            .execute(
                "UPDATE accounts SET is_banned = $2::bool WHERE id = $1::uuid",
                &[&id, &is_banned],
            )
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("set_banned: {e}")))?;
        Ok(())
    }

    /// M3 §7.4 — Anonymize an account after user-initiated
    /// cancellation. The row stays (so existing ratings still
    /// count in aggregation, per OUTLINE §7.4 "数据匿名化保留,
    /// 评分仍计入"); all PII is wiped; `is_cancelled` flips to
    /// TRUE; `cancelled_at` is set; the password_hash is replaced
    /// with a sentinel value so no one can log in with any
    /// password attempt.
    ///
    /// We do NOT set `is_banned=TRUE` because the aggregation
    /// query filters by `is_banned` (H-48) — that would silently
    /// drop the cancelled user's existing ratings. The login
    /// path checks `is_cancelled` separately.
    ///
    /// Idempotent at the SQL level (the WHERE clause won't match
    /// an already-cancelled row).
    pub async fn anonymize_for_cancellation(&self, id: Uuid) -> Result<(), AccountError> {
        let client = self
            .pool
            .get()
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("pool: {e}")))?;
        client
            .execute(
                "UPDATE accounts
                 SET is_cancelled = TRUE,
                     cancelled_at = NOW(),
                     email_enc = ''::bytea,
                     email_hash = ''::bytea,
                     institution_hash = ''::bytea,
                     grade_enc = NULL,
                     password_hash = '$argon2id$v=19$m=19456,t=2,p=1$cancelledsentinel000000000000$0M5B8Iq3Sqz6fDbm7QbMsK6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6e6'
                 WHERE id = $1::uuid AND is_cancelled = FALSE",
                &[&id],
            )
            .await
            .map_err(|e| AccountError::Database(anyhow::anyhow!("anonymize_for_cancellation: {e}")))?;
        Ok(())
    }
}

fn row_to_stored(row: Row) -> StoredAccount {
    StoredAccount {
        id: row.get(0),
        email_hash: row.get(1),
        password_hash: row.get(2),
        tier: row.get(3),
        joined_at: row.get::<_, DateTime<Utc>>(4),
        soft_removed: row.get(5),
        is_banned: row.get(6),
        is_cancelled: row.get(7),
    }
}

// Mark unused Type import as allowed — we may need it for future typed params.
#[allow(dead_code)]
fn _type_marker(_: Type) {}
