//! Database access for the supervisor module.
//!
//! Touches 4 tables: `supervisors`, `supervisor_name_mappings`,
//! `supervisor_creation_requests`, plus the `disciplines` / `colleges`
//! lookup tables for validation. We hand-write SQL (no ORM) so the
//! encryption / hash columns and the dedup-by-hash invariant are obvious.

use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use tokio_postgres::Row;
use uuid::Uuid;

use super::dto::PendingReviewEntry;
use super::error::SupervisorError;

/// Repository for all supervisor-related tables.
#[derive(Clone)]
pub struct SupervisorRepo {
    pool: Pool,
}

impl SupervisorRepo {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    /// Check that a discipline code exists and is active.
    pub async fn discipline_exists(&self, code: &str) -> Result<bool, SupervisorError> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM disciplines WHERE code = $1::text AND is_active)",
                &[&code],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Check that a college code exists and is active.
    pub async fn college_exists(&self, code: &str) -> Result<bool, SupervisorError> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM colleges WHERE code = $1::text AND is_active)",
                &[&code],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Look up a mapping by the dedup triple (submitted_name_hash,
    /// discipline_hash, college_hash). Returns the existing alias if
    /// present (G-19 dedup invariant).
    pub async fn find_mapping_by_dedup(
        &self,
        name_hash: &[u8],
        discipline_hash: &[u8],
        college_hash: &[u8],
    ) -> Result<Option<ExistingMapping>, SupervisorError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT supervisor_id, generated_alias
                 FROM supervisor_name_mappings
                 WHERE submitted_name_hash = $1::bytea
                   AND discipline_hash = $2::bytea
                   AND college_hash = $3::bytea
                 LIMIT 1",
                &[&name_hash, &discipline_hash, &college_hash],
            )
            .await?;
        Ok(row_opt.map(|r| ExistingMapping {
            supervisor_id: r.get(0),
            alias: r.get(1),
        }))
    }

    /// Insert a new creation request (always pending_review on M5b).
    /// Returns the request id.
    pub async fn insert_creation_request(
        &self,
        submitter_id: Uuid,
        submitted_name: &str,
        discipline: &str,
        college: &str,
        sla_deadline: DateTime<Utc>,
    ) -> Result<Uuid, SupervisorError> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "INSERT INTO supervisor_creation_requests
                    (submitter_id, submitted_name, discipline, college, sla_deadline)
                 VALUES ($1::uuid, $2::text, $3::text, $4::text, $5)
                 RETURNING id",
                &[
                    &submitter_id,
                    &submitted_name,
                    &discipline,
                    &college,
                    &sla_deadline,
                ],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Fetch a creation request by id (reviewer path).
    pub async fn find_request_by_id(&self, id: Uuid) -> Result<Option<CreationRequest>, SupervisorError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT id, submitter_id, submitted_name, discipline, college,
                        review_status, sla_deadline, created_at
                 FROM supervisor_creation_requests
                 WHERE id = $1::uuid
                 LIMIT 1",
                &[&id],
            )
            .await?;
        Ok(row_opt.map(|r| CreationRequest {
            id: r.get(0),
            submitter_id: r.get(1),
            submitted_name: r.get(2),
            discipline: r.get(3),
            college: r.get(4),
            review_status: r.get(5),
            sla_deadline: r.get(6),
            created_at: r.get(7),
        }))
    }

    /// List pending-review requests (newest first). Used by the reviewer queue.
    pub async fn list_pending_review(&self, limit: i64) -> Result<Vec<PendingReviewEntry>, SupervisorError> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "SELECT id, submitter_id, submitted_name, discipline, college,
                        created_at, sla_deadline
                 FROM supervisor_creation_requests
                 WHERE review_status = 'pending_review'
                 ORDER BY created_at ASC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows
            .into_iter()
            .map(|r| PendingReviewEntry {
                request_id: r.get(0),
                submitter_id: r.get(1),
                submitted_name: r.get(2),
                discipline: r.get(3),
                college: r.get(4),
                created_at: r.get(5),
                sla_deadline: r.get(6),
            })
            .collect())
    }

    /// Approve a creation request: insert supervisor + mapping, mark request
    /// resolved, recompute k-anonymity count for (discipline, college).
    /// All in one transaction.
    ///
    /// `created_by` in the mapping row is the original submitter (audit),
    /// not the reviewer. Reviewer is recorded on the request row.
    pub async fn approve_request(
        &self,
        request_id: Uuid,
        submitter_id: Uuid,
        reviewer_id: Uuid,
        public_code: &str,
        discipline: &str,
        college: &str,
        submitted_name_enc: &[u8],
        submitted_name_hash: &[u8],
        discipline_hash: &[u8],
        college_hash: &[u8],
    ) -> Result<Uuid, SupervisorError> {
        let mut c = self.pool.get().await?;
        let tx = c.transaction().await?;

        // 1. Insert supervisor (status=approved from the start — this is
        //    the actual approval step, not a request).
        let supervisor_id: Uuid = tx
            .query_one(
                "INSERT INTO supervisors (public_code, discipline, college, review_status)
                 VALUES ($1::text, $2::text, $3::text, 'approved')
                 RETURNING id",
                &[&public_code, &discipline, &college],
            )
            .await?
            .get(0);

        // 2. Insert mapping (created_by = original submitter for audit).
        tx.execute(
            "INSERT INTO supervisor_name_mappings
                (supervisor_id, submitted_name_enc, submitted_name_hash,
                 discipline_hash, college_hash, generated_alias, alias_generation_seed,
                 created_by)
             VALUES ($1::uuid, $2::bytea, $3::bytea, $4::bytea, $5::bytea, $6::text, $7::text, $8::uuid)",
            &[
                &supervisor_id,
                &submitted_name_enc,
                &submitted_name_hash,
                &discipline_hash,
                &college_hash,
                &public_code,
                &public_code,
                &submitter_id, // <-- submitter, not reviewer
            ],
        )
        .await?;

        // 3. Mark request resolved.
        tx.execute(
            "UPDATE supervisor_creation_requests
             SET review_status = 'approved',
                 reviewer_id = $2::uuid,
                 resolved_at = NOW(),
                 resolved_supervisor_id = $3::uuid
             WHERE id = $1::uuid",
            &[&request_id, &reviewer_id, &supervisor_id],
        )
        .await?;

        // 4. Recompute k-anonymity count for this (discipline, college) bucket.
        //    Note: k_anonymity_count column is INTEGER (4-byte), so we cast
        //    i64 → i32 to match. (Postgres won't auto-narrow int8 → int4.)
        let k: i32 = tx
            .query_one(
                "SELECT COUNT(*)::int FROM supervisors
                 WHERE review_status = 'approved'
                   AND discipline = $1::text AND college = $2::text",
                &[&discipline, &college],
            )
            .await?
            .get(0);

        tx.execute(
            "UPDATE supervisors
             SET k_anonymity_count = $1
             WHERE review_status = 'approved'
               AND discipline = $2::text
               AND college = $3::text",
            &[&k, &discipline, &college],
        )
        .await?;

        tx.commit().await?;
        Ok(supervisor_id)
    }

    /// Reject a creation request.
    pub async fn reject_request(
        &self,
        request_id: Uuid,
        reviewer_id: Uuid,
        notes: Option<&str>,
    ) -> Result<(), SupervisorError> {
        let c = self.pool.get().await?;
        c.execute(
            "UPDATE supervisor_creation_requests
             SET review_status = 'rejected',
                 reviewer_id = $2::uuid,
                 resolved_at = NOW(),
                 review_notes = $3
             WHERE id = $1::uuid",
            &[&request_id, &reviewer_id, &notes],
        )
        .await?;
        Ok(())
    }

    /// Look up a supervisor by its public alias.
    pub async fn find_by_alias(&self, alias: &str) -> Result<Option<SupervisorRow>, SupervisorError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT id, public_code, discipline, college, review_status,
                        k_anonymity_count, composite_score, created_at
                 FROM supervisors
                 WHERE public_code = $1::text
                 LIMIT 1",
                &[&alias],
            )
            .await?;
        Ok(row_opt.map(row_to_supervisor))
    }

    /// Get the count of approved ratings for a supervisor.
    pub async fn rating_count(&self, supervisor_id: Uuid) -> Result<i64, SupervisorError> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "SELECT COUNT(*) FROM ratings
                 WHERE supervisor_id = $1::uuid AND review_status = 'approved'",
                &[&supervisor_id],
            )
            .await?;
        Ok(row.get(0))
    }
}

// --- Internal row types used by the repo ---

#[derive(Debug, Clone)]
pub struct ExistingMapping {
    pub supervisor_id: Uuid,
    pub alias: String,
}

#[derive(Debug, Clone)]
pub struct CreationRequest {
    pub id: Uuid,
    pub submitter_id: Uuid,
    pub submitted_name: String,
    pub discipline: String,
    pub college: String,
    pub review_status: String,
    pub sla_deadline: DateTime<Utc>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SupervisorRow {
    pub id: Uuid,
    pub public_code: String,
    pub discipline: String,
    pub college: String,
    pub review_status: String,
    pub k_anonymity_count: i32,
    pub composite_score: Option<f64>,
    pub created_at: DateTime<Utc>,
}

fn row_to_supervisor(r: Row) -> SupervisorRow {
    SupervisorRow {
        id: r.get(0),
        public_code: r.get(1),
        discipline: r.get(2),
        college: r.get(3),
        review_status: r.get(4),
        k_anonymity_count: r.get(5),
        composite_score: r.get(6),
        created_at: r.get(7),
    }
}
