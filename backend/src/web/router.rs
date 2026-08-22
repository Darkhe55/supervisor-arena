//! Top-level router for the user UI.

use axum::{
    routing::{get, post},
    Router,
};

use super::{auth_pages, rating_pages, supervisor_pages};
use crate::AppState;

pub fn web_router() -> Router<AppState> {
    Router::new()
        // Landing + auth
        .route("/", get(auth_pages::index))
        .route("/register", get(auth_pages::register_form).post(auth_pages::register_submit))
        .route("/login", get(auth_pages::login_form).post(auth_pages::login_submit))
        .route("/logout", post(auth_pages::logout))
        .route("/me", get(auth_pages::me))
        // Supervisor browse / create
        .route(
            "/supervisors",
            get(supervisor_pages::search_form).post(supervisor_pages::search_submit),
        )
        .route(
            "/supervisors/new",
            get(supervisor_pages::new_form).post(supervisor_pages::new_submit),
        )
        .route("/supervisors/{alias}", get(supervisor_pages::detail))
        // Rating
        .route(
            "/supervisors/{alias}/rate",
            get(rating_pages::rate_form).post(rating_pages::rate_submit),
        )
}
