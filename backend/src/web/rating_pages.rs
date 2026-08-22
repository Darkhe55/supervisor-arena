//! Rating pages: `/supervisors/{alias}/rate` (form + submit).

use axum::{
    extract::{Path, State},
    response::{IntoResponse, Redirect, Response},
    Form,
};
use serde::Deserialize;

use super::session::MaybeAuth;
use super::templates::{RatingFormTemplate, SupervisorDetailBlock};
use crate::rating::dto::SubmitRatingRequest;
use crate::AppState;

// --- GET /supervisors/{alias}/rate ---

pub async fn rate_form(
    State(state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
    Path(alias): Path<String>,
) -> Response {
    let current_user = match current_user {
        Some(u) => u,
        None => return Redirect::to("/login?flash=login_required").into_response(),
    };
    let supervisor = match lookup_supervisor(&state, &alias).await {
        Ok(s) => s,
        Err(e) => {
            return render_form_error(current_user, e.to_string(), empty_supervisor(alias));
        }
    };
    let template = RatingFormTemplate {
        is_logged_in: true,
        user_tier: current_user.tier.clone(),
        flash: String::new(),
        supervisor,
        form_error: String::new(),
    };
    template.into_response()
}

// --- POST /supervisors/{alias}/rate ---

#[derive(Debug, Deserialize)]
pub struct RateSubmit {
    pub dim: String,
    pub value: i16,
    pub dim_additional: Option<String>,
    pub overall_additional: Option<String>,
    pub additional_level: Option<String>,
}

pub async fn rate_submit(
    State(state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
    Path(alias): Path<String>,
    Form(submit): Form<RateSubmit>,
) -> Response {
    let current_user = match current_user {
        Some(u) => u,
        None => return Redirect::to("/login?flash=login_required").into_response(),
    };
    let supervisor = match lookup_supervisor(&state, &alias).await {
        Ok(s) => s,
        Err(e) => {
            return render_form_error(current_user, e.to_string(), empty_supervisor(alias.clone()));
        }
    };
    let svc = match build_rating_service(&state) {
        Ok(s) => s,
        Err(e) => return render_form_error(current_user, e.to_string(), supervisor),
    };
    let req = SubmitRatingRequest {
        dim: submit.dim.clone(),
        value: submit.value,
        dim_additional: submit.dim_additional.filter(|s| !s.is_empty()),
        overall_additional: submit.overall_additional.filter(|s| !s.is_empty()),
        additional_level: submit.additional_level.filter(|s| !s.is_empty()),
        evidence: Vec::new(),
    };
    match svc
        .submit(current_user.account_id, &alias, req)
        .await
    {
        Ok(_resp) => Redirect::to(&format!("/supervisors/{alias}?flash=rated"))
            .into_response(),
        Err(e) => render_form_error(current_user, e.to_string(), supervisor),
    }
}

fn render_form_error(
    current_user: super::session::CurrentUser,
    err: String,
    supervisor: SupervisorDetailBlock,
) -> Response {
    let template = RatingFormTemplate {
        is_logged_in: true,
        user_tier: current_user.tier,
        flash: String::new(),
        supervisor,
        form_error: err,
    };
    template.into_response()
}

fn empty_supervisor(alias: String) -> SupervisorDetailBlock {
    SupervisorDetailBlock {
        alias,
        discipline: String::new(),
        college: String::new(),
        composite_score: None,
        rating_count: 0,
        k_anonymity_count: 0,
        visible: false,
        radar_research: None,
        radar_resource: None,
        radar_fit: None,
        radar_currency: None,
        radar_ethic: None,
        radar_tool: None,
    }
}

async fn lookup_supervisor(
    state: &AppState,
    alias: &str,
) -> anyhow::Result<SupervisorDetailBlock> {
    use crate::aggregation::{AggregationService, RatingRepo as AggRepo};
    use crate::config::ReviewConfig;
    use crate::supervisor::{alias::AliasGenerator, repo::SupervisorRepo};
    let repo = SupervisorRepo::new(state.db.clone());
    let keys = state.keys.clone();
    let alias_gen = AliasGenerator::from_keystore(&*keys);
    let review_cfg: ReviewConfig = state.config.review.clone();
    let aggregation = AggregationService::new(AggRepo::new(state.db.clone()));
    let svc = crate::supervisor::service::SupervisorService::new(
        repo,
        keys,
        alias_gen,
        review_cfg,
        aggregation,
    );
    let v = svc
        .public_view_by_alias(alias)
        .await
        .map_err(|e| anyhow::anyhow!("{e}"))?
        .ok_or_else(|| anyhow::anyhow!("supervisor not found"))?;
    Ok(SupervisorDetailBlock {
        alias: v.alias,
        discipline: v.discipline,
        college: v.college,
        composite_score: v.composite_score,
        rating_count: v.rating_count,
        k_anonymity_count: v.k_anonymity_count,
        visible: v.visible,
        radar_research: v.radar.research,
        radar_resource: v.radar.resource,
        radar_fit: v.radar.fit,
        radar_currency: v.radar.currency,
        radar_ethic: v.radar.ethic,
        radar_tool: v.radar.tool,
    })
}

fn build_rating_service(
    state: &AppState,
) -> anyhow::Result<crate::rating::service::RatingService> {
    use crate::account::repo::AccountRepo;
    use crate::rating::repo::RatingRepo;
    use crate::supervisor::repo::SupervisorRepo;
    let rating_repo = RatingRepo::new(state.db.clone());
    let supervisor_repo = SupervisorRepo::new(state.db.clone());
    let account_repo = AccountRepo::new(state.db.clone());
    let keys = state.keys.clone();
    let review_cfg: crate::config::ReviewConfig = state.config.review.clone();
    Ok(crate::rating::service::RatingService::new(
        rating_repo,
        supervisor_repo,
        account_repo,
        keys,
        review_cfg,
    ))
}
