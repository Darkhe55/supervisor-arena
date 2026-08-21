//! axum handlers and the auth sub-router
//!
//! Routes:
//! - `POST /auth/register` — body: RegisterRequest -> 201 AuthResponse
//! - `POST /auth/login`    — body: LoginRequest    -> 200 AuthResponse
//! - `GET  /auth/me`       — Authorization: Bearer ... -> 200 AccountResponse
//! - `POST /auth/admin/soft-remove`  — admin: mark account soft-removed
//! - `POST /auth/admin/ban`          — admin: ban / unban
//!
//! Error mapping:
//! - `AccountError::InvalidEmail`       -> 400 with message
//! - `AccountError::WeakPassword`       -> 400 with message
//! - `AccountError::InvalidField`       -> 400 with message
//! - `AccountError::EmailTaken`         -> 409
//! - `AccountError::InvalidCredentials` -> 401 (also /auth/me on missing acct)
//! - `AccountError::InvalidToken`       -> 401
//! - `AccountError::AccountUnavailable` -> 403
//! - `AccountError::RateLimited`        -> 429
//! - everything else                    -> 500 (logged, no detail leaked)

use axum::{
    async_trait,
    extract::{ConnectInfo, FromRequestParts, State},
    http::{header, request::Parts, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde_json::json;
use std::net::SocketAddr;
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
        .route("/cancel", post(cancel_account))
        .route("/admin/soft-remove", post(admin_soft_remove))
        .route("/admin/ban", post(admin_ban))
}

// --- Handlers ---

async fn register(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<RegisterRequest>,
) -> Result<(StatusCode, Json<AuthResponse>), ApiError> {
    let service = account_service(&state)?;
    // M5 邀请试用: build the InvitationService so the register
    // flow can redeem an optional invite code in the same flow.
    let invitation = if req.invite_code.is_some() {
        Some(crate::invitation::InvitationService::new(
            crate::invitation::InvitationRepo::new(state.db.clone()),
            *state.keys.hmac_key(),
        ))
    } else {
        None
    };
    let resp = service.register(req, invitation.as_ref()).await?;
    // M6 §7.9.5 — log the encrypted-field access. We log AFTER
    // the insert succeeds so failed registers don't leave a phantom
    // audit row. Best-effort: the writer never propagates errors.
    let xff = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    state.audit.log_with_ip(
        crate::audit::EncryptionAccess {
            field: "accounts.email_enc",
            account_id: Some(resp.account_id),
            accessor: "account::handler::register",
            purpose: crate::audit::AuditPurpose::Login,
            ip_hash: None,
            success: true,
        },
        xff,
        Some(addr),
        state.keys.hmac_key(),
    ).await;
    Ok((StatusCode::CREATED, Json(resp)))
}

async fn login(
    State(state): State<AppState>,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<LoginRequest>,
) -> Result<Json<AuthResponse>, ApiError> {
    // M3 §7.6 / E-3: per-IP per-minute login rate limit (5/min).
    // We check BEFORE the DB lookup so a brute-force attempt
    // doesn't pay for a bcrypt verify per request. The IP comes
    // from X-Forwarded-For first hop, then connection peer.
    let xff = headers
        .get("x-forwarded-for")
        .and_then(|v| v.to_str().ok());
    let ip = crate::rate_limit::LoginRateLimiter::extract_ip(xff, Some(addr));
    if let Err(e) = state.rate_limit.login.check_and_record(&ip) {
        return Err(ApiError(AccountError::from(e)));
    }
    let service = account_service(&state)?;
    let resp = service.login(req).await?;
    // M6 §7.9.5 — log the login (P1 fields accessed via JWT
    // verification). ip_hash from XFF first hop, falls back to
    // peer address.
    state.audit.log_with_ip(
        crate::audit::EncryptionAccess {
            field: "accounts.password_hash",
            account_id: Some(resp.account_id),
            accessor: "account::handler::login",
            purpose: crate::audit::AuditPurpose::Login,
            ip_hash: None,
            success: true,
        },
        xff,
        Some(addr),
        state.keys.hmac_key(),
    ).await;
    Ok(Json(resp))
}

async fn me(
    State(state): State<AppState>,
    auth: AuthAccount,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
) -> Result<Json<AccountResponse>, ApiError> {
    let service = account_service(&state)?;
    let resp = service.get(auth.account_id).await?;
    // M6 §7.9.5 — /auth/me touches the encrypted PII read path.
    let xff = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    state.audit.log_with_ip(
        crate::audit::EncryptionAccess {
            field: "accounts.email_enc",
            account_id: Some(auth.account_id),
            accessor: "account::handler::me",
            purpose: crate::audit::AuditPurpose::Login,
            ip_hash: None,
            success: true,
        },
        xff,
        Some(addr),
        state.keys.hmac_key(),
    ).await;
    Ok(Json(resp))
}

// --- Admin endpoints (H-48) ---

#[derive(Debug, serde::Deserialize)]
struct AdminTargetRequest {
    target_id: Uuid,
    /// `true` to enable the flag, `false` to clear it.
    value: bool,
}

async fn admin_soft_remove(
    State(state): State<AppState>,
    _auth: AuthAccount,
    Json(req): Json<AdminTargetRequest>,
) -> Result<StatusCode, ApiError> {
    let service = account_service(&state)?;
    service
        .admin_set_soft_removed(req.target_id, req.value)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

async fn admin_ban(
    State(state): State<AppState>,
    _auth: AuthAccount,
    Json(req): Json<AdminTargetRequest>,
) -> Result<StatusCode, ApiError> {
    let service = account_service(&state)?;
    service
        .admin_set_banned(req.target_id, req.value)
        .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// M3 §7.4 — User-initiated account cancellation.
///
/// Anonymizes the account in place: clears PII, marks is_cancelled,
/// sets cancelled_at. Existing ratings keep counting toward
/// aggregation. The endpoint is irreversible — the user cannot
/// "uncancel" via the public API.
async fn cancel_account(
    State(state): State<AppState>,
    auth: AuthAccount,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
) -> Result<StatusCode, ApiError> {
    let service = account_service(&state)?;
    service.cancel_account(auth.account_id).await?;
    // M6 §7.9.5 — cancellation is an irreversible encryption-
    // related event; log it.
    let xff = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    state.audit.log_with_ip(
        crate::audit::EncryptionAccess {
            field: "accounts.email_enc",
            account_id: Some(auth.account_id),
            accessor: "account::handler::cancel",
            purpose: crate::audit::AuditPurpose::Cancellation,
            ip_hash: None,
            success: true,
        },
        xff,
        Some(addr),
        state.keys.hmac_key(),
    ).await;
    Ok(StatusCode::NO_CONTENT)
}

// --- Extractor for the Authorization header ---

/// Extractor that pulls a Bearer token from `Authorization`, verifies it,
/// and yields the account UUID + tier. Returns 401 on any failure.
pub struct AuthAccount {
    pub account_id: Uuid,
    pub tier: String,
}

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
        let claims = service.jwt().verify(token)?;
        let id = Uuid::parse_str(&claims.sub).map_err(|_| AccountError::MalformedSubject)?;
        Ok(AuthAccount {
            account_id: id,
            tier: claims.tier,
        })
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
            AccountError::RateLimited { .. } => (StatusCode::TOO_MANY_REQUESTS, "rate_limited"),
            AccountError::Database(_) | AccountError::Crypto(_) | AccountError::Jwt(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };

        // Log internal errors with detail; client only sees the code.
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self.0, "internal error in account handler");
        }

        // For 429, surface the kind + retry hint so the client can
        // back off correctly. Other branches use the standard
        // { error, message } body.
        let body = match &self.0 {
            AccountError::RateLimited { kind, retry_after_secs } => json!({
                "error": code,
                "message": self.0.to_string(),
                "kind": kind,
                "retry_after_secs": retry_after_secs,
            }),
            _ if status == StatusCode::INTERNAL_SERVER_ERROR => json!({ "error": code }),
            _ => json!({ "error": code, "message": self.0.to_string() }),
        };
        let mut resp = (status, Json(body)).into_response();
        if let AccountError::RateLimited { retry_after_secs, .. } = self.0 {
            use axum::http::HeaderValue;
            if let Ok(v) = HeaderValue::from_str(&retry_after_secs.to_string()) {
                resp.headers_mut().insert("Retry-After", v);
            }
        }
        resp
    }
}
