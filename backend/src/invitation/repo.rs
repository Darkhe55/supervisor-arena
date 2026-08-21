//! Invitation repository.

use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use tokio_postgres::Row;
use uuid::Uuid;

use super::error::InvitationError;

#[derive(Clone)]
pub struct InvitationRepo {
    pool: Pool,
}

impl InvitationRepo {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Insert a new invitation. The `code` is normalized to
    /// uppercase before insert so lookups can be case-insensitive.
    pub async fn insert(
        &self,
        code: &str,
        created_by: Option<Uuid>,
        max_uses: i32,
        expires_at: Option<DateTime<Utc>>,
        note: Option<&str>,
    ) -> Result<InvitationRow, InvitationError> {
        let code_norm = code.to_ascii_uppercase();
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "INSERT INTO account_invitations
                    (code, created_by, max_uses, expires_at, note)
                 VALUES ($1::text, $2, $3, $4, $5)
                 RETURNING id, code, created_by, created_at, used_by, used_at,
                           max_uses, use_count, expires_at, revoked_at, note",
                &[&code_norm, &created_by, &max_uses, &expires_at, &note],
            )
            .await
            .map_err(|e| {
                // Unique violation → caller probably retried with a
                // colliding code (RNG collision). Surface a
                // dedicated error.
                if let Some(db) = e.as_db_error() {
                    if db.code() == &tokio_postgres::error::SqlState::UNIQUE_VIOLATION {
                        return InvitationError::Database(anyhow::anyhow!(
                            "invitation code collision (retry the generation)"
                        ));
                    }
                }
                InvitationError::from(e)
            })?;
        Ok(row_to_invitation(row))
    }

    /// Find an invitation by code (case-insensitive, ignores
    /// dashes). Returns `None` if no row matches.
    pub async fn find_by_code(
        &self,
        code: &str,
    ) -> Result<Option<InvitationRow>, InvitationError> {
        // Normalize: uppercase + strip dashes (so the user can
        // type "B7A6-0289-7E1D" or "b7a602897e1d" or "B7A602897E1D"
        // and they all match).
        let code_norm: String = code
            .chars()
            .filter(|c| *c != '-')
            .collect::<String>()
            .to_ascii_uppercase();
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT id, code, created_by, created_at, used_by, used_at,
                        max_uses, use_count, expires_at, revoked_at, note
                 FROM account_invitations
                 WHERE code = $1::text
                 LIMIT 1",
                &[&code_norm],
            )
            .await?;
        Ok(row_opt.map(row_to_invitation))
    }

    /// Atomically redeem one use of the code. Returns the updated
    /// row. The UPDATE only succeeds if the code is still
    /// redeemable (use_count < max_uses, not revoked, not
    /// expired).
    ///
    /// The caller is responsible for setting `used_by` /
    /// `used_at` in the same transaction (typically by also
    /// inserting the new account row). Here we just bump
    /// `use_count`.
    pub async fn redeem(
        &self,
        id: Uuid,
    ) -> Result<InvitationRow, InvitationError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "UPDATE account_invitations
                 SET use_count = use_count + 1
                 WHERE id = $1::uuid
                   AND use_count < max_uses
                   AND revoked_at IS NULL
                   AND (expires_at IS NULL OR expires_at > NOW())
                 RETURNING id, code, created_by, created_at, used_by, used_at,
                           max_uses, use_count, expires_at, revoked_at, note",
                &[&id],
            )
            .await?;
        row_opt.map(row_to_invitation).ok_or(InvitationError::FullyUsed)
    }

    /// List codes created by a given account (admin / debug view).
    pub async fn list_by_creator(
        &self,
        created_by: Uuid,
    ) -> Result<Vec<InvitationRow>, InvitationError> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "SELECT id, code, created_by, created_at, used_by, used_at,
                        max_uses, use_count, expires_at, revoked_at, note
                 FROM account_invitations
                 WHERE created_by = $1::uuid
                 ORDER BY created_at DESC",
                &[&created_by],
            )
            .await?;
        Ok(rows.into_iter().map(row_to_invitation).collect())
    }
}

fn row_to_invitation(r: Row) -> InvitationRow {
    InvitationRow {
        id: r.get(0),
        code: r.get(1),
        created_by: r.get(2),
        created_at: r.get(3),
        used_by: r.get(4),
        used_at: r.get(5),
        max_uses: r.get(6),
        use_count: r.get(7),
        expires_at: r.get(8),
        revoked_at: r.get(9),
        note: r.get(10),
    }
}

#[derive(Debug, Clone)]
pub struct InvitationRow {
    pub id: Uuid,
    pub code: String,
    pub created_by: Option<Uuid>,
    pub created_at: DateTime<Utc>,
    pub used_by: Option<Uuid>,
    pub used_at: Option<DateTime<Utc>>,
    pub max_uses: i32,
    pub use_count: i32,
    pub expires_at: Option<DateTime<Utc>>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub note: Option<String>,
}
