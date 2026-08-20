//! Supervisor module — Phase 5 (M5) of the project plan
//!
//! Implements the full OUTLINE §7.10 flow:
//!
//! **Phase 5b (this commit)** — service / repo / handler / k-anonymity:
//! - `alias`     — deterministic alias generator (HMAC seed + SplitMix32)
//! - `words`     — word lists + discipline category mapping
//! - `whitelist` — person-name collision check
//! - `dto`       — HTTP DTOs
//! - `error`     — `SupervisorError` mapping
//! - `repo`      — DB access (4 tables + lookup tables)
//! - `service`   — create_request flow (dedup → encrypt → generate alias →
//!                 review queue) + approve/reject + k-anonymous public view
//! - `handler`   — axum routes (`/supervisors/*`)
//!
//! **Deferred to M5c** (still in Phase 5): review SLA scheduler (cron),
//! reject-reason capture in audit log, name-mismatch probe
//! (does a new entry's submitted_name look like a real person?).

pub mod alias;
pub mod dto;
pub mod error;
pub mod handler;
pub mod repo;
pub mod service;
pub mod whitelist;
pub mod words;

pub use alias::{AliasGenerator, AliasInput, AliasStyle};
pub use dto::{
    CreateSupervisorRequest, CreateSupervisorResponse, PendingReviewEntry, ReviewAction,
    ReviewActionKind, SupervisorPublicView, SupervisorRequestStatus,
};
pub use error::SupervisorError;
pub use handler::supervisor_router;
pub use service::SupervisorService;
