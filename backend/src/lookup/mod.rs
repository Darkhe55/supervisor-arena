//! Lookup module — Phase 13 (M5 i18n).
//!
//! Public read-only endpoints that surface the bilingual lookup
//! tables (`disciplines`, `colleges`, `rating_dimensions`) so
//! the frontend can populate dropdowns without hardcoding the
//! canonical lists.
//!
//! # Language negotiation
//!
//! The handler honors the `Accept-Language` header per RFC 7231.
//! Two supported tags: `zh` (default fallback), `en`. Anything else
//! falls back to `zh`.
//!
//! Each row in the response includes BOTH `name_zh` and `name_en`
//! (so the frontend can pick the right one for a runtime toggle
//! without re-fetching). The `name` field at the top level is the
//! negotiated version (zh or en) for clients that don't care to
//! distinguish.
//!
//! # Why a public endpoint (no auth)?
//!
//! The lookup tables are not PII — they're the same set of names
//! every Chinese university uses, and the frontend needs them
//! on the registration form (before the user has a JWT). The
//! `is_active` filter only shows currently-usable entries, so a
//! future "decommissioned discipline" can be soft-hidden without
//! dropping the row.

pub mod error;
pub mod handler;
pub mod service;

pub use handler::lookup_router;
pub use service::{AcceptLanguage, LocalizedDiscipline, LocalizedCollege, LocalizedDimension};
