//! axum handlers for the rating module
//!
//! Routes (mounted at /supervisors/{alias}/ratings/* in lib.rs):
//! - `POST /supervisors/{alias}/ratings`     — submit a single-dim rating
//! - `GET  /supervisors/{alias}/ratings/me`  — current account's existing ratings

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;

use super::dto::{MyRatingsResponse, RatingResponse, SubmitRatingRequest};
use super::error::RatingError;
use super::repo::RatingRepo;
use super::service::RatingService;
use crate::account::AuthAccount;
use crate::AppState;

pub fn rating_router() -> Router<AppState> {
    Router::new()
        .route("/:alias/ratings", post(submit_rating))
        .route("/:alias/ratings/me", get(my_ratings))
}

async fn submit_rating(
    State(state): State<AppState>,
    Path(alias): Path<String>,
    auth: AuthAccount,
    Json(req): Json<SubmitRatingRequest>,
) -> Result<(StatusCode, Json<RatingResponse>), ApiError> {
    let svc = service(&state)?;
    let resp = svc.submit(auth.0, &alias, req).await?;
    let status = if resp.outcome == super::dto::RatingOutcome::Updated {
        StatusCode::OK // 200 for re-submit (supersede)
    } else {
        StatusCode::CREATED // 201 for new
    };
    Ok((status, Json(resp)))
}

async fn my_ratings(
    State(state): State<AppState>,
    Path(alias): Path<String>,
    auth: AuthAccount,
) -> Result<Json<MyRatingsResponse>, ApiError> {
    let svc = service(&state)?;
    let resp = svc.my_ratings(auth.0, &alias).await?;
    Ok(Json(resp))
}

fn service(state: &AppState) -> Result<RatingService, ApiError> {
    use crate::supervisor::repo::SupervisorRepo;
    let rating_repo = RatingRepo::new(state.db.clone());
    let supervisor_repo = SupervisorRepo::new(state.db.clone());
    let account_repo = crate::account::repo::AccountRepo::new(state.db.clone());
    Ok(RatingService::new(
        rating_repo,
        supervisor_repo,
        account_repo,
        state.keys.clone(),
    ))
}

pub struct ApiError(RatingError);

impl From<RatingError> for ApiError {
    fn from(e: RatingError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            RatingError::InvalidDim(_)
            | RatingError::InvalidValue(_)
            | RatingError::InvalidAdditionalLevel(_)
            | RatingError::InvalidEvidence(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
            RatingError::SupervisorNotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            RatingError::SupervisorNotApproved(_) => (StatusCode::FORBIDDEN, "not_approved"),
            RatingError::Database(_) | RatingError::Crypto(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self.0, "internal error in rating handler");
        }
        let body = if status == StatusCode::INTERNAL_SERVER_ERROR {
            json!({ "error": code, "message": self.0.to_string() })
        } else {
            json!({ "error": code, "message": self.0.to_string() })
        };
        (status, Json(body)).into_response()
    }
}
