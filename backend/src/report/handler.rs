//! axum handlers for the report (举报) module.
//!
//! Routes (mounted at /reports in lib.rs):
//! - `POST /reports`                  — authed user submits a report
//! - `GET  /reports/queue`            — reviewer lists pending + reviewing
//! - `GET  /reports/:id`              — reviewer reads one report
//! - `POST /reports/:id/claim`        — reviewer claims for review
//! - `POST /reports/:id/resolve`      — reviewer resolves
//!
//! # Authorization
//!
//! - POST /reports: any authed user (AuthAccount extractor)
//! - All other routes: AuthAccount extractor is still used (we just
//!   don't enforce a "reviewer role" in the M3 MVP — the review queue
//!   is gated by JWT presence and we trust the JWT sub for now).
//!   M5b will add a proper reviewer role on the JWT tier claim.
//!
//! # Error → status code mapping
//!
//! - `SelfReport`               → 400 invalid_input (you can't report yourself)
//! - `TargetNotFound`           → 404 not_found
//! - `TargetNotReportable`      → 409 conflict
//! - `ReportNotPending`         → 409 conflict
//! - `ReportNotFound`           → 404 not_found
//! - `TextTooLong`              → 400 invalid_input
//! - `InvalidInput`             → 400 invalid_input
//! - `Database`                 → 500 (logged, no detail leaked)

use axum::{
    extract::{Path, Query, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::dto::{ReportDetail, ReportSummary, ResolveReportRequest, SubmitReportRequest};
use super::error::ReportError;
use super::service::ReportService;
use crate::account::AuthAccount;
use crate::AppState;

pub fn report_router() -> Router<AppState> {
    Router::new()
        .route("/", post(submit_report))
        .route("/queue", get(list_queue))
        .route("/:id", get(get_report))
        .route("/:id/claim", post(claim_report))
        .route("/:id/resolve", post(resolve_report))
}

async fn submit_report(
    State(state): State<AppState>,
    auth: AuthAccount,
    Json(req): Json<SubmitReportRequest>,
) -> Result<(StatusCode, Json<Uuid>), ApiError> {
    let svc = service(&state)?;
    let id = svc.submit_report(auth.0, req).await?;
    Ok((StatusCode::CREATED, Json(id)))
}

#[derive(Debug, Deserialize)]
pub struct QueueQuery {
    #[serde(default = "default_queue_limit")]
    pub limit: i64,
}

fn default_queue_limit() -> i64 {
    50
}

async fn list_queue(
    State(state): State<AppState>,
    _auth: AuthAccount,
    Query(q): Query<QueueQuery>,
) -> Result<Json<Vec<ReportSummary>>, ApiError> {
    let svc = service(&state)?;
    let list = svc.list_pending(q.limit).await?;
    Ok(Json(list))
}

async fn get_report(
    State(state): State<AppState>,
    _auth: AuthAccount,
    Path(id): Path<Uuid>,
) -> Result<Json<ReportDetail>, ApiError> {
    let svc = service(&state)?;
    let detail = svc
        .get(id)
        .await?
        .ok_or(ApiError(ReportError::ReportNotFound(id)))?;
    Ok(Json(detail))
}

async fn claim_report(
    State(state): State<AppState>,
    auth: AuthAccount,
    Path(id): Path<Uuid>,
) -> Result<Json<ReportDetail>, ApiError> {
    let svc = service(&state)?;
    let detail = svc.claim(id, auth.0).await?;
    Ok(Json(detail))
}

async fn resolve_report(
    State(state): State<AppState>,
    auth: AuthAccount,
    Path(id): Path<Uuid>,
    Json(req): Json<ResolveReportRequest>,
) -> Result<Json<ReportDetail>, ApiError> {
    let svc = service(&state)?;
    let detail = svc
        .resolve(id, auth.0, req.resolution, req.note.as_deref())
        .await?;
    Ok(Json(detail))
}

// --- Helpers ---

fn service(state: &AppState) -> Result<ReportService, ApiError> {
    use super::repo::ReportRepo;
    Ok(ReportService::new(ReportRepo::new(state.db.clone())))
}

// --- Error wrapper ---

pub struct ApiError(ReportError);

impl From<ReportError> for ApiError {
    fn from(e: ReportError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            ReportError::SelfReport
            | ReportError::TextTooLong(_)
            | ReportError::InvalidInput(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
            ReportError::TargetNotFound { .. } => (StatusCode::NOT_FOUND, "target_not_found"),
            ReportError::TargetNotReportable { .. } => (StatusCode::CONFLICT, "not_reportable"),
            ReportError::ReportNotPending(_, _) => (StatusCode::CONFLICT, "not_pending"),
            ReportError::ReportNotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ReportError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self.0, "internal error in report handler");
        }
        let body = if status == StatusCode::INTERNAL_SERVER_ERROR {
            json!({ "error": code })
        } else {
            json!({ "error": code, "message": self.0.to_string() })
        };
        (status, Json(body)).into_response()
    }
}
