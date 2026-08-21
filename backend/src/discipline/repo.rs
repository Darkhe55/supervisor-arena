//! Database access for the discipline-weight-voting module.
//!
//! Touches 4 tables: the existing `discipline_weight_votes` (extended in
//! migration 13 with `discipline_code TEXT`), plus the three new ones
//! (`discipline_weight_voters`, `discipline_weights`,
//! `discipline_weight_history`). We hand-write SQL — no ORM.
//!
//! # Note on `discipline_weight_votes.discipline_code`
//!
//! The original M1 schema had a `discipline_hash BYTEA` column. Migration
//! 13 adds `discipline_code TEXT NOT NULL DEFAULT ''` and this module
//! uses that column exclusively for the voting flow. `discipline_hash` is
//! vestigial (will be removed in a later cleanup migration once we are
//! sure nothing else references it).

use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use tokio_postgres::Row;
use uuid::Uuid;

use super::dto::{BallotChoice, VoteSummary, WeightEntry, WeightHistoryEntry};
use super::error::DisciplineError;

const COOLDOWN_DAYS: i64 = 30;
const MIN_AGREE_FOR_APPLY: i32 = 3;
const MIN_ACTIVE_USERS_FOR_APPLY: i64 = 5;
const APPLY_AGREE_RATIO: f64 = 0.6;

/// All 6 dims, in OUTLINE §3 / RADAR_DIMS display order.
const ALL_DIMS: &[&str] = &[
    "research",
    "resource",
    "fit",
    "currency",
    "ethic",
    "tool",
];

#[derive(Clone)]
pub struct DisciplineRepo {
    pool: Pool,
}

impl DisciplineRepo {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    // ---- Constants (re-exported for the service) ----
    pub const COOLDOWN_DAYS: i64 = COOLDOWN_DAYS;
    pub const MIN_AGREE_FOR_APPLY: i32 = MIN_AGREE_FOR_APPLY;
    pub const MIN_ACTIVE_USERS_FOR_APPLY: i64 = MIN_ACTIVE_USERS_FOR_APPLY;
    pub const APPLY_AGREE_RATIO: f64 = APPLY_AGREE_RATIO;
    pub const ALL_DIMS: &'static [&'static str] = ALL_DIMS;

    // ---- Discipline / dim validation ----

    /// Check that a discipline code exists and is active.
    pub async fn discipline_exists(&self, code: &str) -> Result<bool, DisciplineError> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "SELECT EXISTS(SELECT 1 FROM disciplines WHERE code = $1::text AND is_active)",
                &[&code],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Pure check that `dim` is one of the 6 known dim codes. Kept here
    /// so the repo owns the canonical list.
    pub fn is_valid_dim(dim: &str) -> bool {
        ALL_DIMS.contains(&dim)
    }

    // ---- Vote lifecycle ----

    /// Insert a new `pending` proposal. Returns the vote id.
    pub async fn insert_vote(
        &self,
        discipline: &str,
        dim: &str,
        proposed_weight: f64,
        proposer_id: Uuid,
        reason: Option<&str>,
    ) -> Result<Uuid, DisciplineError> {
        let c = self.pool.get().await?;
        // `discipline_hash` (BYTEA NOT NULL) is a vestigial column from
        // the M1 speculative schema — for the M2 voting flow we use
        // `discipline_code`. We still have to write *something* to the
        // old column to satisfy the NOT NULL constraint. We write the
        // UTF-8 bytes of the code (lossless, unique, sortable).
        let code_bytes: Vec<u8> = discipline.as_bytes().to_vec();
        let row = c
            .query_one(
                "INSERT INTO discipline_weight_votes
                    (discipline_hash, discipline_code, dim,
                     proposed_weight, proposer_id, reason)
                 VALUES ($1::bytea, $2::text, $3::text,
                         $4::double precision, $5::uuid, $6::text)
                 RETURNING id",
                &[
                    &code_bytes,
                    &discipline,
                    &dim,
                    &proposed_weight,
                    &proposer_id,
                    &reason,
                ],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Fetch a single vote by id.
    pub async fn find_vote(&self, vote_id: Uuid) -> Result<Option<VoteRow>, DisciplineError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT id, discipline_code, dim, proposed_weight, proposer_id,
                        reason, agree_count, disagree_count, status, applied_at,
                        created_at
                 FROM discipline_weight_votes
                 WHERE id = $1::uuid
                 LIMIT 1",
                &[&vote_id],
            )
            .await?;
        Ok(row_opt.map(row_to_vote))
    }

    /// List `pending` votes for a discipline (newest first).
    pub async fn list_pending_votes(
        &self,
        discipline: &str,
    ) -> Result<Vec<VoteRow>, DisciplineError> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "SELECT id, discipline_code, dim, proposed_weight, proposer_id,
                        reason, agree_count, disagree_count, status, applied_at,
                        created_at
                 FROM discipline_weight_votes
                 WHERE discipline_code = $1::text
                   AND status = 'pending'
                 ORDER BY created_at DESC",
                &[&discipline],
            )
            .await?;
        Ok(rows.into_iter().map(row_to_vote).collect())
    }

    /// List recent votes for a discipline (any status, newest first).
    pub async fn list_recent_votes(
        &self,
        discipline: &str,
        limit: i64,
    ) -> Result<Vec<VoteRow>, DisciplineError> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "SELECT id, discipline_code, dim, proposed_weight, proposer_id,
                        reason, agree_count, disagree_count, status, applied_at,
                        created_at
                 FROM discipline_weight_votes
                 WHERE discipline_code = $1::text
                 ORDER BY created_at DESC
                 LIMIT $2",
                &[&discipline, &limit],
            )
            .await?;
        Ok(rows.into_iter().map(row_to_vote).collect())
    }

    // ---- Ballots (individual votes) ----

    /// Insert an individual ballot, then bump the aggregate count on the
    /// parent vote. All in one transaction. Returns the new
    /// (agree_count, disagree_count) tuple.
    pub async fn cast_ballot(
        &self,
        vote_id: Uuid,
        voter_id: Uuid,
        choice: BallotChoice,
    ) -> Result<(i32, i32), DisciplineError> {
        let mut c = self.pool.get().await?;
        let tx = c.transaction().await?;
        tx.execute(
            "INSERT INTO discipline_weight_voters (vote_id, voter_id, choice)
             VALUES ($1::uuid, $2::uuid, $3::text)",
            &[&vote_id, &voter_id, &choice_label(choice)],
        )
        .await?;
        let bump_agree = if choice == BallotChoice::Agree { 1 } else { 0 };
        let bump_disagree = if choice == BallotChoice::Agree { 0 } else { 1 };
        let row = tx
            .query_one(
                "UPDATE discipline_weight_votes
                 SET agree_count = agree_count + $2,
                     disagree_count = disagree_count + $3
                 WHERE id = $1::uuid
                 RETURNING agree_count, disagree_count",
                &[&vote_id, &bump_agree, &bump_disagree],
            )
            .await?;
        let agree: i32 = row.get(0);
        let disagree: i32 = row.get(1);
        tx.commit().await?;
        Ok((agree, disagree))
    }

    /// Check if `voter_id` already cast a ballot on `vote_id`.
    pub async fn has_voted(
        &self,
        vote_id: Uuid,
        voter_id: Uuid,
    ) -> Result<bool, DisciplineError> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "SELECT EXISTS(
                    SELECT 1 FROM discipline_weight_voters
                    WHERE vote_id = $1::uuid AND voter_id = $2::uuid
                 )",
                &[&vote_id, &voter_id],
            )
            .await?;
        Ok(row.get(0))
    }

    // ---- Eligibility / cooldown ----

    /// Count distinct users with ≥ MIN_VOTER_RATINGS approved ratings in
    /// supervisors of `discipline`. This is the "active users" denominator
    /// for the 60% threshold (OUTLINE §4.4).
    pub async fn count_active_users_in_discipline(
        &self,
        discipline: &str,
    ) -> Result<i64, DisciplineError> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "SELECT COUNT(*)::bigint FROM (
                    SELECT r.account_id
                    FROM ratings r
                    JOIN supervisors s ON s.id = r.supervisor_id
                    WHERE s.discipline = $1::text
                      AND r.review_status = 'approved'
                      AND r.superseded_by IS NULL
                    GROUP BY r.account_id
                    HAVING COUNT(*) >= 3
                 ) AS u",
                &[&discipline],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Eligibility for a specific user: ≥ 3 approved ratings in this discipline?
    pub async fn user_is_eligible(
        &self,
        user_id: Uuid,
        discipline: &str,
    ) -> Result<bool, DisciplineError> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "SELECT (
                    SELECT COUNT(*) FROM ratings r
                    JOIN supervisors s ON s.id = r.supervisor_id
                    WHERE r.account_id = $1::uuid
                      AND s.discipline = $2::text
                      AND r.review_status = 'approved'
                      AND r.superseded_by IS NULL
                 ) >= 3",
                &[&user_id, &discipline],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Cooldown check: has the (discipline, dim) had a *real* weight
    /// applied (i.e. by a user vote, not the bootstrap equal-weights
    /// row) within the last `COOLDOWN_DAYS` days? If so, return the
    /// applied_at. Bootstrap rows have `source_vote_id IS NULL` and
    /// don't count toward the cooldown (H-42 — the bootstrap is
    /// initialization, not a user-driven change).
    pub async fn cooldown_active(
        &self,
        discipline: &str,
        dim: &str,
    ) -> Result<Option<DateTime<Utc>>, DisciplineError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT applied_at FROM discipline_weights
                 WHERE discipline = $1::text AND dim = $2::text
                   AND source_vote_id IS NOT NULL
                   AND applied_at > NOW() - ($3 || ' days')::interval
                 LIMIT 1",
                &[&discipline, &dim, &COOLDOWN_DAYS.to_string()],
            )
            .await?;
        Ok(row_opt.map(|r| r.get(0)))
    }

    // ---- Application ----

    /// Mark a vote as `applied` and set its `applied_at`. Caller is
    /// responsible for first applying the new weights via
    /// `apply_weights` in the service layer.
    pub async fn mark_vote_applied(&self, vote_id: Uuid) -> Result<(), DisciplineError> {
        let c = self.pool.get().await?;
        c.execute(
            "UPDATE discipline_weight_votes
             SET status = 'applied', applied_at = NOW()
             WHERE id = $1::uuid AND status = 'pending'",
            &[&vote_id],
        )
        .await?;
        Ok(())
    }

    /// Mark a vote as `rejected`. Used for proposals that fail the
    /// threshold on apply, or that the service decides to reject for
    /// other reasons.
    pub async fn mark_vote_rejected(&self, vote_id: Uuid) -> Result<(), DisciplineError> {
        let c = self.pool.get().await?;
        c.execute(
            "UPDATE discipline_weight_votes
             SET status = 'rejected'
             WHERE id = $1::uuid AND status = 'pending'",
            &[&vote_id],
        )
        .await?;
        Ok(())
    }

    // ---- Live weights + history ----

    /// Fetch the live weight for every dim in `discipline`. Always
    /// returns 6 rows (one per dim) — the bootstrap insert in
    /// migration 13 guarantees this.
    pub async fn get_current_weights(
        &self,
        discipline: &str,
    ) -> Result<Vec<WeightRow>, DisciplineError> {
        let c = self.pool.get().await?;
        // Pass ALL_DIMS as a `Vec<&str>` so tokio-postgres sends it as
        // a real TEXT[] array (the `&&[&str]` form would deref to
        // `&[&str]` which the postgres driver refuses). Use a Vec to
        // own the strings.
        let dims_vec: Vec<&str> = ALL_DIMS.to_vec();
        let rows = c
            .query(
                "SELECT discipline, dim, weight, source_vote_id, applied_at
                 FROM discipline_weights
                 WHERE discipline = $1::text
                 ORDER BY array_position($2::text[], dim)",
                &[&discipline, &dims_vec],
            )
            .await?;
        Ok(rows.into_iter().map(row_to_weight).collect())
    }

    /// Upsert the weight for `(discipline, dim)`. Called by the service
    /// after renormalization. The caller is responsible for reading the
    /// old weight (via `get_old_weight`) and logging a history row
    /// (via `insert_history`) **before** calling this.
    pub async fn upsert_weight(
        &self,
        discipline: &str,
        dim: &str,
        weight: f64,
        source_vote_id: Option<Uuid>,
    ) -> Result<(), DisciplineError> {
        let c = self.pool.get().await?;
        c.execute(
            "INSERT INTO discipline_weights
                (discipline, dim, weight, source_vote_id, applied_at)
             VALUES ($1::text, $2::text, $3::double precision, $4, NOW())
             ON CONFLICT (discipline, dim) DO UPDATE
               SET weight = EXCLUDED.weight,
                   source_vote_id = EXCLUDED.source_vote_id,
                   applied_at = NOW()",
            &[&discipline, &dim, &weight, &source_vote_id],
        )
        .await?;
        Ok(())
    }

    /// Read a single (discipline, dim) live weight. Returns `None` if no
    /// row exists (defensive — the bootstrap should prevent this).
    pub async fn get_old_weight(
        &self,
        discipline: &str,
        dim: &str,
    ) -> Result<Option<f64>, DisciplineError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT weight FROM discipline_weights
                 WHERE discipline = $1::text AND dim = $2::text
                 LIMIT 1",
                &[&discipline, &dim],
            )
            .await?;
        Ok(row_opt.map(|r| r.get(0)))
    }

    /// Append a history event.
    pub async fn insert_history(
        &self,
        discipline: &str,
        dim: &str,
        old_weight: Option<f64>,
        new_weight: f64,
        action: &str,
        actor_id: Option<Uuid>,
        source_vote_id: Option<Uuid>,
    ) -> Result<Uuid, DisciplineError> {
        let c = self.pool.get().await?;
        let row = c
            .query_one(
                "INSERT INTO discipline_weight_history
                    (discipline, dim, old_weight, new_weight, action, actor_id, source_vote_id)
                 VALUES ($1::text, $2::text, $3, $4::double precision, $5::text, $6, $7)
                 RETURNING id",
                &[
                    &discipline,
                    &dim,
                    &old_weight,
                    &new_weight,
                    &action,
                    &actor_id,
                    &source_vote_id,
                ],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Fetch history rows for a (discipline, optional dim) pair, newest first.
    pub async fn list_weight_history(
        &self,
        discipline: &str,
        dim: Option<&str>,
        limit: i64,
    ) -> Result<Vec<WeightHistoryEntry>, DisciplineError> {
        let c = self.pool.get().await?;
        let rows = match dim {
            Some(d) => {
                c.query(
                    "SELECT id, discipline, dim, old_weight, new_weight, action,
                            source_vote_id, actor_id, created_at
                     FROM discipline_weight_history
                     WHERE discipline = $1::text AND dim = $2::text
                     ORDER BY created_at DESC
                     LIMIT $3",
                    &[&discipline, &d, &limit],
                )
                .await?
            }
            None => {
                c.query(
                    "SELECT id, discipline, dim, old_weight, new_weight, action,
                            source_vote_id, actor_id, created_at
                     FROM discipline_weight_history
                     WHERE discipline = $1::text
                     ORDER BY created_at DESC
                     LIMIT $2",
                    &[&discipline, &limit],
                )
                .await?
            }
        };
        Ok(rows
            .into_iter()
            .map(|r| WeightHistoryEntry {
                id: r.get(0),
                discipline: r.get(1),
                dim: r.get(2),
                old_weight: r.get::<_, Option<f64>>(3),
                new_weight: r.get(4),
                action: r.get(5),
                source_vote_id: r.get::<_, Option<Uuid>>(6),
                actor_id: r.get::<_, Option<Uuid>>(7),
                created_at: r.get(8),
            })
            .collect())
    }

    // ---- Summary helpers (for the handler / DTO) ----

    /// Build a `VoteSummary` from a `VoteRow` + the active-user count for
    /// the (discipline, dim) bucket.
    pub fn summarize(
        row: &VoteRow,
        active_users: i64,
    ) -> VoteSummary {
        let total = (row.agree_count + row.disagree_count) as f64;
        let ratio = if total > 0.0 {
            row.agree_count as f64 / total
        } else {
            0.0
        };
        let ready = row.status == "pending"
            && row.agree_count >= MIN_AGREE_FOR_APPLY
            && active_users >= MIN_ACTIVE_USERS_FOR_APPLY
            && ratio >= APPLY_AGREE_RATIO;
        VoteSummary {
            vote_id: row.id,
            discipline: row.discipline_code.clone(),
            dim: row.dim.clone(),
            proposed_weight: row.proposed_weight,
            reason: row.reason.clone(),
            proposer_id: row.proposer_id,
            agree_count: row.agree_count,
            disagree_count: row.disagree_count,
            status: row.status.clone(),
            applied_at: row.applied_at,
            created_at: row.created_at,
            ready_to_apply: ready,
        }
    }

    /// Build a `VoteDetail` (single-vote variant) from a `VoteRow` +
    /// active-user count.
    pub fn detail(row: &VoteRow, active_users: i64) -> super::dto::VoteDetail {
        let total = (row.agree_count + row.disagree_count) as f64;
        let ratio = if total > 0.0 {
            row.agree_count as f64 / total
        } else {
            0.0
        };
        let ready = row.status == "pending"
            && row.agree_count >= MIN_AGREE_FOR_APPLY
            && active_users >= MIN_ACTIVE_USERS_FOR_APPLY
            && ratio >= APPLY_AGREE_RATIO;
        super::dto::VoteDetail {
            vote_id: row.id,
            discipline: row.discipline_code.clone(),
            dim: row.dim.clone(),
            proposed_weight: row.proposed_weight,
            agree_count: row.agree_count,
            disagree_count: row.disagree_count,
            status: row.status.clone(),
            applied_at: row.applied_at,
            created_at: row.created_at,
            threshold_met: ready,
        }
    }

    /// Convert a `BTreeMap<dim, weight>` to a `Vec<WeightEntry>` in
    /// `ALL_DIMS` order, for the public API.
    pub fn weight_map_to_entries(
        map: &std::collections::BTreeMap<
            String,
            (f64, Option<Uuid>, DateTime<Utc>),
        >,
    ) -> Vec<WeightEntry> {
        let mut out = Vec::with_capacity(ALL_DIMS.len());
        for &d in ALL_DIMS {
            if let Some((w, src, at)) = map.get(d) {
                out.push(WeightEntry {
                    dim: d.to_string(),
                    weight: *w,
                    applied_at: *at,
                    source_vote_id: *src,
                });
            }
        }
        out
    }
}

fn choice_label(choice: BallotChoice) -> &'static str {
    match choice {
        BallotChoice::Agree => "agree",
        BallotChoice::Disagree => "disagree",
    }
}

fn row_to_vote(r: Row) -> VoteRow {
    VoteRow {
        id: r.get(0),
        discipline_code: r.get(1),
        dim: r.get(2),
        proposed_weight: r.get(3),
        proposer_id: r.get(4),
        reason: r.get(5),
        agree_count: r.get(6),
        disagree_count: r.get(7),
        status: r.get(8),
        applied_at: r.get(9),
        created_at: r.get(10),
    }
}

fn row_to_weight(r: Row) -> WeightRow {
    WeightRow {
        discipline: r.get(0),
        dim: r.get(1),
        weight: r.get(2),
        source_vote_id: r.get::<_, Option<Uuid>>(3),
        applied_at: r.get(4),
    }
}

#[derive(Debug, Clone)]
pub struct VoteRow {
    pub id: Uuid,
    pub discipline_code: String,
    pub dim: String,
    pub proposed_weight: f64,
    pub proposer_id: Uuid,
    pub reason: Option<String>,
    pub agree_count: i32,
    pub disagree_count: i32,
    pub status: String,
    pub applied_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct WeightRow {
    pub discipline: String,
    pub dim: String,
    pub weight: f64,
    pub source_vote_id: Option<Uuid>,
    pub applied_at: DateTime<Utc>,
}
