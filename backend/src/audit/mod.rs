//! Encryption audit log writer — Phase 14 (M6 / OUTLINE §7.9.5).
//!
//! Records every access to a P0/P1/P2 encrypted field for forensic
//! review. The DB schema (`encryption_audit_log`) has been in place
//! since migration 11; this module is the writer side.
//!
//! # Usage
//!
//! Call from the handler/service that touches an encrypted column:
//!
//! ```ignore
//! state.audit.log(EncryptionAccess {
//!     field: "accounts.email_enc",
//!     account_id: Some(account_id),
//!     accessor: "account::service::login",
//!     purpose: AuditPurpose::Login,
//!     ip_hash: Some(ip_hmac),
//!     success: true,
//! });
//! ```
//!
//! Writes are **best-effort**: a failed audit write logs a warning
//! but does NOT fail the request. Reasoning: an audit log that can
//! itself block user actions creates a denial-of-service vector, and
//! the underlying action is the security-relevant one (it already
//! happened).
//!
//! # Out of scope (deferred to M5+ / M6+)
//!
//! - Read-side audit (`SELECT * FROM accounts WHERE ...` for review)
//! - `audit_log` query API (no GET /audit/* endpoint yet — admins
//!   query the table directly)
//! - Log shipping to a SIEM
//! - Retention policy (table grows unboundedly for now)

pub mod writer;

pub use writer::{AuditLog, AuditPurpose, EncryptionAccess};
