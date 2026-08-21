//! Aggregation module — Phase 7 (M7) of the project plan
//!
//! Computes `composite_score` and `radar_dimensions` for a supervisor from
//! their **approved** ratings (filter on `ratings.review_status = 'approved'`).
//!
//! # Public clamping rule (OUTLINE §8 注释 `composite_score`)
//!
//! The mean of approved ratings can be negative when a supervisor's net
//! reputation is bad. For the public display, we clamp to `>= 0`:
//! - Raw mean: arithmetic mean of approved rating values in [-100, 100]
//! - Public `composite_score`: `max(0, raw_mean)` — we do not surface
//!   negative numbers publicly, because k-anonymity and the F-3
//!   "non-disclosure" wall already protect identity, and a public
//!   negative mean invites dog-piling (OUTLINE §7.1 anti-abuse rationale).
//!
//! # Lazy vs eager (H-33)
//!
//! M7 computes **lazily on public read**. The `supervisors.composite_score`
//! and `supervisors.radar_dimensions` columns are kept as a denormalized
//! cache, populated by a future background job (M7b). For M7 we just
//! read the approved ratings and compute on the fly. Latency is fine
//! because each supervisor has at most a few hundred ratings.
//!
//! # Algorithm
//!
//! ```text
//! approved_ratings = SELECT dim, value FROM ratings
//!                     WHERE supervisor_id = $1 AND review_status = 'approved'
//! per_dim_avg      = group by dim, mean(value)
//! radar            = {dim: per_dim_avg[dim] or null for all 6 dims}
//! raw_mean         = mean(per_dim_avg.values())  // only dims that have data
//! composite        = max(0, raw_mean) if any data else null
//! ```
//!
//! `radar_dimensions` includes all 6 dims (null for those with no approved
//! rating) so the frontend can render an empty hexagon leg rather than
//! guessing.

pub mod error;
pub mod repo;
pub mod service;

pub use repo::{ApprovedRating, RatingRepo};
pub use service::{
    compute_from_approved, AggregationService, RadarDimensions, SupervisorScore,
};

/// All 6 rating dimensions, in OUTLINE §3 display order.
pub const RADAR_DIMS: &[&str] = &[
    "research",
    "resource",
    "fit",
    "currency",
    "ethic",
    "tool",
];
