//! Discipline-weight-voting module — Phase 9 (M2) of the project plan.
//!
//! Implements OUTLINE §4.4 + DECISIONS C-2:
//!   - `dto`     — HTTP DTOs (SubmitVoteRequest, VoteSummary, etc.)
//!   - `error`   — `DisciplineError` (thiserror)
//!   - `repo`    — DB access (4 tables: `discipline_weight_votes` + the 3
//!                  new ones from migration 13)
//!   - `service` — pure helpers (`renormalize`, `should_apply`,
//!                  `validate_proposed_weight`) + I/O flow
//!   - `handler` — axum routes (`/disciplines/:code/...`)
//!
//! # Decision summary (H-42 / H-43)
//!
//! - **Eligibility**: user must have ≥ 3 approved ratings in the
//!   discipline (mirrors OUTLINE §4.4 "投票门槛").
//! - **Cooldown**: same `(discipline, dim)` cannot be applied more
//!   than once per 30 days (OUTLINE §4.4 "冷却期").
//! - **Apply threshold** (H-42): `agree_count >= 3` AND
//!   `active_users(discipline) >= 5` AND
//!   `agree_count / (agree_count + disagree_count) >= 0.6`.
//! - **Self-deal blocked**: a user cannot ballot on their own proposal.
//! - **Renormalization** (H-43): when one dim's weight is changed, the
//!   other 5 dims are uniformly rebalanced so the 6 weights sum to 1.0.
//!   See `service::renormalize` for the formula and unit tests.
//! - **History**: every applied/rejected change appends a row to
//!   `discipline_weight_history` (audit + future M5b chart).

pub mod dto;
pub mod error;
pub mod handler;
pub mod repo;
pub mod service;

pub use dto::{
    BallotChoice, CastBallotRequest, CurrentWeightsResponse, SubmitVoteRequest, VoteDetail,
    VoteSummary, WeightEntry, WeightHistoryEntry,
};
pub use error::DisciplineError;
pub use handler::discipline_router;
pub use repo::{DisciplineRepo, VoteRow, WeightRow};
pub use service::{BallotOutcome, CurrentWeightsView, DisciplineService};
