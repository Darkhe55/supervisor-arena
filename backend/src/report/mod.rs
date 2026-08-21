//! Report (举报) module — Phase 10 (M3) of the project plan.
//!
//! Implements OUTLINE §7.10.7 + DECISIONS G-3:
//!   - `dto`     — HTTP DTOs (SubmitReportRequest, ReportSummary, etc.)
//!   - `error`   — `ReportError` (thiserror)
//!   - `repo`    — DB access (`reports` + cross-checks on `ratings`
//!                  and `supervisors` to validate the target)
//!   - `service` — pure helpers (sla_breached, summarize, validate_submit)
//!                  + I/O flow (submit_report, claim, resolve)
//!   - `handler` — axum routes (`/reports/...`)
//!
//! # Decision summary (H-48..H-50)
//!
//! - **Soft-remove / ban filter at the aggregation layer** (H-48):
//!   any rating whose author is `soft_removed = TRUE` or `is_banned
//!   = TRUE` is excluded from the public composite_score, radar,
//!   discipline-weight active-user count, and weight-vote eligibility
//!   count. The filter is applied via JOIN in the read queries, not
//!   by mutating the ratings table — that way the audit trail is
//!   preserved and the soft-remove can be reverted.
//! - **Self-report blocked** (H-49): rating / additional_info targets
//!   from the same account that submits the report return
//!   `SelfReport` (400). Supervisor targets are exempt (no single
//!   author).
//! - **SLA = 24h** (H-50): `sla_deadline = submitted_at + 24h`.
//!   `sla_breached` is `true` only when the report is still `pending`
//!   past the deadline. Claiming the report (status = `reviewing`)
//!   resets the SLA flag even if the deadline is in the past.
//! - **No rating-mutation side-effect from resolution** (M3 MVP):
//!   the report's `resolution` is recorded, but the actual rating
//!   hide / supervisor remove is a separate write that will be
//!   implemented in M5+ as a transactional "moderation action".
//!   M3's MVP keeps the audit trail clean by separating the two.

pub mod dto;
pub mod error;
pub mod handler;
pub mod repo;
pub mod service;

pub use dto::{
    ReportDetail, ReportReason, ReportResolution, ReportSummary, ResolveReportRequest,
    SubmitReportRequest, TargetType,
};
pub use error::ReportError;
pub use handler::report_router;
pub use repo::{ReportRepo, ReportRow};
pub use service::ReportService;
