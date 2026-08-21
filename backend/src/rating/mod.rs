//! Rating module — Phase 6 (M6) of the project plan
//!
//! Implements OUTLINE §3-§6 (rating dimensions + slider + additional info),
//! §7.10.4 D (B-9 supersede relationship for repeat-rating),
//! and M6b sensitivity detection + auto-approval flow (G-12).
//!
//! Routes (mounted under /supervisors in lib.rs):
//! - `POST /supervisors/{alias}/ratings`     — submit a single-dim rating
//! - `GET  /supervisors/{alias}/ratings/me`  — current account's existing ratings
//! - `GET  /supervisors/ratings/review/queue` — reviewer: list pending ratings
//! - `POST /supervisors/ratings/review/{id}`  — reviewer: approve | reject
//!
//! Scope (M6 + M6b):
//! - 6 dimensions hardcoded (matches CHECK constraint)
//! - value in [-100, 100] (CHECK); negative scores always allowed here
//! - Optional P2 fields: dim_additional_enc, overall_additional_enc,
//!   additional_level (L1-L4), evidence (URL array)
//! - B-9: re-submitting the same (account, supervisor, dim) marks the old
//!   row as superseded_by = new row id
//! - Snapshot of rater's discipline_hash at submission time
//! - **M6b**: sensitivity detection on additional text (4 levels: Clean /
//!   P2Warn / P1Redact / P0Strict), auto-approval when REVIEW__MODE =
//!   auto_pass AND no P0 detected. Otherwise stays pending for human review.
//!
//! Deferred to M6c:
//! - Rate limiting (M3 anti-abuse integration)
//! - Composite_score recompute on rating approve (background job)
//! - P1 redaction write-back (M6b flags it; M6c writes the redacted_* cols)

pub mod dto;
pub mod error;
pub mod handler;
pub mod redaction;
pub mod repo;
pub mod service;
pub mod sensitivity;

pub use dto::{MyRatingsResponse, RatingResponse, SubmitRatingRequest};
pub use error::RatingError;
pub use handler::rating_router;
pub use redaction::redact_p1;
pub use sensitivity::{SensitivityFlag, SensitivityError};
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
