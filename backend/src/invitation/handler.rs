//! axum handlers for the invitation module.
//!
//! Routes (mounted at /invitations in lib.rs):
//! - `POST /invitations`         — authed user creates a new code
//! - `GET  /invitations/me`      — list codes I created
//! - `GET  /invitations/:code`   — public lookup of a code's status
//!
//! The redemption itself happens inside /auth/register
//! (the register handler accepts an optional `invite_code` field
//! and calls `InvitationService::redeem` after the account row
//! is created). We don't expose a separate "POST /invitations/:code/redeem"
//! here because redemption must be atomic with registration.

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;

use super::error::InvitationError;
use super::service::InvitationService;
use crate::account::AuthAccount;
use crate::AppState;

pub fn invitation_router() -> Router<AppState> {
    Router::new()
        .route("/", post(create_invitation))
        .route("/me", get(list_my_invitations))
        .route("/:code", get(lookup_invitation))
}

#[derive(Debug, Deserialize)]
pub struct CreateInvitationRequest {
    /// Optional: how many times the code can be redeemed. Default 1.
    #[serde(default = "default_max_uses")]
    pub max_uses: i32,
    /// Optional: expiry in seconds from now. Default 30 days.
    /// NULL = use the service default.
    #[serde(default)]
    pub expires_in_secs: Option<i64>,
    /// Optional: free-form note ("for Alice, CS @ MIT").
    #[serde(default)]
    pub note: Option<String>,
}

fn default_max_uses() -> i32 {
    1
}

#[derive(Debug, serde::Serialize)]
pub struct InvitationResponse {
    /// The user-displayable code, with dashes.
    /// e.g. "7K9X-3RT1-A82B". Returned ONCE (at creation time);
    /// subsequent GETs return the canonical (dashed) form.
    pub code: String,
    /// Canonical row metadata.
    pub id: uuid::Uuid,
    pub created_at: chrono::DateTime<chrono::Utc>,
    pub max_uses: i32,
    pub expires_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn create_invitation(
    State(state): State<AppState>,
    auth: AuthAccount,
    Json(req): Json<CreateInvitationRequest>,
) -> Result<(StatusCode, Json<InvitationResponse>), ApiError> {
    // Build the InvitationService. We need an HMAC key to derive
    // the code; we use the application's hmac_key from
    // AppState::keys.
    let svc = InvitationService::new(
        super::repo::InvitationRepo::new(state.db.clone()),
        *state.keys.hmac_key(),
    );

    let expires_at = req
        .expires_in_secs
        .map(|s| chrono::Utc::now() + chrono::Duration::seconds(s));

    // Validate max_uses (the DB also has a CHECK constraint but we
    // fail fast with a nicer error here).
    if req.max_uses < 1 || req.max_uses > 1000 {
        return Err(ApiError(InvitationError::Database(anyhow::anyhow!(
            "max_uses must be between 1 and 1000"
        ))));
    }

    let (display_code, row) = svc
        .create(Some(auth.account_id), req.max_uses, expires_at, req.note.as_deref())
        .await?;

    Ok((
        StatusCode::CREATED,
        Json(InvitationResponse {
            code: display_code,
            id: row.id,
            created_at: row.created_at,
            max_uses: row.max_uses,
            expires_at: row.expires_at,
        }),
    ))
}

async fn list_my_invitations(
    State(state): State<AppState>,
    auth: AuthAccount,
) -> Result<Json<serde_json::Value>, ApiError> {
    let svc = InvitationService::new(
        super::repo::InvitationRepo::new(state.db.clone()),
        *state.keys.hmac_key(),
    );
    let rows = svc.list_by_creator(auth.account_id).await?;
    // Strip the raw 12-char code from the response — the
    // inviter only ever sees the dashed display form. (They can
    // look it up by id if they need the raw form again.)
    let json_rows: Vec<_> = rows
        .into_iter()
        .map(|r| {
            json!({
                "id": r.id,
                "display_code": InvitationService::format_code(&r.code),
                "created_at": r.created_at,
                "max_uses": r.max_uses,
                "use_count": r.use_count,
                "expires_at": r.expires_at,
                "revoked_at": r.revoked_at,
                "note": r.note,
            })
        })
        .collect();
    Ok(Json(json!(json_rows)))
}

async fn lookup_invitation(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    // Public — no auth. Lookup is read-only, the response is
    // limited to "exists / not exists" + lifecycle status.
    let svc = InvitationService::new(
        super::repo::InvitationRepo::new(state.db.clone()),
        *state.keys.hmac_key(),
    );
    let row = svc
        .lookup(&code)
        .await?
        .ok_or(ApiError(InvitationError::CodeNotFound(code.clone())))?;
    let now = chrono::Utc::now();
    let status = if row.revoked_at.is_some() {
        "revoked"
    } else if row.use_count >= row.max_uses {
        "fully_used"
    } else if row.expires_at.map_or(false, |e| now > e) {
        "expired"
    } else {
        "active"
    };
    Ok(Json(json!({
        "status": status,
        "max_uses": row.max_uses,
        "use_count": row.use_count,
        "remaining": row.max_uses - row.use_count,
        "expires_at": row.expires_at,
    })))
}

pub struct ApiError(InvitationError);

impl From<InvitationError> for ApiError {
    fn from(e: InvitationError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            InvitationError::CodeNotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            InvitationError::FullyUsed => (StatusCode::CONFLICT, "fully_used"),
            InvitationError::Expired(_) => (StatusCode::GONE, "expired"),
            InvitationError::Revoked(_) => (StatusCode::FORBIDDEN, "revoked"),
            InvitationError::InvalidInviter(_) => {
                (StatusCode::BAD_REQUEST, "invalid_inviter")
            }
            InvitationError::Database(_) => {
                (StatusCode::INTERNAL_SERVER_ERROR, "internal_error")
            }
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self.0, "internal error in invitation handler");
        }
        let body = if status == StatusCode::INTERNAL_SERVER_ERROR {
            json!({ "error": code })
        } else {
            json!({ "error": code, "message": self.0.to_string() })
        };
        (status, Json(body)).into_response()
    }
}
