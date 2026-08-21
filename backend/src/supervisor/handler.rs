//! axum handlers for the supervisor module
//!
//! Routes (mounted at /supervisors/* in lib.rs):
//! - `POST /supervisors/request`         — authed user submits a new entry
//! - `GET  /supervisors/by-alias/{a}`    — public view (k-anon aware)
//! - `GET  /supervisors/review/queue`    — reviewer: list pending
//! - `POST /supervisors/review/{id}`     — reviewer: approve | reject
//!
//! Error → status code mapping (matches account module pattern):
//!   InvalidInput      → 400
//!   UnknownDiscipline/College → 400
//!   NotFound          → 404
//!   AliasGeneration / Database / Crypto → 500 (logged)

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use super::dto::{
    CreateSupervisorRequest, CreateSupervisorResponse, PendingReviewEntry, ReviewAction,
    SupervisorPublicView,
};
use super::error::SupervisorError;
use super::service::SupervisorService;
use crate::account::AuthAccount;
use crate::AppState;

pub fn supervisor_router() -> Router<AppState> {
    Router::new()
        .route("/request", post(create_request))
        .route("/by-alias/:alias", get(public_view))
        .route("/review/queue", get(pending_queue))
        .route("/review/:id", post(review))
}

// --- Handlers ---

async fn create_request(
    State(state): State<AppState>,
    auth: AuthAccount,
    Json(req): Json<CreateSupervisorRequest>,
) -> Result<(StatusCode, Json<CreateSupervisorResponse>), ApiError> {
    let svc = service(&state)?;
    let resp = svc.create_request(auth.0, req).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

async fn public_view(
    State(state): State<AppState>,
    Path(alias): Path<String>,
) -> Result<Json<SupervisorPublicView>, ApiError> {
    let svc = service(&state)?;
    match svc.public_view_by_alias(&alias).await? {
        Some(v) => Ok(Json(v)),
        None => Err(ApiError(SupervisorError::NotFound)),
    }
}

async fn pending_queue(
    State(state): State<AppState>,
    _auth: AuthAccount,
) -> Result<Json<Vec<PendingReviewEntry>>, ApiError> {
    let svc = service(&state)?;
    let list = svc.pending_reviews(100).await?;
    Ok(Json(list))
}

async fn review(
    State(state): State<AppState>,
    _auth: AuthAccount,
    Path(id): Path<uuid::Uuid>,
    Json(action): Json<ReviewAction>,
) -> Result<StatusCode, ApiError> {
    let svc = service(&state)?;
    match action.action {
        super::dto::ReviewActionKind::Approve => {
            svc.approve(id, _auth.0, action.notes.as_deref()).await?;
        }
        super::dto::ReviewActionKind::Reject => {
            svc.reject(id, _auth.0, action.notes.as_deref()).await?;
        }
    }
    Ok(StatusCode::NO_CONTENT)
}

// --- Helpers ---

fn service(state: &AppState) -> Result<SupervisorService, ApiError> {
    use crate::aggregation::{AggregationService, RatingRepo as AggRepo};
    use crate::config::ReviewConfig;
    let repo = SupervisorRepo::new(state.db.clone());
    let keys = state.keys.clone();
    let alias_gen = super::alias::AliasGenerator::from_keystore(&keys);
    let review_cfg: ReviewConfig = state.config.review.clone();
    let aggregation = AggregationService::new(AggRepo::new(state.db.clone()));
    Ok(SupervisorService::new(repo, keys, alias_gen, review_cfg, aggregation))
}

// --- Error wrapper ---

pub struct ApiError(SupervisorError);

impl From<SupervisorError> for ApiError {
    fn from(e: SupervisorError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            SupervisorError::InvalidInput(_)
            | SupervisorError::UnknownDiscipline(_)
            | SupervisorError::UnknownCollege(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
            SupervisorError::NotFound => (StatusCode::NOT_FOUND, "not_found"),
            SupervisorError::Duplicate(_) => (StatusCode::CONFLICT, "duplicate"),
            SupervisorError::KAnonymityHidden(..) => (StatusCode::NOT_FOUND, "hidden"),
            SupervisorError::AliasGeneration(_)
            | SupervisorError::Database(_)
            | SupervisorError::Crypto(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            // Log full detail for debugging.
            tracing::error!(error = ?self.0, "internal error in supervisor handler");
        }
        let body = if status == StatusCode::INTERNAL_SERVER_ERROR {
            // M5b dev mode: include the message in the body so curl tests
            // can see what went wrong. Production should hide this.
            json!({ "error": code, "message": self.0.to_string() })
        } else {
            json!({ "error": code, "message": self.0.to_string() })
        };
        (status, Json(body)).into_response()
    }
}

// Re-export the repo for the service factory above.
use super::repo::SupervisorRepo;
