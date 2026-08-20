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
                "SELECT id, email_hash, password_hash, tier, joined_at, soft_removed, is_banned
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
                "SELECT id, email_hash, password_hash, tier, joined_at, soft_removed, is_banned
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
    }
}

// Mark unused Type import as allowed — we may need it for future typed params.
#[allow(dead_code)]
fn _type_marker(_: Type) {}
