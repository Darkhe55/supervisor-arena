//! Supervisor pages: `/supervisors` (search), `/supervisors/new` (create),
//! `/supervisors/{alias}` (detail).

use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect, Response},
    Form,
};
use serde::Deserialize;

use super::auth_pages::{load_colleges, load_disciplines};
use super::session::MaybeAuth;
use super::templates::{
    CollegeOption, DisciplineOption, SearchQuery, SearchResultRow, SearchResultsBlock,
    SupervisorDetailBlock, SupervisorDetailTemplate, SupervisorNewForm, SupervisorNewTemplate,
    SupervisorSearchTemplate,
};
use crate::supervisor::dto::{CreateSupervisorRequest, SearchEntry, SearchResponse};
use crate::AppState;

// --- GET /supervisors ---

pub async fn search_form(
    State(state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
) -> Response {
    let disciplines = load_disciplines(&state).await;
    let colleges = load_colleges(&state).await;
    let template = SupervisorSearchTemplate {
        is_logged_in: current_user.is_some(),
        user_tier: current_user.as_ref().map(|u| u.tier.clone()).unwrap_or_default(),
        flash: String::new(),
        disciplines,
        colleges,
        query: SearchQuery::default(),
        results: empty_results(),
        form_error: String::new(),
        has_results: false,
    };
    template.into_response()
}

fn empty_results() -> SearchResultsBlock {
    SearchResultsBlock {
        discipline_label: String::new(),
        college_label: String::new(),
        total: 0,
        results: Vec::new(),
    }
}

// --- POST /supervisors ---

#[derive(Debug, Deserialize)]
pub struct SearchSubmit {
    pub discipline: String,
    pub college: String,
}

pub async fn search_submit(
    State(state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
    Form(submit): Form<SearchSubmit>,
) -> Response {
    let disciplines = load_disciplines(&state).await;
    let colleges = load_colleges(&state).await;
    let query = SearchQuery {
        discipline: submit.discipline.clone(),
        college: submit.college.clone(),
    };
    let svc = build_supervisor_service(&state);
    let mut has_results = false;
    let mut results = empty_results();
    let mut form_error = String::new();
    if let Ok(svc) = &svc {
        match svc
            .search(
                &submit.discipline,
                &submit.college,
                50,
                0,
            )
            .await
        {
            Ok(r) => {
                has_results = true;
                results = to_results_block(&submit.discipline, &submit.college, r);
            }
            Err(e) => {
                form_error = format!("搜索失败: {e}");
            }
        }
    } else {
        form_error = "服务不可用".to_string();
    }
    let template = SupervisorSearchTemplate {
        is_logged_in: current_user.is_some(),
        user_tier: current_user.as_ref().map(|u| u.tier.clone()).unwrap_or_default(),
        flash: String::new(),
        disciplines,
        colleges,
        query,
        results,
        form_error,
        has_results,
    };
    template.into_response()
}

fn to_results_block(discipline: &str, college: &str, resp: SearchResponse) -> SearchResultsBlock {
    let discipline_label = discipline.to_string();
    let college_label = college.to_string();
    let rows: Vec<SearchResultRow> = resp
        .results
        .into_iter()
        .map(search_entry_to_row)
        .collect();
    SearchResultsBlock {
        discipline_label,
        college_label,
        total: resp.total,
        results: rows,
    }
}

fn search_entry_to_row(e: SearchEntry) -> SearchResultRow {
    SearchResultRow {
        alias: e.alias,
        discipline: e.discipline,
        college: e.college,
        composite_score: e.composite_score,
        rating_count: e.rating_count,
        visible: true, // search results already filtered server-side by k-anonymity
    }
}

// --- GET /supervisors/new ---

pub async fn new_form(
    State(state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
) -> Response {
    let current_user = match current_user {
        Some(u) => u,
        None => return Redirect::to("/login?flash=login_required").into_response(),
    };
    let disciplines = load_disciplines(&state).await;
    let colleges = load_colleges(&state).await;
    let template = SupervisorNewTemplate {
        is_logged_in: true,
        user_tier: current_user.tier.clone(),
        flash: String::new(),
        disciplines,
        colleges,
        form: SupervisorNewForm::default(),
        form_error: String::new(),
    };
    template.into_response()
}

// --- POST /supervisors/new ---

#[derive(Debug, Deserialize)]
pub struct NewSubmit {
    pub submitted_name: String,
    pub discipline: String,
    pub college: String,
}

pub async fn new_submit(
    State(state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
    Form(submit): Form<NewSubmit>,
) -> Response {
    let current_user = match current_user {
        Some(u) => u,
        None => return Redirect::to("/login?flash=login_required").into_response(),
    };
    let disciplines = super::auth_pages::load_disciplines(&state).await;
    let colleges = load_colleges(&state).await;
    let form = SupervisorNewForm {
        submitted_name: submit.submitted_name.clone(),
        discipline: submit.discipline.clone(),
        college: submit.college.clone(),
    };
    let svc = match build_supervisor_service(&state) {
        Ok(s) => s,
        Err(e) => {
            return render_new_error(current_user, disciplines, colleges, form, e.to_string());
        }
    };
    let req = CreateSupervisorRequest {
        submitted_name: submit.submitted_name,
        discipline: submit.discipline,
        college: submit.college,
    };
    match svc.create_request(current_user.account_id, req).await {
        Ok(resp) => Redirect::to(&format!("/supervisors/{}", resp.alias)).into_response(),
        Err(e) => render_new_error(current_user, disciplines, colleges, form, e.to_string()),
    }
}

fn render_new_error(
    current_user: super::session::CurrentUser,
    disciplines: Vec<DisciplineOption>,
    colleges: Vec<CollegeOption>,
    form: SupervisorNewForm,
    err: String,
) -> Response {
    let template = SupervisorNewTemplate {
        is_logged_in: true,
        user_tier: current_user.tier,
        flash: String::new(),
        disciplines,
        colleges,
        form,
        form_error: err,
    };
    template.into_response()
}

// --- GET /supervisors/{alias} ---

pub async fn detail(
    State(state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
    Path(alias): Path<String>,
) -> Response {
    eprintln!("!!! detail handler called with alias={:?}", alias);
    tracing::debug!(alias = %alias, "GET /supervisors/{alias}");
    let svc = match build_supervisor_service(&state) {
        Ok(s) => s,
        Err(e) => {
            return detail_error(current_user, e.to_string());
        }
    };
    match svc.public_view_by_alias(&alias).await {
        Ok(Some(v)) => {
            let supervisor = SupervisorDetailBlock {
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
            };
            let k_gated = !v.visible;
            let template = SupervisorDetailTemplate {
                is_logged_in: current_user.is_some(),
                user_tier: current_user.as_ref().map(|u| u.tier.clone()).unwrap_or_default(),
                flash: String::new(),
                supervisor,
                not_found: false,
                pending_review: false,
                k_gated,
            };
            template.into_response()
        }
        Ok(None) => detail_not_found(current_user),
        Err(e) => detail_error(current_user, e.to_string()),
    }
}

fn detail_not_found(current_user: Option<super::session::CurrentUser>) -> Response {
    let placeholder = empty_supervisor_detail(String::new());
    let template = SupervisorDetailTemplate {
        is_logged_in: current_user.is_some(),
        user_tier: current_user.as_ref().map(|u| u.tier.clone()).unwrap_or_default(),
        flash: String::new(),
        supervisor: placeholder,
        not_found: true,
        pending_review: false,
        k_gated: false,
    };
    let mut resp = template.into_response();
    *resp.status_mut() = StatusCode::NOT_FOUND;
    resp
}

fn detail_error(current_user: Option<super::session::CurrentUser>, err: String) -> Response {
    let placeholder = empty_supervisor_detail(String::new());
    let template = SupervisorDetailTemplate {
        is_logged_in: current_user.is_some(),
        user_tier: current_user.as_ref().map(|u| u.tier.clone()).unwrap_or_default(),
        flash: err,
        supervisor: placeholder,
        not_found: false,
        pending_review: false,
        k_gated: false,
    };
    let mut resp = template.into_response();
    *resp.status_mut() = StatusCode::INTERNAL_SERVER_ERROR;
    resp
}

fn empty_supervisor_detail(alias: String) -> SupervisorDetailBlock {
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

// --- helpers ---

fn build_supervisor_service(
    state: &AppState,
) -> anyhow::Result<crate::supervisor::service::SupervisorService> {
    use crate::aggregation::{AggregationService, RatingRepo as AggRepo};
    use crate::config::ReviewConfig;
    use crate::discipline::DisciplineRepo;
    use crate::supervisor::{alias::AliasGenerator, repo::SupervisorRepo};
    let repo = SupervisorRepo::new(state.db.clone());
    let keys = state.keys.clone();
    let alias_gen = AliasGenerator::from_keystore(&*keys);
    let review_cfg: ReviewConfig = state.config.review.clone();
    let aggregation = AggregationService::new(AggRepo::new(state.db.clone()));
    let discipline_repo = DisciplineRepo::new(state.db.clone());
    Ok(crate::supervisor::service::SupervisorService::new(
        repo,
        keys,
        alias_gen,
        review_cfg,
        aggregation,
    )
    .with_discipline_repo(discipline_repo))
}
