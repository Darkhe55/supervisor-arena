//! DTOs for the supervisor HTTP API

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::aggregation::RadarDimensions;

/// POST /supervisors/request — a user submits a new supervisor entry.
///
/// `submitted_name` is the *raw* user-provided string (may be a real name,
/// a nickname, or pure gibberish). The server encrypts it (P0) and hashes
/// (P1) it; only the alias ever surfaces publicly.
#[derive(Debug, Deserialize)]
pub struct CreateSupervisorRequest {
    pub submitted_name: String,
    pub discipline: String,
    pub college: String,
}

/// Response from POST /supervisors/request and dedup hits.
#[derive(Debug, Serialize)]
pub struct CreateSupervisorResponse {
    pub request_id: Uuid,
    pub alias: String,
    /// `pending_review` for new entries, `deduplicated` if the same
    /// (submitted_name, discipline, college) tuple already has a mapping.
    pub status: SupervisorRequestStatus,
    pub discipline: String,
    pub college: String,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum SupervisorRequestStatus {
    PendingReview,
    Approved,
    Deduplicated,
}

/// Body of POST /supervisors/review/{request_id}
#[derive(Debug, Deserialize)]
pub struct ReviewAction {
    pub action: ReviewActionKind,
    #[serde(default)]
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReviewActionKind {
    Approve,
    Reject,
}

/// Public-facing supervisor view (returned by GET /supervisors/by-alias/{alias}).
///
/// Score + radar are computed lazily on every read from the approved
/// ratings (see `aggregation::AggregationService`).
#[derive(Debug, Serialize)]
pub struct SupervisorPublicView {
    pub alias: String,
    pub discipline: String,
    pub college: String,
    /// Whether the entry is currently public-visible (k-anon ≥ threshold
    /// AND status=approved). `false` = entry exists but hidden.
    pub visible: bool,
    pub k_anonymity_count: i32,
    /// Mean of approved ratings, clamped to `>= 0`. `None` if no approved
    /// ratings yet.
    pub composite_score: Option<f64>,
    /// 6-dim radar (research/resource/fit/currency/ethic/tool). Each dim
    /// is `None` if no approved ratings exist for that dim.
    pub radar: RadarDimensions,
    /// Total number of approved (current, non-superseded) ratings.
    pub rating_count: i32,
    pub created_at: DateTime<Utc>,
}

/// Minimal pending-review summary (for /supervisors/review/queue).
#[derive(Debug, Serialize)]
pub struct PendingReviewEntry {
    pub request_id: Uuid,
    pub submitter_id: Uuid,
    pub submitted_name: String, // 明文, only visible to reviewers (G-15)
    pub discipline: String,
    pub college: String,
    pub created_at: DateTime<Utc>,
    pub sla_deadline: DateTime<Utc>,
}

/// Query parameters for GET /supervisors/search.
#[derive(Debug, Deserialize)]
pub struct SearchParams {
    pub discipline: String,
    pub college: String,
    #[serde(default = "default_limit")]
    pub limit: i64,
    #[serde(default)]
    pub offset: i64,
}

fn default_limit() -> i64 {
    20
}

/// Response from GET /supervisors/search — list of k-anon-gated supervisors
/// matching the (discipline, college) bucket.
#[derive(Debug, Serialize)]
pub struct SearchResponse {
    pub discipline: String,
    pub college: String,
    pub total: i64,
    pub limit: i64,
    pub offset: i64,
    pub results: Vec<SearchEntry>,
}

#[derive(Debug, Serialize)]
pub struct SearchEntry {
    pub alias: String,
    pub discipline: String,
    pub college: String,
    pub composite_score: Option<f64>,
    pub radar: RadarDimensions,
    pub rating_count: i32,
    pub created_at: DateTime<Utc>,
}
