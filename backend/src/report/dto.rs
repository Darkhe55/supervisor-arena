//! DTOs for the report (举报) module
//!
//! Implements OUTLINE §7.10.7 + DECISIONS G-3: any user can report a
//! rating / supervisor / additional_info; backend moderators work
//! the queue. All moderation actions are audit-logged (target =
//! encryption_audit_log + reports.resolution).

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `POST /reports` — submit a new report.
#[derive(Debug, Deserialize)]
pub struct SubmitReportRequest {
    /// One of: rating, supervisor, additional_info.
    pub target_type: TargetType,
    pub target_id: Uuid,
    /// One of: defamation, insult, privacy, research_leak, other.
    pub reason: ReportReason,
    /// Optional free-form detail (≤ 2000 chars).
    #[serde(default)]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum TargetType {
    Rating,
    Supervisor,
    AdditionalInfo,
}

impl TargetType {
    /// Validate the string form against the DB CHECK constraint.
    pub fn as_db_str(self) -> &'static str {
        match self {
            TargetType::Rating => "rating",
            TargetType::Supervisor => "supervisor",
            TargetType::AdditionalInfo => "additional_info",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "rating" => Some(TargetType::Rating),
            "supervisor" => Some(TargetType::Supervisor),
            "additional_info" => Some(TargetType::AdditionalInfo),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportReason {
    Defamation,
    Insult,
    Privacy,
    ResearchLeak,
    Other,
}

impl ReportReason {
    pub fn as_db_str(self) -> &'static str {
        match self {
            ReportReason::Defamation => "defamation",
            ReportReason::Insult => "insult",
            ReportReason::Privacy => "privacy",
            ReportReason::ResearchLeak => "research_leak",
            ReportReason::Other => "other",
        }
    }

    pub fn from_db_str(s: &str) -> Option<Self> {
        match s {
            "defamation" => Some(ReportReason::Defamation),
            "insult" => Some(ReportReason::Insult),
            "privacy" => Some(ReportReason::Privacy),
            "research_leak" => Some(ReportReason::ResearchLeak),
            "other" => Some(ReportReason::Other),
            _ => None,
        }
    }
}

/// `POST /reports/:id/resolve` — moderator's resolution.
#[derive(Debug, Deserialize)]
pub struct ResolveReportRequest {
    pub resolution: ReportResolution,
    /// Optional reviewer note (≤ 2000 chars). Audit-logged.
    #[serde(default)]
    pub note: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReportResolution {
    /// Content was removed (rating hidden, supervisor removed, etc).
    Removed,
    /// User warned but content stays.
    Warned,
    /// Report rejected (no violation found).
    Rejected,
    /// Closed without action (insufficient evidence, duplicate, etc).
    NoAction,
}

impl ReportResolution {
    pub fn as_db_str(self) -> &'static str {
        match self {
            ReportResolution::Removed => "removed",
            ReportResolution::Warned => "warned",
            ReportResolution::Rejected => "rejected",
            ReportResolution::NoAction => "no_action",
        }
    }
}

/// Common summary view — used in queue listings.
#[derive(Debug, Serialize)]
pub struct ReportSummary {
    pub report_id: Uuid,
    pub target_type: String,
    pub target_id: Uuid,
    pub reason: String,
    pub status: String,
    pub submitted_at: DateTime<Utc>,
    pub sla_deadline: DateTime<Utc>,
    pub reporter_id: Uuid,
    /// `true` if the SLA has been breached (no reviewer claimed yet).
    pub sla_breached: bool,
}

/// Single-report detail (reviewer view).
#[derive(Debug, Serialize)]
pub struct ReportDetail {
    pub report_id: Uuid,
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
    pub sla_breached: bool,
}
