//! Top-level router for the user UI.
//!
//! Split into two pieces so lib.rs can nest the supervisor-scoped routes
//! inside the existing `/supervisors` nest (alongside the JSON API), which
//! keeps axum's path-pattern matching happy — bare `/supervisors/{alias}`
//! conflicts with `/supervisors/{alias}/ratings` (JSON) if both routers
//! are merged separately, so we keep all `/supervisors/*` routes in the
//! same merged tree.

use axum::{
    routing::{get, post},
    Router,
};

use super::{auth_pages, rating_pages, supervisor_pages};
use crate::AppState;

/// Routes that live at the URL root: `/`, `/register`, `/login`, `/logout`,
/// `/me`.
pub fn root_router() -> Router<AppState> {
    Router::new()
        .route("/", get(auth_pages::index))
        .route(
            "/register",
            get(auth_pages::register_form).post(auth_pages::register_submit),
        )
        .route(
            "/login",
            get(auth_pages::login_form).post(auth_pages::login_submit),
        )
        .route("/logout", post(auth_pages::logout))
        .route("/me", get(auth_pages::me))
}

/// Routes that live under `/supervisors/*` — must be merged INTO the
/// `/supervisors` nest (not `.merge()`d at the top level) so axum
/// matches the bare `{name}` and `{name}/rate` patterns. Uses
/// relative paths (no `/supervisors` prefix) because they get nested.
///
/// Param name is `name` (not `alias`) to avoid matchit conflict with the
/// JSON router's `/:alias/ratings` and `/by-alias/:alias` patterns.
// H-54 web UI supervisor routes are now registered directly in the main
// Router (lib.rs) — see the comment there for why we don't merge them
// into the /supervisors nest. Kept as empty stub for API stability.
pub fn supervisors_router() -> Router<AppState> {
    Router::new()
}

#[allow(dead_code)]
fn _suppress() {
    // (kept for diff)
    let _ = supervisor_pages::detail;
    let _ = rating_pages::rate_form;
}
