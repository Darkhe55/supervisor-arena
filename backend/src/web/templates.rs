//! askama template structs for the user UI.
//!
//! Askama 0.12 does NOT support `if let Some(x) = y` syntax, so we
//! pre-flatten `Option`s into (1) a `bool` for "is present" checks and
//! (2) a `String` for the value. The handler sets them from the
//! `Option<CurrentUser>` / `Option<String>` source values.

use askama::Template;
use serde::Serialize;

use super::session::CurrentUser;

#[derive(Template)]
#[template(path = "index.html")]
pub struct IndexTemplate {
    pub is_logged_in: bool,
    pub user_tier: String,
    pub flash: String,
}

impl IndexTemplate {
    pub fn from(current_user: Option<CurrentUser>, flash: Option<String>) -> Self {
        Self {
            is_logged_in: current_user.is_some(),
            user_tier: current_user.as_ref().map(|u| u.tier.clone()).unwrap_or_default(),
            flash: flash.unwrap_or_default(),
        }
    }
}

#[derive(Template)]
#[template(path = "register.html")]
pub struct RegisterTemplate {
    pub is_logged_in: bool,
    pub user_tier: String,
    pub flash: String,
    pub form: RegisterForm,
    pub disciplines: Vec<DisciplineOption>,
    pub form_error: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct DisciplineOption {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct RegisterForm {
    pub email: String,
    pub discipline: String,
    pub institution: String,
    pub grade: String,
    pub invite_code: String,
}

#[derive(Template)]
#[template(path = "login.html")]
pub struct LoginTemplate {
    pub is_logged_in: bool,
    pub user_tier: String,
    pub flash: String,
    pub form: LoginForm,
    pub form_error: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct LoginForm {
    pub email: String,
}

#[derive(Template)]
#[template(path = "me.html")]
pub struct MeTemplate {
    pub is_logged_in: bool,
    pub user_tier: String,
    pub user_id: String,
    pub flash: String,
}

#[derive(Template)]
#[template(path = "supervisor_search.html")]
pub struct SupervisorSearchTemplate {
    pub is_logged_in: bool,
    pub user_tier: String,
    pub flash: String,
    pub disciplines: Vec<DisciplineOption>,
    pub colleges: Vec<CollegeOption>,
    pub query: SearchQuery,
    pub results: SearchResultsBlock, // empty when no search yet
    pub form_error: String,
    pub has_results: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct CollegeOption {
    pub code: String,
    pub name: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SearchQuery {
    pub discipline: String,
    pub college: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResultsBlock {
    pub discipline_label: String,
    pub college_label: String,
    pub total: i64,
    pub results: Vec<SearchResultRow>,
}

#[derive(Debug, Clone, Serialize)]
pub struct SearchResultRow {
    pub alias: String,
    pub discipline: String,
    pub college: String,
    pub composite_score: Option<f64>,
    pub rating_count: i32,
    pub visible: bool,
}

#[derive(Template)]
#[template(path = "supervisor_new.html")]
pub struct SupervisorNewTemplate {
    pub is_logged_in: bool,
    pub user_tier: String,
    pub flash: String,
    pub disciplines: Vec<DisciplineOption>,
    pub colleges: Vec<CollegeOption>,
    pub form: SupervisorNewForm,
    pub form_error: String,
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct SupervisorNewForm {
    pub submitted_name: String,
    pub discipline: String,
    pub college: String,
}

#[derive(Template)]
#[template(path = "supervisor_detail.html")]
pub struct SupervisorDetailTemplate {
    pub is_logged_in: bool,
    pub user_tier: String,
    pub flash: String,
    pub supervisor: SupervisorDetailBlock,
    pub not_found: bool,
    pub pending_review: bool,
    pub k_gated: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct SupervisorDetailBlock {
    pub alias: String,
    pub discipline: String,
    pub college: String,
    pub composite_score: Option<f64>,
    pub rating_count: i32,
    pub k_anonymity_count: i32,
    pub visible: bool,
    pub radar_research: Option<f64>,
    pub radar_resource: Option<f64>,
    pub radar_fit: Option<f64>,
    pub radar_currency: Option<f64>,
    pub radar_ethic: Option<f64>,
    pub radar_tool: Option<f64>,
}

#[derive(Template)]
#[template(path = "rating_form.html")]
pub struct RatingFormTemplate {
    pub is_logged_in: bool,
    pub user_tier: String,
    pub flash: String,
    pub supervisor: SupervisorDetailBlock,
    pub form_error: String,
}
