//! Business logic for the report (举报) module.
//!
//! Pure helpers are split out and unit-tested (SLA calculation, status
//! transitions). I/O-backed operations live in `ReportRepo`.
//!
//! # Flow
//!
//! 1. Any authed user submits a report (POST /reports). Self-reports
//!    are blocked (E-2 / H-49). The target must exist.
//! 2. A moderator (reviewer) lists the pending queue
//!    (GET /reports/queue) and claims a report (POST /reports/:id/claim).
//!    A report is `pending` until claimed, then `reviewing`.
//! 3. The reviewer resolves (POST /reports/:id/resolve). The terminal
//!    `status='resolved'` with a `resolution` of removed / warned /
//!    rejected / no_action.
//! 4. The rating / supervisor row is NOT auto-mutated by the report
//!    service — that side-effect lives in the rating/supervisor
//!    services (called by the reviewer flow in a future M5+ commit).
//!    M3's MVP just stores the resolution; the side-effect is
//!    intentionally a separate write so it's auditable.
//!
//! # SLA (H-50)
//!
//! - `submitted_at + 24h` = `sla_deadline`
//! - `sla_breached = now() > sla_deadline AND status = 'pending'`
//!   (claim resets the SLA tracking — the report is now in the
//!   reviewer's hands)

use chrono::{DateTime, Utc};
use uuid::Uuid;

use super::dto::{
    ReportDetail, ReportReason, ReportResolution, ReportSummary, SubmitReportRequest,
    TargetType,
};
use super::error::ReportError;
use super::repo::{ReportRepo, ReportRow};

const DESCRIPTION_MAX: usize = 2000;
const NOTE_MAX: usize = 2000;

#[derive(Clone)]
pub struct ReportService {
    repo: ReportRepo,
}

impl ReportService {
    pub fn new(repo: ReportRepo) -> Self {
        Self { repo }
    }

    // ---- Pure helpers (unit-tested below) ----

    /// Pure: has the report's SLA been breached (still pending and past
    /// the deadline)?
    pub fn sla_breached(row: &ReportRow, now: DateTime<Utc>) -> bool {
        row.status == "pending" && now > row.sla_deadline
    }

    /// Pure: format a ReportRow into a public ReportSummary.
    pub fn summarize(row: &ReportRow, now: DateTime<Utc>) -> ReportSummary {
        ReportSummary {
            report_id: row.id,
            target_type: row.target_type.clone(),
            target_id: row.target_id,
            reason: row.reason.clone(),
            status: row.status.clone(),
            submitted_at: row.submitted_at,
            sla_deadline: row.sla_deadline,
            reporter_id: row.reporter_id,
            sla_breached: Self::sla_breached(row, now),
        }
    }

    /// Pure: format a ReportRow into a detailed ReportDetail.
    pub fn detail(row: &ReportRow, now: DateTime<Utc>) -> ReportDetail {
        ReportDetail {
            report_id: row.id,
            reporter_id: row.reporter_id,
            target_type: row.target_type.clone(),
            target_id: row.target_id,
            reason: row.reason.clone(),
            description: row.description.clone(),
            status: row.status.clone(),
            reviewer_id: row.reviewer_id,
            resolution: row.resolution.clone(),
            submitted_at: row.submitted_at,
            resolved_at: row.resolved_at,
            sla_deadline: row.sla_deadline,
            sla_breached: Self::sla_breached(row, now),
        }
    }

    /// Pure: validate a SubmitReportRequest. Returns Err on out-of-range
    /// / missing fields.
    pub fn validate_submit(req: &SubmitReportRequest) -> Result<(), ReportError> {
        if let Some(d) = &req.description {
            if d.len() > DESCRIPTION_MAX {
                return Err(ReportError::TextTooLong(d.len()));
            }
        }
        // target_type and reason are enums (deserialized via serde),
        // so the variants are already validated.
        Ok(())
    }

    // ---- I/O-backed operations ----

    pub async fn submit_report(
        &self,
        reporter_id: Uuid,
        req: SubmitReportRequest,
    ) -> Result<Uuid, ReportError> {
        Self::validate_submit(&req)?;

        // 1. Self-report check (rating targets only).
        if req.target_type == TargetType::Rating
            || req.target_type == TargetType::AdditionalInfo
        {
            if let Some(owner) = self
                .repo
                .rating_account_id(req.target_id)
                .await?
            {
                if owner == reporter_id {
                    return Err(ReportError::SelfReport);
                }
            }
        }

        // 2. Target must exist.
        if !self
            .repo
            .target_exists(req.target_type, req.target_id)
            .await?
        {
            return Err(ReportError::TargetNotFound {
                target_type: req.target_type.as_db_str().to_string(),
                target_id: req.target_id,
            });
        }

        // 3. Insert.
        let report_id = self
            .repo
            .insert_report(
                reporter_id,
                req.target_type,
                req.target_id,
                req.reason.as_db_str(),
                req.description.as_deref(),
            )
            .await?;
        Ok(report_id)
    }

    pub async fn list_pending(
        &self,
        limit: i64,
    ) -> Result<Vec<ReportSummary>, ReportError> {
        let rows = self.repo.list_pending(limit).await?;
        let now = Utc::now();
        Ok(rows.iter().map(|r| Self::summarize(r, now)).collect())
    }

    pub async fn get(
        &self,
        report_id: Uuid,
    ) -> Result<Option<ReportDetail>, ReportError> {
        let row = match self.repo.find_report(report_id).await? {
            Some(r) => r,
            None => return Ok(None),
        };
        Ok(Some(Self::detail(&row, Utc::now())))
    }

    pub async fn claim(
        &self,
        report_id: Uuid,
        reviewer_id: Uuid,
    ) -> Result<ReportDetail, ReportError> {
        let row = self.repo.claim(report_id, reviewer_id).await?;
        Ok(Self::detail(&row, Utc::now()))
    }

    pub async fn resolve(
        &self,
        report_id: Uuid,
        reviewer_id: Uuid,
        resolution: ReportResolution,
        note: Option<&str>,
    ) -> Result<ReportDetail, ReportError> {
        if let Some(n) = note {
            if n.len() > NOTE_MAX {
                return Err(ReportError::TextTooLong(n.len()));
            }
        }
        let row = self
            .repo
            .resolve(report_id, reviewer_id, resolution, note)
            .await?;
        Ok(Self::detail(&row, Utc::now()))
    }
}

// Suppress unused import for the variants we use only via
// `submit_report` / `resolve` JSON paths.
#[allow(dead_code)]
fn _phantom_reason(_: ReportReason) {}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;

    fn row(
        id: Uuid,
        status: &str,
        submitted_secs_ago: i64,
        sla_total_secs: i64,
    ) -> ReportRow {
        let now = Utc::now();
        ReportRow {
            id,
            reporter_id: Uuid::nil(),
            target_type: "rating".to_string(),
            target_id: Uuid::nil(),
            reason: "defamation".to_string(),
            description: None,
            status: status.to_string(),
            reviewer_id: None,
            resolution: None,
            submitted_at: now - Duration::seconds(submitted_secs_ago),
            resolved_at: None,
            sla_deadline: now - Duration::seconds(submitted_secs_ago) + Duration::seconds(sla_total_secs),
        }
    }

    // ---- sla_breached ----

    #[test]
    fn sla_not_breached_when_pending_and_under_deadline() {
        let r = row(Uuid::new_v4(), "pending", 60, 24 * 3600); // 1 min ago, 24h SLA
        assert!(!ReportService::sla_breached(&r, Utc::now()));
    }

    #[test]
    fn sla_breached_when_pending_and_past_deadline() {
        let r = row(Uuid::new_v4(), "pending", 48 * 3600, 24 * 3600); // 48h ago, 24h SLA
        assert!(ReportService::sla_breached(&r, Utc::now()));
    }

    #[test]
    fn sla_not_breached_when_claimed_or_resolved() {
        // Even if the deadline is in the past, a 'reviewing' report
        // is no longer pending → not breached (someone is on it).
        let r_reviewing = row(Uuid::new_v4(), "reviewing", 48 * 3600, 24 * 3600);
        assert!(!ReportService::sla_breached(&r_reviewing, Utc::now()));

        let r_resolved = row(Uuid::new_v4(), "resolved", 48 * 3600, 24 * 3600);
        assert!(!ReportService::sla_breached(&r_resolved, Utc::now()));
    }

    #[test]
    fn sla_boundary_exactly_at_deadline_is_not_breached() {
        // The check is `now > deadline` (strict), so at the exact
        // moment of the deadline we report NOT breached. This gives
        // the reviewer a 1-second grace window.
        let now = Utc::now();
        let r = ReportRow {
            id: Uuid::new_v4(),
            reporter_id: Uuid::nil(),
            target_type: "rating".to_string(),
            target_id: Uuid::nil(),
            reason: "defamation".to_string(),
            description: None,
            status: "pending".to_string(),
            reviewer_id: None,
            resolution: None,
            submitted_at: now - Duration::seconds(24 * 3600),
            resolved_at: None,
            sla_deadline: now,
        };
        assert!(!ReportService::sla_breached(&r, now));
    }

    // ---- validate_submit ----

    #[test]
    fn validate_accepts_clean_request() {
        let req = SubmitReportRequest {
            target_type: TargetType::Rating,
            target_id: Uuid::new_v4(),
            reason: ReportReason::Defamation,
            description: None,
        };
        assert!(ReportService::validate_submit(&req).is_ok());
    }

    #[test]
    fn validate_accepts_within_limit_description() {
        let req = SubmitReportRequest {
            target_type: TargetType::Rating,
            target_id: Uuid::new_v4(),
            reason: ReportReason::Privacy,
            description: Some("x".repeat(2000)),
        };
        assert!(ReportService::validate_submit(&req).is_ok());
    }

    #[test]
    fn validate_rejects_oversized_description() {
        let req = SubmitReportRequest {
            target_type: TargetType::Rating,
            target_id: Uuid::new_v4(),
            reason: ReportReason::Privacy,
            description: Some("x".repeat(2001)),
        };
        match ReportService::validate_submit(&req) {
            Err(ReportError::TextTooLong(2001)) => {}
            other => panic!("expected TextTooLong(2001), got {other:?}"),
        }
    }

    // ---- summarize / detail ----

    #[test]
    fn summarize_includes_sla_breached_flag() {
        let r = row(Uuid::new_v4(), "pending", 48 * 3600, 24 * 3600);
        let s = ReportService::summarize(&r, Utc::now());
        assert!(s.sla_breached);
        assert_eq!(s.report_id, r.id);
    }

    #[test]
    fn detail_includes_optional_resolution() {
        let mut r = row(Uuid::new_v4(), "resolved", 60, 24 * 3600);
        r.resolution = Some("removed".to_string());
        r.reviewer_id = Some(Uuid::new_v4());
        let d = ReportService::detail(&r, Utc::now());
        assert_eq!(d.resolution.as_deref(), Some("removed"));
        assert!(d.reviewer_id.is_some());
    }
}
