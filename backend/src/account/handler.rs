//! axum handlers and the auth sub-router
//!
//! Routes:
//! - `POST /auth/register` — body: RegisterRequest -> 201 AuthResponse
//! - `POST /auth/login`    — body: LoginRequest    -> 200 AuthResponse
//! - `GET  /auth/me`       — Authorization: Bearer ... -> 200 AccountResponse
//!
//! Error mapping:
//! - `AccountError::InvalidEmail`       -> 400 with message
//! - `AccountError::WeakPassword`       -> 400 with message
//! - `AccountError::InvalidField`       -> 400 with message
//! - `AccountError::EmailTaken`         -> 409
//! - `AccountError::InvalidCredentials` -> 401 (also /auth/me on missing acct)
//! - `AccountError::InvalidToken`       -> 401
//! - `AccountError::AccountUnavailable` -> 403
//! - everything else                    -> 500 (logged, no detail leaked)

use axum::{
    async_trait,
    extract::{FromRequestParts, State},
    http::{header, request::Parts, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use uuid::Uuid;

use super::dto::{AccountResponse, AuthResponse, LoginRequest, RegisterRequest};
use super::error::AccountError;
use super::service::AccountService;
use crate::AppState;

/// Build the `/auth/*` sub-router and attach it to the main app.
///
/// Note: when used with `Router::nest("/auth", ...)` the inner paths must
/// NOT include the `/auth` prefix (nest does not strip it).
pub fn auth_router() -> Router<AppState> {
    Router::new()
        .route("/register", post(register))
        .route("/login", post(login))
        .route("/me", get(me))
}

// --- Handlers ---

async fn register(
    State(state): State<AppState>,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), ApiError> {
    let service = account_service(&state)?;
    let resp = service.register(req).await?;
    Ok((StatusCode::CREATED, Json(resp)))
}

async fn login(
    State(state): State<AppState>,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    let service = account_service(&state)?;
    let resp = service.login(req).await?;
    Ok(Json(resp))
}

async fn me(
    State(state): State<AppState>,
    auth: AuthAccount,
) -> Result<Json<AccountResponse>, ApiError> {
    let service = account_service(&state)?;
    let resp = service.get(auth.0).await?;
    Ok(Json(resp))
}

// --- Extractor for the Authorization header ---

/// Extractor that pulls a Bearer token from `Authorization`, verifies it,
/// and yields the account UUID. Returns 401 on any failure.
pub struct AuthAccount(pub Uuid);

#[async_trait]
impl FromRequestParts<AppState> for AuthAccount {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Self::Rejection> {
        let header_val = parts
            .headers
            .get(header::AUTHORIZATION)
            .ok_or(AccountError::InvalidToken)?
            .to_str()
            .map_err(|_| AccountError::InvalidToken)?;

        let token = header_val
            .strip_prefix("Bearer ")
            .ok_or(AccountError::InvalidToken)?;

        let service = account_service(state)?;
        let id = service.jwt().verify_account_id(token)?;
        Ok(AuthAccount(id))
    }
}

// --- Helpers ---

fn account_service(state: &AppState) -> Result<AccountService, ApiError> {
    AccountService::new(
        crate::account::repo::AccountRepo::new(state.db.clone()),
        state.keys.clone(),
        &state.config.auth,
    )
    .map_err(|e| {
        tracing::error!(error = ?e, "failed to build AccountService (likely bad config)");
        ApiError(AccountError::AccountUnavailable) // maps to 403; safer than 500 detail
    })
}

// --- Error type that maps AccountError -> HTTP ---

/// Public-facing error type. Maps `AccountError` to status + JSON body.
/// We never include the original error message in the 5xx body.
pub struct ApiError(AccountError);

impl From<AccountError> for ApiError {
    fn from(e: AccountError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            AccountError::InvalidEmail(_)
            | AccountError::WeakPassword(_)
            | AccountError::InvalidField { .. } => (StatusCode::BAD_REQUEST, "invalid_input"),
            AccountError::EmailTaken => (StatusCode::CONFLICT, "email_taken"),
            AccountError::InvalidCredentials | AccountError::InvalidToken
            | AccountError::MalformedSubject => (StatusCode::UNAUTHORIZED, "unauthorized"),
            AccountError::AccountUnavailable => (StatusCode::FORBIDDEN, "unavailable"),
            AccountError::RateLimited(_) => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            AccountError::Database(_) | AccountError::Crypto(_) | AccountError::Jwt(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };

        // Log internal errors with detail; client only sees the code.
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self.0, "internal error in account handler");
        }

        let body = if status == StatusCode::INTERNAL_SERVER_ERROR {
            json!({ "error": code })
        } else {
            // Surface the message for client errors (validation, auth).
            json!({ "error": code, "message": self.0.to_string() })
        };
        (status, Json(body)).into_response()
    }
}
