//! Auth pages: `/register`, `/login`, `/logout`, `/me`.
//!
//! These call into the same `AccountService` the JSON API uses — no
//! duplicate business logic. The only "new" thing here is rendering HTML
//! around the service results.

use axum::{
    extract::State,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Redirect, Response},
    Form,
};
use serde::Deserialize;

use super::session::{build_session_cookie, MaybeAuth};
use super::templates::{
    DisciplineOption, IndexTemplate, LoginForm, LoginTemplate, MeTemplate, RegisterForm,
    RegisterTemplate,
};
use crate::AppState;

const COOKIE_SECURE: bool = false; // dev. Production should set Secure flag.

// --- GET / ---

pub async fn index(
    State(_state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
) -> Response {
    let template = IndexTemplate::from(current_user, None);
    template.into_response()
}

// --- GET /register ---

pub async fn register_form(
    State(state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
) -> Response {
    let disciplines = load_disciplines(&state).await;
    let template = RegisterTemplate {
        is_logged_in: current_user.is_some(),
        user_tier: current_user.as_ref().map(|u| u.tier.clone()).unwrap_or_default(),
        flash: String::new(),
        form: RegisterForm::default(),
        disciplines,
        form_error: String::new(),
    };
    template.into_response()
}

// --- POST /register ---

#[derive(Debug, Deserialize)]
pub struct RegisterSubmit {
    pub email: String,
    pub password: String,
    pub discipline: String,
    pub institution: String,
    pub grade: Option<String>,
    pub invite_code: Option<String>,
}

pub async fn register_submit(
    State(state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
    Form(submit): Form<RegisterSubmit>,
) -> Response {
    let disciplines = load_disciplines(&state).await;
    let form = RegisterForm {
        email: submit.email.clone(),
        discipline: submit.discipline.clone(),
        institution: submit.institution.clone(),
        grade: submit.grade.clone().unwrap_or_default(),
        invite_code: submit.invite_code.clone().unwrap_or_default(),
    };
    let service = match build_account_service(&state) {
        Ok(s) => s,
        Err(e) => {
            return render_register_error(current_user, form, disciplines, e.to_string());
        }
    };
    let req = crate::account::dto::RegisterRequest {
        email: submit.email,
        password: submit.password,
        discipline: submit.discipline,
        institution: submit.institution,
        grade: submit.grade,
        invite_code: submit.invite_code,
    };
    match service.register(req, None).await {
        Ok(auth) => redirect_with_cookie("/me?flash=welcome", &auth.access_token, auth.expires_in),
        Err(e) => render_register_error(current_user, form, disciplines, friendly_error(e)),
    }
}

fn render_register_error(
    current_user: Option<super::session::CurrentUser>,
    form: RegisterForm,
    disciplines: Vec<DisciplineOption>,
    err: String,
) -> Response {
    let template = RegisterTemplate {
        is_logged_in: current_user.is_some(),
        user_tier: current_user.as_ref().map(|u| u.tier.clone()).unwrap_or_default(),
        flash: String::new(),
        form,
        disciplines,
        form_error: err,
    };
    template.into_response()
}

// --- GET /login ---

pub async fn login_form(
    State(_state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
) -> Response {
    let template = LoginTemplate {
        is_logged_in: current_user.is_some(),
        user_tier: current_user.as_ref().map(|u| u.tier.clone()).unwrap_or_default(),
        flash: String::new(),
        form: LoginForm::default(),
        form_error: String::new(),
    };
    template.into_response()
}

// --- POST /login ---

#[derive(Debug, Deserialize)]
pub struct LoginSubmit {
    pub email: String,
    pub password: String,
}

pub async fn login_submit(
    State(state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
    Form(submit): Form<LoginSubmit>,
) -> Response {
    let service = match build_account_service(&state) {
        Ok(s) => s,
        Err(e) => {
            return render_login_error(
                current_user,
                LoginForm {
                    email: submit.email,
                },
                e.to_string(),
            );
        }
    };
    let req = crate::account::dto::LoginRequest {
        email: submit.email.clone(),
        password: submit.password,
    };
    match service.login(req).await {
        Ok(auth) => redirect_with_cookie("/me?flash=logged_in", &auth.access_token, auth.expires_in),
        Err(e) => render_login_error(
            current_user,
            LoginForm {
                email: submit.email,
            },
            friendly_error(e),
        ),
    }
}

fn render_login_error(
    current_user: Option<super::session::CurrentUser>,
    form: LoginForm,
    err: String,
) -> Response {
    let template = LoginTemplate {
        is_logged_in: current_user.is_some(),
        user_tier: current_user.as_ref().map(|u| u.tier.clone()).unwrap_or_default(),
        flash: String::new(),
        form,
        form_error: err,
    };
    template.into_response()
}

// --- POST /logout ---

pub async fn logout() -> Response {
    let cookie = super::session::build_clear_cookie(COOKIE_SECURE);
    let mut resp = Redirect::to("/").into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        cookie.parse().expect("static cookie header is valid"),
    );
    resp
}

// --- GET /me ---

pub async fn me(
    State(_state): State<AppState>,
    MaybeAuth(current_user): MaybeAuth,
    headers: HeaderMap,
) -> Response {
    let current_user = match current_user {
        Some(u) => u,
        None => return Redirect::to("/login?flash=login_required").into_response(),
    };
    let _ = headers;
    let template = MeTemplate {
        is_logged_in: true,
        user_tier: current_user.tier.clone(),
        user_id: current_user.account_id.to_string(),
        flash: String::new(),
    };
    template.into_response()
}

// --- helpers ---

fn build_account_service(
    state: &AppState,
) -> anyhow::Result<crate::account::service::AccountService> {
    crate::account::service::AccountService::new(
        crate::account::repo::AccountRepo::new(state.db.clone()),
        state.keys.clone(),
        &state.config.auth,
    )
}

fn redirect_with_cookie(location: &str, token: &str, max_age_secs: i64) -> Response {
    let cookie = build_session_cookie(token, max_age_secs, COOKIE_SECURE);
    let mut resp = Redirect::to(location).into_response();
    resp.headers_mut().insert(
        header::SET_COOKIE,
        cookie.parse().expect("static cookie header is valid"),
    );
    resp
}

pub async fn load_disciplines(state: &AppState) -> Vec<DisciplineOption> {
    let svc = crate::lookup::service::LookupService::new(state.db.clone());
    match svc
        .list_disciplines(crate::lookup::service::AcceptLanguage::Zh)
        .await
    {
        Ok(list) => list
            .into_iter()
            .map(|d| DisciplineOption {
                code: d.code,
                name: d.name,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

pub async fn load_colleges(state: &AppState) -> Vec<super::templates::CollegeOption> {
    let svc = crate::lookup::service::LookupService::new(state.db.clone());
    match svc
        .list_colleges(crate::lookup::service::AcceptLanguage::Zh)
        .await
    {
        Ok(list) => list
            .into_iter()
            .map(|c| super::templates::CollegeOption {
                code: c.code,
                name: c.name,
            })
            .collect(),
        Err(_) => Vec::new(),
    }
}

/// Strip debug prefixes and the `AccountError` source from a service error
/// so the user sees something actionable, not a 5-line anyhow chain.
fn friendly_error(e: impl std::fmt::Display) -> String {
    let s = e.to_string();
    // Strip the leading "AccountError: " if present.
    s.trim_start_matches("AccountError: ")
        .trim_start_matches("SupervisorError: ")
        .trim_start_matches("RatingError: ")
        .to_string()
}

#[allow(dead_code)]
fn _unused_status(_code: StatusCode) {}
