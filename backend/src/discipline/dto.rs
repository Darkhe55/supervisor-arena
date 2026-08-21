//! DTOs for the discipline-weight-voting HTTP API.
//!
//! See OUTLINE §4.4 + DECISIONS C-2 for the user-facing flow:
//!   - Eligible user (≥3 approved ratings in this discipline) proposes a
//!     new weight for one dimension.
//!   - Other eligible users agree / disagree on the proposal.
//!   - When the threshold is met, the weight is applied to the live table
//!     and all 6 dimensions are renormalized to sum to 1.0.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

/// `POST /disciplines/{code}/weight-votes` — submit a new weight proposal.
///
/// `proposed_weight` is the *new* weight for `dim` (0..=1, single-dimension).
/// Renormalization of the other 5 dims happens at apply time (the others
/// are uniformly rebalanced to make the 6 dims sum to 1.0).
#[derive(Debug, Deserialize)]
pub struct SubmitVoteRequest {
    /// One of: research, resource, fit, currency, ethic, tool.
    pub dim: String,
    /// New weight for `dim`, 0..=1 (validated server-side).
    pub proposed_weight: f64,
    /// Optional human-readable reason (stored as plaintext — never
    /// displayed publicly, but visible to admins on audit).
    #[serde(default)]
    pub reason: Option<String>,
}

/// `GET /disciplines/{code}/weight-votes` — one proposal summary.
#[derive(Debug, Serialize)]
pub struct VoteSummary {
    pub vote_id: Uuid,
    pub discipline: String,
    pub dim: String,
    pub proposed_weight: f64,
    pub reason: Option<String>,
    pub proposer_id: Uuid,
    pub agree_count: i32,
    pub disagree_count: i32,
    /// `pending` | `applied` | `rejected`
    pub status: String,
    pub applied_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// `true` if `apply_threshold_reached` is true — only for `pending` rows.
    pub ready_to_apply: bool,
}

/// `POST /disciplines/{code}/weight-votes/{vote_id}/ballot`
/// — cast an agree or disagree vote.
#[derive(Debug, Deserialize)]
pub struct CastBallotRequest {
    /// `agree` or `disagree`.
    pub choice: BallotChoice,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum BallotChoice {
    Agree,
    Disagree,
}

/// `GET /disciplines/{code}/weight-votes/{vote_id}` — single vote detail
/// (counts + status).
#[derive(Debug, Serialize)]
pub struct VoteDetail {
    pub vote_id: Uuid,
    pub discipline: String,
    pub dim: String,
    pub proposed_weight: f64,
    pub agree_count: i32,
    pub disagree_count: i32,
    pub status: String,
    pub applied_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    /// `true` if the threshold is met (server-side flag, for clients to
    /// know when to expect the application to happen). `false` if the
    /// threshold has not been reached.
    pub threshold_met: bool,
}

/// `GET /disciplines/{code}/weights` — current applied weight per dim.
#[derive(Debug, Serialize)]
pub struct CurrentWeightsResponse {
    pub discipline: String,
    /// One entry per of the 6 dims, sorted in `RADAR_DIMS` order.
    pub weights: Vec<WeightEntry>,
    /// Sum of all 6 (always 1.0 in steady state, but we surface it for
    /// sanity checks).
    pub sum: f64,
    /// When the *latest* weight in this discipline was applied (newest
    /// applied_at). `None` if no manual application has ever happened
    /// (all 6 weights come from the bootstrap equal-weights row).
    pub last_applied_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Serialize)]
pub struct WeightEntry {
    pub dim: String,
    pub weight: f64,
    pub applied_at: DateTime<Utc>,
    /// The vote that produced this weight (NULL for the bootstrap row).
    pub source_vote_id: Option<Uuid>,
}

/// `GET /disciplines/{code}/weights/history?dim=...` — history log
#[derive(Debug, Serialize)]
pub struct WeightHistoryEntry {
    pub id: Uuid,
    pub discipline: String,
    pub dim: String,
    pub old_weight: Option<f64>,
    pub new_weight: f64,
    pub action: String,
    pub source_vote_id: Option<Uuid>,
    pub actor_id: Option<Uuid>,
    pub created_at: DateTime<Utc>,
}
