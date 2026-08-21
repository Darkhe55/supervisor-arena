//! axum handlers for the lookup module.
//!
//! Routes (mounted at /lookup in lib.rs, no auth):
//! - `GET /lookup/disciplines`     — list all active disciplines
//! - `GET /lookup/colleges`        — list all active colleges
//! - `GET /lookup/rating-dimensions` — list all 6 rating dimensions
//!
//! # Authorization
//!
//! None — these are read-only public lists. They power the frontend
//! registration / rating form. The `is_active` filter means a
//! future "decommissioned discipline" can be soft-hidden without
//! dropping the row.

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use serde_json::json;

use super::error::LookupError;
use super::service::{AcceptLanguage, LookupService, LocalizedCollege, LocalizedDimension, LocalizedDiscipline};
use crate::AppState;

pub fn lookup_router() -> Router<AppState> {
    Router::new()
        .route("/disciplines", get(list_disciplines))
        .route("/colleges", get(list_colleges))
        .route("/rating-dimensions", get(list_dimensions))
}

fn parse_lang(headers: &HeaderMap) -> AcceptLanguage {
    AcceptLanguage::parse(headers.get(header::ACCEPT_LANGUAGE).and_then(|v| v.to_str().ok()))
}

async fn list_disciplines(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<LocalizedDiscipline>>, ApiError> {
    let lang = parse_lang(&headers);
    let svc = LookupService::new(state.db.clone());
    let list = svc.list_disciplines(lang).await?;
    Ok(Json(list))
}

async fn list_colleges(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<LocalizedCollege>>, ApiError> {
    let lang = parse_lang(&headers);
    let svc = LookupService::new(state.db.clone());
    let list = svc.list_colleges(lang).await?;
    Ok(Json(list))
}

async fn list_dimensions(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Json<Vec<LocalizedDimension>>, ApiError> {
    let lang = parse_lang(&headers);
    let svc = LookupService::new(state.db.clone());
    let list = svc.list_dimensions(lang).await?;
    Ok(Json(list))
}

pub struct ApiError(LookupError);

impl From<LookupError> for ApiError {
    fn from(e: LookupError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let status = match &self.0 {
            LookupError::Database(_) => StatusCode::INTERNAL_SERVER_ERROR,
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self.0, "internal error in lookup handler");
        }
        let body = if status == StatusCode::INTERNAL_SERVER_ERROR {
            json!({ "error": "internal_error" })
        } else {
            json!({ "error": "internal_error", "message": self.0.to_string() })
        };
        (status, Json(body)).into_response()
    }
}
