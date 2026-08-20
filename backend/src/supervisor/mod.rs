//! Supervisor module — Phase 5 (M5) of the project plan
//!
//! Implements the public-side of OUTLINE §7.10 (导师匿名系统 — 无关化名 + k-匿名).
//!
//! **This commit covers the deterministic alias generator core**:
//! - Word lists (literary / nature / geometric / 6 学科门类)
//! - Person-name whitelist (starter set, growth plan documented)
//! - `AliasGenerator` — HMAC-seeded deterministic algorithm
//! - Style + discipline-fused templates
//! - Whitelist collision detection + retry
//! - 1-to-1 enforcement (delegated to DB UNIQUE constraint)
//!
//! **Deferred to M5b**: the supervisor creation flow itself
//! (POST /supervisors/request, dedup-via-hash, mapping table write,
//! review queue). That's the orchestration + DB write layer on top
//! of this core.

pub mod alias;
pub mod error;
pub mod whitelist;
pub mod words;

pub use alias::{AliasGenerator, AliasInput, AliasStyle};
pub use error::AliasError;
