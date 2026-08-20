//! DTOs for the rating HTTP API

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// POST /supervisors/{alias}/ratings — submit one rating (one dimension).
#[derive(Debug, Deserialize)]
pub struct SubmitRatingRequest {
    /// One of: research | resource | fit | currency | ethic | tool
    pub dim: String,
    /// -100..=100 (default range 0-100; negative scores always permitted
    /// at API level for M6 — C-6 unlock is a future enhancement)
    pub value: i16,
    #[serde(default)]
    pub dim_additional: Option<String>,
    #[serde(default)]
    pub overall_additional: Option<String>,
    #[serde(default)]
    pub additional_level: Option<String>,
    #[serde(default)]
    pub evidence: Vec<String>,
}

/// Response after a successful rating submission.
#[derive(Debug, Serialize)]
pub struct RatingResponse {
    pub rating_id: Uuid,
    pub supervisor_id: Uuid,
    pub dim: String,
    pub value: i16,
    /// "created" if new, "updated" if this rating superseded a previous one
    /// (B-9 — see OUTLINE §7.10.4 D).
    pub outcome: RatingOutcome,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum RatingOutcome {
    Created,
    Updated,
}

/// Response from GET /supervisors/{alias}/ratings/me — list of this account's
/// existing ratings for the supervisor, across all 6 dimensions.
#[derive(Debug, Serialize)]
pub struct MyRatingsResponse {
    pub supervisor_id: Uuid,
    pub supervisor_alias: String,
    pub ratings: Vec<MyRatingEntry>,
}

#[derive(Debug, Serialize)]
pub struct MyRatingEntry {
    pub rating_id: Uuid,
    pub dim: String,
    pub value: i16,
    pub created_at: DateTime<Utc>,
    pub superseded_by: Option<Uuid>,
}
