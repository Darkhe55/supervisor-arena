//! Invitation module — Phase 16 (M5 邀请试用).
//!
//! Per OUTLINE §11 / M5, the MVP needs a way to seed initial
//! users via invitation codes. The flow:
//!
//! 1. An authed user generates a code (POST /invitations).
//!    The backend returns a one-time-display string like
//!    `7K9X-3RT1-A82B`.
//! 2. The new user passes that code to /auth/register as an
//!    optional `invite_code` field.
//! 3. The backend atomically redeems the code (transactional
//!    UPDATE) and tags the new account as "invited" by setting
//!    `accounts.invited_by_account_id` to the inviter's ID.
//! 4. The frontend can show a "thanks for joining early" UX
//!    for these users.
//!
//! # Scope
//!
//! Per OUTLINE §7.6, registration is *open* — invitation is an
//! OPTIONAL path, not a gate. Codes have a `max_uses` and an
//! optional `expires_at` so they're not infinitely redeemable.
//!
//! # Out of scope (deferred)
//!
//! - Tier upgrade / trial period (would need a `tier` field on
//!   accounts and a cron to roll back)
//! - Rate limit on code generation (we trust the user — for
//!   the M5 MVP the codes are short-lived by design)
//! - Admin endpoint to list / revoke codes (M5+ can add)

pub mod error;
pub mod handler;
pub mod repo;
pub mod service;

pub use error::InvitationError;
pub use handler::invitation_router;
pub use repo::{InvitationRepo, InvitationRow};
pub use service::{InvitationService, RedemptionOutcome};
