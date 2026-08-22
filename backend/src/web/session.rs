//! Session management for the user UI: a signed JWT stored in an httpOnly
//! cookie. The cookie name is `sa_jwt`; the server reads it on every request
//! and tries to verify. If valid, `MaybeAuth` yields `Some(CurrentUser)`;
//! if missing or invalid, it yields `None` (page renders logged-out).

use axum::{
    async_trait,
    extract::FromRequestParts,
    http::{header, request::Parts},
};
use uuid::Uuid;

use crate::AppState;

pub const COOKIE_NAME: &str = "sa_jwt";

/// Current logged-in user. Populated by [`MaybeAuth`] when a valid session
/// cookie is present.
#[derive(Debug, Clone)]
pub struct CurrentUser {
    pub account_id: Uuid,
    pub tier: String,
}

/// Extractor that yields `Option<CurrentUser>` — pages can render for both
/// logged-in and logged-out users. Unlike `account::handler::AuthAccount`,
/// missing / invalid cookies do NOT cause a 401; they yield `None`.
pub struct MaybeAuth(pub Option<CurrentUser>);

#[async_trait]
impl FromRequestParts<AppState> for MaybeAuth {
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let user = read_session(parts, state).await;
        Ok(MaybeAuth(user))
    }
}

/// Read the session cookie and verify the JWT against the configured secret.
/// Returns `None` for any failure (missing cookie, malformed, expired,
/// bad signature, bad sub UUID) — never panics, never errors.
async fn read_session(parts: &mut Parts, state: &AppState) -> Option<CurrentUser> {
    // 1. Find the sa_jwt cookie.
    let cookie_header = parts
        .headers
        .get(header::COOKIE)
        .and_then(|v| v.to_str().ok())?;
    let token = parse_cookie(cookie_header, COOKIE_NAME)?;

    // 2. Verify the JWT.
    let service = build_account_service(state).ok()?;
    let claims = service.jwt().verify(token).ok()?;
    let account_id = Uuid::parse_str(&claims.sub).ok()?;
    Some(CurrentUser {
        account_id,
        tier: claims.tier,
    })
}

fn parse_cookie<'a>(cookie_header: &'a str, name: &str) -> Option<&'a str> {
    for part in cookie_header.split(';') {
        let part = part.trim();
        if let Some(rest) = part.strip_prefix(&format!("{name}=")) {
            return Some(rest);
        }
    }
    None
}

fn build_account_service(state: &AppState) -> anyhow::Result<crate::account::service::AccountService> {
    crate::account::service::AccountService::new(
        crate::account::repo::AccountRepo::new(state.db.clone()),
        state.keys.clone(),
        &state.config.auth,
    )
}

/// Build the `Set-Cookie` value for a fresh login. HttpOnly + SameSite=Lax
/// to mitigate XSS exfiltration + basic CSRF defense. `Secure` is left off
/// in dev so http://localhost works; production deployments should set
/// `WEB__COOKIE_SECURE=1` to add the Secure flag.
pub fn build_session_cookie(token: &str, max_age_secs: i64, secure: bool) -> String {
    let mut s = format!(
        "{name}={token}; Path=/; HttpOnly; SameSite=Lax; Max-Age={age}",
        name = COOKIE_NAME,
        token = token,
        age = max_age_secs,
    );
    if secure {
        s.push_str("; Secure");
    }
    s
}

/// Build the `Set-Cookie` value that clears the session.
pub fn build_clear_cookie(secure: bool) -> String {
    let mut s = format!(
        "{name}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0",
        name = COOKIE_NAME
    );
    if secure {
        s.push_str("; Secure");
    }
    s
}
