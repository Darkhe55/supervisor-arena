//! axum handlers for the discipline-weight-voting module.
//!
//! Routes (mounted at `/disciplines/*` in `lib.rs`):
//! - `POST /disciplines/:code/weight-votes`           — submit a proposal
//! - `GET  /disciplines/:code/weight-votes`           — list pending proposals
//! - `GET  /disciplines/:code/weight-votes/:id`       — single proposal detail
//! - `POST /disciplines/:code/weight-votes/:id/ballot` — cast agree/disagree
//! - `GET  /disciplines/:code/weights`                — current applied weights
//! - `GET  /disciplines/:code/weights/history`        — history (audit + chart)

use axum::{
    extract::{ConnectInfo, Path, Query, State},
    http::{HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use serde::Deserialize;
use serde_json::json;
use uuid::Uuid;

use super::dto::{
    BallotChoice, CastBallotRequest, CurrentWeightsResponse, SubmitVoteRequest,
    VoteDetail, VoteSummary, WeightHistoryEntry,
};
use super::error::DisciplineError;
use super::service::DisciplineService;
use crate::account::AuthAccount;
use crate::AppState;

pub fn discipline_router() -> Router<AppState> {
    Router::new()
        .route(
            "/:code/weight-votes",
            post(submit_vote).get(list_pending_votes),
        )
        .route("/:code/weight-votes/:id", get(get_vote))
        .route(
            "/:code/weight-votes/:id/ballot",
            post(cast_ballot),
        )
        .route("/:code/weights", get(get_current_weights))
        .route("/:code/weights/history", get(list_weight_history))
}

// --- Handlers ---

async fn submit_vote(
    State(state): State<AppState>,
    auth: AuthAccount,
    Path(code): Path<String>,
    ConnectInfo(addr): ConnectInfo<std::net::SocketAddr>,
    headers: HeaderMap,
    Json(req): Json<SubmitVoteRequest>,
) -> Result<(StatusCode, Json<Uuid>), ApiError> {
    let svc = service(&state)?;
    let vote_id = svc
        .submit_vote(
            &code,
            &req.dim,
            req.proposed_weight,
            req.reason.as_deref(),
            auth.account_id,
        )
        .await?;
    // M6 — weight-vote submit (reason field is plain text but the
    // operation is governance-related; we log the action for the
    // audit trail).
    let xff = headers.get("x-forwarded-for").and_then(|v| v.to_str().ok());
    state.audit.log_with_ip(
        crate::audit::EncryptionAccess {
            field: "discipline_weight_votes.reason",
            account_id: Some(auth.account_id),
            accessor: "discipline::handler::submit_vote",
            purpose: crate::audit::AuditPurpose::Review,
            ip_hash: None,
            success: true,
        },
        xff,
        Some(addr),
        state.keys.hmac_key(),
    ).await;
    Ok((StatusCode::CREATED, Json(vote_id)))
}

async fn list_pending_votes(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<Vec<VoteSummary>>, ApiError> {
    let svc = service(&state)?;
    let list = svc.list_pending_votes(&code).await?;
    Ok(Json(list))
}

async fn get_vote(
    State(state): State<AppState>,
    Path((code, id)): Path<(String, Uuid)>,
) -> Result<Json<VoteDetail>, ApiError> {
    let svc = service(&state)?;
    let detail = svc
        .get_vote(id)
        .await?
        .ok_or(ApiError(DisciplineError::VoteNotFound(id)))?;
    // Also verify the vote's discipline matches the URL code.
    if detail.discipline != code {
        return Err(ApiError(DisciplineError::VoteNotFound(id)));
    }
    Ok(Json(detail))
}

async fn cast_ballot(
    State(state): State<AppState>,
    auth: AuthAccount,
    Path((_code, id)): Path<(String, Uuid)>,
    Json(req): Json<CastBallotRequest>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let svc = service(&state)?;
    let outcome = svc.cast_ballot(id, auth.account_id, req.choice).await?;
    // M3 §7.6: also rate-limit ballots? They're rarer than
    // ratings but can still be abused. For M3 MVP we leave
    // ballots un-rate-limited — the per-account daily rating
    // limit already caps the worst case. M5+ can revisit.
    Ok(Json(json!({
        "vote_id": outcome.vote_id,
        "agree_count": outcome.agree_count,
        "disagree_count": outcome.disagree_count,
        "applied": outcome.applied,
    })))
}

async fn get_current_weights(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<CurrentWeightsResponse>, ApiError> {
    let svc = service(&state)?;
    let view = svc.get_current_weights(&code).await?;
    Ok(Json(CurrentWeightsResponse {
        discipline: code,
        weights: view.entries,
        sum: view.sum,
        last_applied_at: view.last_applied_at,
    }))
}

#[derive(Debug, Deserialize)]
pub struct HistoryQuery {
    pub dim: Option<String>,
    #[serde(default = "default_history_limit")]
    pub limit: i64,
}

fn default_history_limit() -> i64 {
    50
}

async fn list_weight_history(
    State(state): State<AppState>,
    Path(code): Path<String>,
    Query(q): Query<HistoryQuery>,
) -> Result<Json<Vec<WeightHistoryEntry>>, ApiError> {
    let svc = service(&state)?;
    let list = svc
        .list_weight_history(&code, q.dim.as_deref(), q.limit)
        .await?;
    Ok(Json(list))
}

// --- Helpers ---

fn service(state: &AppState) -> Result<DisciplineService, ApiError> {
    use super::repo::DisciplineRepo;
    Ok(DisciplineService::new(DisciplineRepo::new(state.db.clone())))
}

// --- Error wrapper ---

pub struct ApiError(DisciplineError);

impl From<DisciplineError> for ApiError {
    fn from(e: DisciplineError) -> Self {
        ApiError(e)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self.0 {
            DisciplineError::UnknownDiscipline(_)
            | DisciplineError::InvalidDim(_)
            | DisciplineError::InvalidWeight(_) => (StatusCode::BAD_REQUEST, "invalid_input"),
            DisciplineError::NotEligible { .. }
            | DisciplineError::CooldownActive { .. }
            | DisciplineError::SelfBallot => (StatusCode::FORBIDDEN, "forbidden"),
            DisciplineError::AlreadyVoted(_) => (StatusCode::CONFLICT, "already_voted"),
            DisciplineError::VoteNotPending(_, _) => (StatusCode::CONFLICT, "vote_not_pending"),
            DisciplineError::VoteNotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            DisciplineError::Database(_) => (StatusCode::INTERNAL_SERVER_ERROR, "internal_error"),
        };
        if status == StatusCode::INTERNAL_SERVER_ERROR {
            tracing::error!(error = ?self.0, "internal error in discipline handler");
        }
        let body = if status == StatusCode::INTERNAL_SERVER_ERROR {
            json!({ "error": code })
        } else {
            json!({ "error": code, "message": self.0.to_string() })
        };
        (status, Json(body)).into_response()
    }
}

// Quiet the unused-import warning for `BallotChoice` (it's only used via
// the Deserialize derive in `CastBallotRequest`).
#[allow(dead_code)]
fn _ballot_choice_phantom(_: BallotChoice) {}
