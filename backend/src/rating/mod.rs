//! Rating module — Phase 6 (M6) of the project plan
//!
//! Implements OUTLINE §3-§6 (rating dimensions + slider + additional info)
//! and §7.10.4 D (B-9 supersede relationship for repeat-rating).
//!
//! Routes (mounted at /supervisors/{alias}/ratings/* in lib.rs):
//! - `POST /supervisors/{alias}/ratings`     — submit a single-dim rating
//! - `GET  /supervisors/{alias}/ratings/me`  — current account's existing ratings
//!
//! Scope (M6 first cut):
//! - 6 dimensions hardcoded (matches CHECK constraint)
//! - value in [-100, 100] (CHECK); negative scores always allowed here,
//!   C-6 unlock logic is a future enhancement
//! - Optional P2 fields: dim_additional_enc, overall_additional_enc,
//!   additional_level (L1-L4), evidence (URL array)
//! - B-9: re-submitting the same (account, supervisor, dim) marks the old
//!   row as superseded_by = new row id
//! - Snapshot of rater's discipline_hash at submission time
//!   (for aggregation weighting — see OUTLINE §5)
//! - Initial review_status = 'pending_review'; M6b adds sensitivity
//!   detection (P0/P1/P2 flags) + auto-approval flow
//!
//! Deferred to M6b/M6c:
//! - Rate limiting (M3 anti-abuse integration)
//! - Sensitivity filter (G-12) + auto-approve thresholds
//! - Aggregation triggers (Phase 7 hooks)
//! - Composite_score recompute on rating approve

pub mod dto;
pub mod error;
pub mod handler;
pub mod repo;
pub mod service;

pub use dto::{MyRatingsResponse, RatingResponse, SubmitRatingRequest};
pub use error::RatingError;
pub use handler::rating_router;
pub use service::RatingService;

/// The 6 rating dimensions, per OUTLINE §3.
pub const DIMS: &[&str] = &[
    "research",  // 科研能力
    "resource",  // 资源
    "fit",       // 匹配度
    "currency",  // 时效性
    "ethic",     // 师德
    "tool",      // 综合工具性
];
