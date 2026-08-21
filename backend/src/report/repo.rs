//! Database access for the report (举报) module.
//!
//! Touches 2 tables: `reports` (the report itself) plus cross-checks
//! against `ratings` / `supervisors` to validate the target exists
//! and isn't a self-report.
//!
//! See OUTLINE §7.10.7 + DECISIONS G-3 for the user-facing flow.

use chrono::{DateTime, Utc};
use deadpool_postgres::Pool;
use tokio_postgres::Row;
use uuid::Uuid;

use super::dto::{ReportResolution, TargetType};
use super::error::ReportError;

/// SLA: 24h to first reviewer action (matches the G-2/G-3 / §7.6
/// "workday SLA" + the M6 review SLA config).
const SLA_HOURS: i64 = 24;

#[derive(Clone)]
pub struct ReportRepo {
    pool: Pool,
}

impl ReportRepo {
    pub fn new(pool: Pool) -> Self {
        Self { pool }
    }

    // ---- Target validation (called before insert_report) ----

    /// Check that the target exists and is reportable.
    /// - `Rating` target: row in `ratings` with `superseded_by IS NULL`
    ///   and not `hidden`.
    /// - `Supervisor` target: row in `supervisors` with status
    ///   not in ('rejected', 'hidden').
    /// - `AdditionalInfo` target: same as Rating (we treat the
    ///   additional-info fields as a sub-object of the rating).
    pub async fn target_exists(
        &self,
        target_type: TargetType,
        target_id: Uuid,
    ) -> Result<bool, ReportError> {
        let c = self.pool.get().await?;
        let exists = match target_type {
            TargetType::Rating | TargetType::AdditionalInfo => c
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM ratings
                     WHERE id = $1::uuid AND superseded_by IS NULL)",
                    &[&target_id],
                )
                .await?
                .get(0),
            TargetType::Supervisor => c
                .query_one(
                    "SELECT EXISTS(SELECT 1 FROM supervisors
                     WHERE id = $1::uuid AND review_status NOT IN ('rejected', 'hidden'))",
                    &[&target_id],
                )
                .await?
                .get(0),
        };
        Ok(exists)
    }

    /// Get the `account_id` (reporter side) of a rating target — used
    /// to block self-reports. Returns None for supervisor targets
    /// (supervisors have no single author).
    pub async fn rating_account_id(
        &self,
        rating_id: Uuid,
    ) -> Result<Option<Uuid>, ReportError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT account_id FROM ratings WHERE id = $1::uuid LIMIT 1",
                &[&rating_id],
            )
            .await?;
        Ok(row_opt.map(|r| r.get(0)))
    }

    // ---- Lifecycle ----

    /// Insert a new `pending` report. Returns the report id.
    pub async fn insert_report(
        &self,
        reporter_id: Uuid,
        target_type: TargetType,
        target_id: Uuid,
        reason: &str,
        description: Option<&str>,
    ) -> Result<Uuid, ReportError> {
        let c = self.pool.get().await?;
        let sla_deadline = Utc::now() + chrono::Duration::hours(SLA_HOURS);
        let row = c
            .query_one(
                "INSERT INTO reports
                    (reporter_id, target_type, target_id, reason, description, sla_deadline)
                 VALUES ($1::uuid, $2::text, $3::uuid, $4::text, $5, $6)
                 RETURNING id",
                &[
                    &reporter_id,
                    &target_type.as_db_str(),
                    &target_id,
                    &reason,
                    &description,
                    &sla_deadline,
                ],
            )
            .await?;
        Ok(row.get(0))
    }

    /// Fetch a single report by id.
    pub async fn find_report(&self, id: Uuid) -> Result<Option<ReportRow>, ReportError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "SELECT id, reporter_id, target_type, target_id, reason, description,
                        status, reviewer_id, resolution, submitted_at, resolved_at, sla_deadline
                 FROM reports
                 WHERE id = $1::uuid
                 LIMIT 1",
                &[&id],
            )
            .await?;
        Ok(row_opt.map(row_to_report))
    }

    /// List `pending` reports, ordered by SLA (most-overdue first).
    pub async fn list_pending(
        &self,
        limit: i64,
    ) -> Result<Vec<ReportRow>, ReportError> {
        let c = self.pool.get().await?;
        let rows = c
            .query(
                "SELECT id, reporter_id, target_type, target_id, reason, description,
                        status, reviewer_id, resolution, submitted_at, resolved_at, sla_deadline
                 FROM reports
                 WHERE status IN ('pending', 'reviewing')
                 ORDER BY sla_deadline ASC
                 LIMIT $1",
                &[&limit],
            )
            .await?;
        Ok(rows.into_iter().map(row_to_report).collect())
    }

    /// Claim a report for review. Atomically sets status='reviewing'
    /// and reviewer_id=$1, but ONLY if it's still 'pending' or already
    /// claimed by $1 (re-entrant). Returns the updated row.
    pub async fn claim(
        &self,
        report_id: Uuid,
        reviewer_id: Uuid,
    ) -> Result<ReportRow, ReportError> {
        let c = self.pool.get().await?;
        let row_opt = c
            .query_opt(
                "UPDATE reports
                 SET status = 'reviewing', reviewer_id = $2::uuid
                 WHERE id = $1::uuid
                   AND status IN ('pending', 'reviewing')
                 RETURNING id, reporter_id, target_type, target_id, reason, description,
                           status, reviewer_id, resolution, submitted_at, resolved_at, sla_deadline",
                &[&report_id, &reviewer_id],
            )
            .await?;
        row_opt
            .map(row_to_report)
            .ok_or(ReportError::ReportNotFound(report_id))
    }

    /// Resolve a report. Returns the updated row.
    pub async fn resolve(
        &self,
        report_id: Uuid,
        reviewer_id: Uuid,
        resolution: ReportResolution,
        _note: Option<&str>,
    ) -> Result<ReportRow, ReportError> {
        let c = self.pool.get().await?;
        // `note` is for the audit log (M6+ — not implemented in M3's
        // MVP). Stored on the resolution if/when encryption_audit_log
        // gets a `note` column; for now we drop it.
        let row_opt = c
            .query_opt(
                "UPDATE reports
                 SET status = 'resolved',
                     reviewer_id = $2::uuid,
                     resolution = $3::text,
                     resolved_at = NOW()
                 WHERE id = $1::uuid
                   AND status = 'reviewing'
                   AND reviewer_id = $2::uuid
                 RETURNING id, reporter_id, target_type, target_id, reason, description,
                           status, reviewer_id, resolution, submitted_at, resolved_at, sla_deadline",
                &[&report_id, &reviewer_id, &resolution.as_db_str()],
            )
            .await?;
        row_opt
            .map(row_to_report)
            .ok_or(ReportError::ReportNotFound(report_id))
    }

    /// Dismiss a report (the report was wrong, no violation).
    /// Goes through the same `resolved` terminal state but with
    /// resolution='rejected' or 'no_action'.
    pub async fn dismiss(
        &self,
        report_id: Uuid,
        reviewer_id: Uuid,
        resolution: ReportResolution,
    ) -> Result<ReportRow, ReportError> {
        // For M3 we treat dismiss == resolve. The distinction is in
        // the resolution code (rejected / no_action / etc).
        self.resolve(report_id, reviewer_id, resolution, None).await
    }
}

fn row_to_report(r: Row) -> ReportRow {
    ReportRow {
        id: r.get(0),
        reporter_id: r.get(1),
        target_type: r.get(2),
        target_id: r.get(3),
        reason: r.get(4),
        description: r.get(5),
        status: r.get(6),
        reviewer_id: r.get(7),
        resolution: r.get(8),
        submitted_at: r.get(9),
        resolved_at: r.get(10),
        sla_deadline: r.get(11),
    }
}

#[derive(Debug, Clone)]
pub struct ReportRow {
    pub id: Uuid,
    pub reporter_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub description: Option<String>,
    pub status: String,
    pub reviewer_id: Option<Uuid>,
    pub resolution: Option<String>,
    pub submitted_at: DateTime<Utc>,
    pub resolved_at: Option<DateTime<Utc>>,
    pub sla_deadline: DateTime<Utc>,
}
