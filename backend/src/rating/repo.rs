//! Database access for the rating module
//!
//! Touches 1 table: `ratings`. We also need to look up the supervisor
//! (id, review_status) by public_code — that uses the supervisor table.

use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use tokio_postgres::types::ToSql;
use uuid::Uuid;

use super::error::RatingError;

#[derive(Clone)]
pub struct RatingRepo {
    pool: Pool,
}

impl RatingRepo {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Resolve a supervisor's public alias to (id, review_status).
    /// Returns `Ok(None)` if the alias doesn't exist.
    pub async fn find_supervisor_by_alias(
        &self,
        alias: &str,
    ) -> Result<Option<SupervisorLookup>, RatingError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT id, review_status::text
                 FROM supervisors
                 WHERE public_code = $1::text
                 LIMIT 1",
                &[&alias],
            )
            .await?;
        Ok(row_opt.map(|r| SupervisorLookup {
            id: r.get(0),
            review_status: r.get(1),
        }))
    }

    /// Find an existing non-superseded rating for (account, supervisor, dim).
    /// Returns its id, or None.
    pub async fn find_current_rating(
        &self,
        account_id: Uuid,
        supervisor_id: Uuid,
        dim: &str,
    ) -> Result<Option<Uuid>, RatingError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT id FROM ratings
                 WHERE account_id = $1::uuid
                   AND supervisor_id = $2::uuid
                   AND dim = $3::text
                   AND superseded_by IS NULL
                 LIMIT 1",
                &[&account_id, &supervisor_id, &dim],
            )
            .await?;
        Ok(row_opt.map(|r| r.get(0)))
    }

    /// Mark an existing rating as superseded by the new rating id.
    pub async fn mark_superseded(
        &self,
        old_id: Uuid,
        new_id: Uuid,
    ) -> Result<(), RatingError> {
        let c = self.pool.get().await?;
        c.execute(
            "UPDATE ratings SET superseded_by = $1::uuid WHERE id = $2::uuid",
            &[&new_id, &old_id],
        )
        .await?;
        Ok(())
    }

    /// Submit a rating with B-9 supersede semantics.
    ///
    /// Chicken-and-egg problem: the UQ `uq_ratings_one_current` blocks
    /// having two rows with the same (account, supervisor, dim) and
    /// superseded_by IS NULL. But the FK `ratings_superseded_by_fkey`
    /// requires superseded_by to reference an existing row.
    ///
    /// Solution: in one transaction, do 3 steps:
    ///   1. UPDATE old SET superseded_by = old.id (self-reference — satisfies
    ///      the FK because the row exists, AND frees the UQ slot because
    ///      superseded_by is no longer NULL)
    ///   2. INSERT new (now allowed because old has non-NULL superseded_by)
    ///   3. UPDATE old SET superseded_by = new.id (fixes the self-reference
    ///      to point to the real new row, completing the B-9 chain)
    ///
    /// The brief window between steps 1 and 2 (or between 2 and 3) leaves
    /// the old row pointing to itself or to new, but never to a phantom
    /// row. Concurrent reads via /me that land in this window see the new
    /// row as the current (because the old row is no longer NULL superseded).
    ///
    /// If `existing_old_id` is None, just inserts (no supersede chain).
    pub async fn submit_with_supersede(
        &self,
        account_id: Uuid,
        supervisor_id: Uuid,
        dim: &str,
        value: i16,
        discipline_hash: &[u8],
        dim_additional_enc: Option<&[u8]>,
        overall_additional_enc: Option<&[u8]>,
        additional_level: Option<&str>,
        evidence: &[String],
        existing_old_id: Option<Uuid>,
    ) -> Result<Uuid, RatingError> {
        let mut c = self.pool.get().await?;
        let tx = c.transaction().await?;
        let evidence_refs: Vec<&str> = evidence.iter().map(String::as_str).collect();

        let new_id: Uuid = if let Some(old_id) = existing_old_id {
            // Step 1: mark old as superseded-by-self. FK satisfied (row exists),
            // UQ freed (old.superseded_by is no longer NULL).
            tx.execute(
                "UPDATE ratings SET superseded_by = id WHERE id = $1::uuid",
                &[&old_id],
            )
            .await?;

            // Step 2: insert new row.
            let row = tx
                .query_one(
                    "INSERT INTO ratings
                        (account_id, supervisor_id, dim, value, discipline_hash,
                         dim_additional_enc, overall_additional_enc, additional_level,
                         evidence)
                     VALUES ($1::uuid, $2::uuid, $3::text, $4, $5::bytea,
                             $6::bytea, $7::bytea, $8::text, $9::text[])
                     RETURNING id",
                    &[
                        &account_id as &(dyn ToSql + Sync),
                        &supervisor_id as &(dyn ToSql + Sync),
                        &dim as &(dyn ToSql + Sync),
                        &value as &(dyn ToSql + Sync),
                        &discipline_hash as &(dyn ToSql + Sync),
                        &dim_additional_enc as &(dyn ToSql + Sync),
                        &overall_additional_enc as &(dyn ToSql + Sync),
                        &additional_level as &(dyn ToSql + Sync),
                        &evidence_refs as &(dyn ToSql + Sync),
                    ],
                )
                .await?;
            let id: Uuid = row.get(0);

            // Step 3: fix old's superseded_by to point to the real new id.
            tx.execute(
                "UPDATE ratings SET superseded_by = $1::uuid WHERE id = $2::uuid",
                &[&id, &old_id],
            )
            .await?;

            id
        } else {
            // No existing row — just insert.
            let row = tx
                .query_one(
                    "INSERT INTO ratings
                        (account_id, supervisor_id, dim, value, discipline_hash,
                         dim_additional_enc, overall_additional_enc, additional_level,
                         evidence)
                     VALUES ($1::uuid, $2::uuid, $3::text, $4, $5::bytea,
                             $6::bytea, $7::bytea, $8::text, $9::text[])
                     RETURNING id",
                    &[
                        &account_id as &(dyn ToSql + Sync),
                        &supervisor_id as &(dyn ToSql + Sync),
                        &dim as &(dyn ToSql + Sync),
                        &value as &(dyn ToSql + Sync),
                        &discipline_hash as &(dyn ToSql + Sync),
                        &dim_additional_enc as &(dyn ToSql + Sync),
                        &overall_additional_enc as &(dyn ToSql + Sync),
                        &additional_level as &(dyn ToSql + Sync),
                        &evidence_refs as &(dyn ToSql + Sync),
                    ],
                )
                .await?;
            row.get(0)
        };

        tx.commit().await?;
        Ok(new_id)
    }

    /// Insert a new rating. Returns the new id.
    ///
    /// `discipline_hash` is a snapshot of the rater's declared discipline
    /// at submission time (used for aggregation weighting, OUTLINE §5).
    /// Optional P2 fields (additional text) are AES-256-GCM encrypted by
    /// the service before this call.
    #[allow(clippy::too_many_arguments)]
    pub async fn insert_rating(
        &self,
        account_id: Uuid,
        supervisor_id: Uuid,
        dim: &str,
        value: i16,
        discipline_hash: &[u8],
        dim_additional_enc: Option<&[u8]>,
        overall_additional_enc: Option<&[u8]>,
        additional_level: Option<&str>,
        evidence: &[String],
    ) -> Result<Uuid, RatingError> {
        let c = self.pool.get().await?;

        // Postgres TEXT[] parameter — we pass the slice directly, tokio-postgres
        // serialises &[String] (which is &[&str] at the wire level) as text[].
        let evidence_refs: Vec<&str> = evidence.iter().map(String::as_str).collect();

        let row = c
            .query_one(
                "INSERT INTO ratings
                    (account_id, supervisor_id, dim, value, discipline_hash,
                     dim_additional_enc, overall_additional_enc, additional_level,
                     evidence)
                 VALUES ($1::uuid, $2::uuid, $3::text, $4, $5::bytea,
                         $6::bytea, $7::bytea, $8::text,
                         $9::text[])
                 RETURNING id",
                &[
                    &account_id as &(dyn ToSql + Sync),
                    &supervisor_id as &(dyn ToSql + Sync),
                    &dim as &(dyn ToSql + Sync),
                    &value as &(dyn ToSql + Sync),
                    &discipline_hash as &(dyn ToSql + Sync),
                    &dim_additional_enc as &(dyn ToSql + Sync),
                    &overall_additional_enc as &(dyn ToSql + Sync),
                    &additional_level as &(dyn ToSql + Sync),
                    &evidence_refs as &(dyn ToSql + Sync),
                ],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Fetch all of `account_id`'s current ratings for `supervisor_id`
    /// (across all 6 dimensions, with supersede chain visible).
    /// Used by GET /supervisors/{alias}/ratings/me.
    pub async fn list_my_ratings(
        &self,
        account_id: Uuid,
        supervisor_id: Uuid,
    ) -> Result<Vec<RatingRow>, RatingError> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "SELECT id, dim, value, created_at, superseded_by
                 FROM ratings
                 WHERE account_id = $1::uuid
                   AND supervisor_id = $2::uuid
                 ORDER BY created_at DESC",
                &[&account_id, &supervisor_id],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| RatingRow {
                id: r.get(0),
                dim: r.get(1),
                value: r.get(2),
                created_at: r.get(3),
                superseded_by: r.get(4),
            })
            .collect())
    }
}

#[derive(Debug, Clone)]
pub struct SupervisorLookup {
    pub id: Uuid,
    pub review_status: String,
}

#[derive(Debug, Clone)]
pub struct RatingRow {
    pub id: Uuid,
    pub dim: String,
    pub value: i16,
    pub created_at: DateTime<Utc>,
    pub superseded_by: Option<Uuid>,
}
