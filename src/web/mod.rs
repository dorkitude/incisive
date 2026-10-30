//! The web app: email sign-in, projects, API and account keys, install snippets, schema docs, dashboard.

pub mod dashboard;
mod explore;
pub mod mail;
mod pages;

use crate::api::St;
use crate::auth;
use crate::meta::User;
use crate::query::timeframe::{parse_timezone, Zone};
use axum::extract::{FromRequestParts, Path, State};
use axum::http::{header, request::Parts, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use parking_lot::Mutex;
use std::collections::HashMap;

pub const SESSION_COOKIE: &str = "incisive_session";
pub const TZ_COOKIE: &str = "incisive_tz";
/// The project the user last opened, so `/app` returns to it.
pub const PROJECT_COOKIE: &str = "incisive_project";
pub const TRACKER_JS: &str = include_str!("tracker.js");
pub const STYLE_CSS: &str = include_str!("style.css");

pub fn router(st: St) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/login", get(login_page).post(login_submit))
        .route("/login/code", post(code_submit))
        .route("/logout", post(logout))
        .route("/app", get(app_home))
        .route("/app/projects", post(create_project))
        .route("/app/p/{pid}", get(pages::dashboard))
        .route("/app/p/{pid}/explore", get(explore::builder))
        .route("/app/p/{pid}/explore/save", post(explore::save_query))
        .route("/app/p/{pid}/stream", get(explore::stream_page))
        .route("/app/p/{pid}/collections", get(explore::collections_page))
        .route("/app/p/{pid}/pivot", get(explore::pivot_page))
        .route("/app/p/{pid}/funnels", get(explore::funnels_page))
        .route("/app/p/{pid}/keys", get(pages::keys_page).post(pages::create_key))
        .route("/app/p/{pid}/keys/{id}/{action}", post(pages::key_action))
        .route("/app/p/{pid}/install", get(pages::install_page).post(pages::mint_write_key))
        .route("/app/p/{pid}/schema", get(pages::schema_page))
        .route("/app/p/{pid}/settings", get(pages::settings_page).post(pages::rename_project))
        .route("/app/p/{pid}/account", get(pages::account_page).post(pages::create_account_key))
        .route("/app/p/{pid}/account/{id}/delete", post(pages::delete_account_key))
        .route("/js/incisive.js", get(tracker_js))
        .route("/app/ignore", post(ignore_me))
        .route("/static/style.css", get(style_css))
        .route("/static/brand/{file}", get(brand))
        .route("/favicon.ico", get(|| brand(Path("incisive-icon.svg".to_string()))))
        .with_state(st)
}

async fn tracker_js() -> Response {
    ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=3600")], TRACKER_JS).into_response()
}

/// The "Ignore my traffic" checkbox: on ignores this address, off stops ignoring it.
async fn ignore_me(State(st): State<St>, u: WebUser, Form(form): Form<HashMap<String, String>>) -> Response {
    if let Err(r) = u.check_csrf(&form) {
        return r;
    }
    let Some(ip) = &u.ip else { return StatusCode::NO_CONTENT.into_response() };
    let r = if form.get("on").map(String::as_str) == Some("1") { st.meta.ignore_ip(&u.user.id, ip) } else { st.meta.unignore_ip(&u.user.id, ip) };
    match r {
        Ok(()) => StatusCode::NO_CONTENT.into_response(),
        Err(e) => internal(e),
    }
}

/// Logo and icon files from `src/web/brand/`.
const BRAND: [(&str, &str); 6] = [
    ("favicon.svg", include_str!("brand/favicon.svg")),
    ("incisive-icon.svg", include_str!("brand/incisive-icon.svg")),
    ("incisive-icon-dark.svg", include_str!("brand/incisive-icon-dark.svg")),
    ("incisive-app-icon.svg", include_str!("brand/incisive-app-icon.svg")),
    ("incisive-logo.svg", include_str!("brand/incisive-logo.svg")),
    ("incisive-logo-dark.svg", include_str!("brand/incisive-logo-dark.svg")),
];

async fn brand(Path(file): Path<String>) -> Response {
    match BRAND.iter().find(|(name, _)| *name == file) {
        Some((_, svg)) => ([(header::CONTENT_TYPE, "image/svg+xml"), (header::CACHE_CONTROL, "public, max-age=86400")], *svg).into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

async fn style_css() -> Response {
    ([(header::CONTENT_TYPE, "text/css; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=300")], STYLE_CSS).into_response()
}

pub fn cookie(headers: &HeaderMap, name: &str) -> Option<String> {
    headers.get_all(header::COOKIE).iter().filter_map(|v| v.to_str().ok()).flat_map(|v| v.split(';')).find_map(|kv| {
        let (k, v) = kv.trim().split_once('=')?;
        (k == name).then(|| percent_encoding::percent_decode_str(v).decode_utf8_lossy().into_owned())
    })
}

/// A signed-in user.
pub struct WebUser {
    pub user: User,
    pub csrf: String,
    /// The client address, for "Ignore my traffic".
    pub ip: Option<String>,
    /// The browser's timezone (from the `incisive_tz` cookie), or UTC until it is known.
    pub tz: String,
    pub zone: Zone,
}

/// Reads the timezone cookie, falling back to UTC when it is missing or not a zone we know.
fn browser_zone(headers: &HeaderMap) -> (String, Zone) {
    cookie(headers, TZ_COOKIE)
        .and_then(|name| parse_timezone(Some(&serde_json::Value::String(name.clone()))).ok().map(|z| (name, z)))
        .unwrap_or_else(|| ("UTC".into(), Zone::utc()))
}

pub fn csrf_for(session: &str) -> String {
    auth::sha256_hex(&format!("{session}:csrf"))[..32].to_string()
}

impl FromRequestParts<St> for WebUser {
    type Rejection = Response;

    async fn from_request_parts(parts: &mut Parts, st: &St) -> Result<Self, Self::Rejection> {
        let login = || Redirect::to("/login").into_response();
        let Some(token) = cookie(&parts.headers, SESSION_COOKIE) else { return Err(login()) };
        match st.meta.session_user(&token) {
            Ok(Some(user)) => {
                let (tz, zone) = browser_zone(&parts.headers);
                Ok(WebUser { user, csrf: csrf_for(&token), ip: crate::api::client_of(parts, st.cfg.server.trust_forwarded_for).ip, tz, zone })
            }
            Ok(None) => Err(login()),
            Err(e) => Err(crate::api::ApiError::from(e).into_response()),
        }
    }
}

impl WebUser {
    pub fn check_csrf(&self, form: &HashMap<String, String>) -> Result<(), Response> {
        if form.get("csrf").map(String::as_str) == Some(self.csrf.as_str()) {
            Ok(())
        } else {
            Err((StatusCode::FORBIDDEN, "invalid form token; reload the page and try again").into_response())
        }
    }

    /// Loads a project the user belongs to.
    pub fn project(&self, st: &St, pid: &str) -> Result<crate::meta::Project, Response> {
        let not_found = || (StatusCode::NOT_FOUND, Html(pages::message_page("Not found", "That project does not exist or you are not a member."))).into_response();
        match st.meta.is_member(pid, &self.user.id) {
            Ok(true) => {}
            _ => return Err(not_found()),
        }
        st.meta.project(pid).ok().flatten().ok_or_else(not_found)
    }
}

pub fn internal(e: anyhow::Error) -> Response {
    if e.is::<crate::meta::SignupNotAllowed>() {
        return (StatusCode::FORBIDDEN, Html(pages::login_page(Some(&e.to_string())))).into_response();
    }
    crate::api::ApiError::from(e).into_response()
}

async fn home(State(st): State<St>, headers: HeaderMap) -> Response {
    if let Some(t) = cookie(&headers, SESSION_COOKIE)
        && let Ok(Some(_)) = st.meta.session_user(&t) {
            return Redirect::to("/app").into_response();
        }
    Html(pages::login_page(None)).into_response()
}

async fn login_page() -> Html<String> {
    Html(pages::login_page(None))
}

static LOGIN_ATTEMPTS: once_cell::sync::Lazy<Mutex<HashMap<String, Vec<i64>>>> = once_cell::sync::Lazy::new(Default::default);

/// At most 5 sign-in emails per address per 15 minutes.
fn login_allowed(email: &str) -> bool {
    let now = chrono::Utc::now().timestamp();
    let mut m = LOGIN_ATTEMPTS.lock();
    if m.len() > 10_000 {
        m.clear();
    }
    let v = m.entry(email.to_string()).or_default();
    v.retain(|t| now - t < 15 * 60);
    if v.len() >= 5 {
        return false;
    }
    v.push(now);
    true
}

async fn login_submit(State(st): State<St>, Form(form): Form<HashMap<String, String>>) -> Response {
    let email = form.get("email").map(|s| s.trim().to_lowercase()).unwrap_or_default();
    if !crate::meta::valid_email(&email) {
        return Html(pages::login_page(Some("Please enter a valid email address."))).into_response();
    }
    if !login_allowed(&email) {
        return Html(pages::login_page(Some("Too many codes requested for this address. Please wait a few minutes."))).into_response();
    }
    let code = match st.meta.create_login_code(&email) {
        Ok(c) => c,
        Err(e) => return internal(e),
    };
    // A short, plain message with no links or buttons, which mail filters treat as transactional.
    let text = format!("Your Incisive sign-in code is {code}\n\nEnter it on the sign-in page. It expires in 15 minutes and works once. If you did not ask for it, you can ignore this email.\n");
    let html = pages::code_email_html(&code);
    if let Err(e) = st.mailer.send(&email, &format!("{code} is your Incisive sign-in code"), &text, &html).await {
        tracing::error!(error = %e, "sending sign-in email failed");
        return Html(pages::login_page(Some("We could not send the email just now. Please try again in a minute."))).into_response();
    }
    Html(pages::code_page(&email, None)).into_response()
}

async fn code_submit(State(st): State<St>, Form(form): Form<HashMap<String, String>>) -> Response {
    let email = form.get("email").map(|s| s.trim().to_lowercase()).unwrap_or_default();
    let code: String = form.get("code").map(|s| s.chars().filter(char::is_ascii_digit).collect()).unwrap_or_default();
    match st.meta.consume_login_code(&email, &code) {
        Ok(true) => {}
        Ok(false) => return Html(pages::code_page(&email, Some("That code is wrong or has expired. Check the latest email, or request a new code."))).into_response(),
        Err(e) => return internal(e),
    }
    let result = (|| -> anyhow::Result<String> {
        let (user, created) = st.meta.ensure_user(&email)?;
        if created || st.meta.projects_for(&user.id)?.is_empty() {
            st.meta.create_project(Some(&user.id), "My website")?;
        }
        st.meta.create_session(&user.id)
    })();
    match result {
        Ok(session) => {
            let secure = if st.cfg.server.secure_cookies { "; Secure" } else { "" };
            let c = format!("{SESSION_COOKIE}={session}; Path=/; HttpOnly; SameSite=Lax; Max-Age={}{secure}", crate::meta::SESSION_TTL_SECS);
            ([(header::SET_COOKIE, c)], Redirect::to("/app")).into_response()
        }
        Err(e) => internal(e),
    }
}

async fn logout(State(st): State<St>, headers: HeaderMap) -> Response {
    if let Some(t) = cookie(&headers, SESSION_COOKIE) {
        let _ = st.meta.delete_session(&t);
    }
    let c = format!("{SESSION_COOKIE}=; Path=/; HttpOnly; SameSite=Lax; Max-Age=0");
    ([(header::SET_COOKIE, c)], Redirect::to("/login")).into_response()
}

async fn app_home(State(st): State<St>, u: WebUser, headers: HeaderMap) -> Response {
    let projects = match st.meta.projects_for(&u.user.id) {
        Ok(p) => p,
        Err(e) => return internal(e),
    };
    // The last project opened, else the newest one.
    let last = cookie(&headers, PROJECT_COOKIE);
    let pick = projects.iter().find(|p| Some(&p.id) == last.as_ref()).or(projects.last());
    let pid = match pick {
        Some(p) => p.id.clone(),
        None => match st.meta.create_project(Some(&u.user.id), "My website") {
            Ok((p, _)) => p.id,
            Err(e) => return internal(e),
        },
    };
    Redirect::to(&format!("/app/p/{pid}")).into_response()
}

async fn create_project(State(st): State<St>, u: WebUser, Form(form): Form<HashMap<String, String>>) -> Response {
    if let Err(r) = u.check_csrf(&form) {
        return r;
    }
    let name = form.get("name").map(|s| s.trim()).filter(|s| !s.is_empty()).unwrap_or("Untitled project");
    match st.meta.create_project(Some(&u.user.id), &name.chars().take(80).collect::<String>()) {
        Ok((p, _)) => Redirect::to(&format!("/app/p/{}/install", p.id)).into_response(),
        Err(e) => internal(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_zone_falls_back_to_utc() {
        let with = |c: &str| {
            let mut h = HeaderMap::new();
            h.insert(header::COOKIE, c.parse().unwrap());
            browser_zone(&h).0
        };
        assert_eq!(with("incisive_tz=America%2FChicago"), "America/Chicago");
        assert_eq!(with("incisive_tz=Mars%2FOlympus"), "UTC");
        assert_eq!(browser_zone(&HeaderMap::new()).0, "UTC");
    }
}
