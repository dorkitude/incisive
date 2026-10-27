//! The web app: email sign-in, projects, API keys, install snippets, schema docs, dashboard.

pub mod dashboard;
pub mod mail;
mod pages;

use crate::api::St;
use crate::auth;
use crate::meta::User;
use axum::extract::{FromRequestParts, Query, State};
use axum::http::{header, request::Parts, HeaderMap, StatusCode};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::routing::{get, post};
use axum::{Form, Router};
use parking_lot::Mutex;
use std::collections::HashMap;

pub const SESSION_COOKIE: &str = "incisive_session";
pub const TZ_COOKIE: &str = "incisive_tz";
pub const TRACKER_JS: &str = include_str!("tracker.js");
pub const STYLE_CSS: &str = include_str!("style.css");

pub fn router(st: St) -> Router {
    Router::new()
        .route("/", get(home))
        .route("/login", get(login_page).post(login_submit))
        .route("/auth/verify", get(verify_page).post(verify_submit))
        .route("/logout", post(logout))
        .route("/app", get(app_home))
        .route("/app/projects", post(create_project))
        .route("/app/p/{pid}", get(pages::dashboard))
        .route("/app/p/{pid}/keys", get(pages::keys_page).post(pages::create_key))
        .route("/app/p/{pid}/keys/{id}/{action}", post(pages::key_action))
        .route("/app/p/{pid}/install", get(pages::install_page).post(pages::mint_write_key))
        .route("/app/p/{pid}/schema", get(pages::schema_page))
        .route("/app/p/{pid}/settings", get(pages::settings_page).post(pages::rename_project))
        .route("/js/incisive.js", get(tracker_js))
        .route("/static/style.css", get(style_css))
        .route("/favicon.ico", get(|| async { StatusCode::NO_CONTENT }))
        .with_state(st)
}

async fn tracker_js() -> Response {
    ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8"), (header::CACHE_CONTROL, "public, max-age=3600")], TRACKER_JS).into_response()
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
            Ok(Some(user)) => Ok(WebUser { user, csrf: csrf_for(&token) }),
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

fn valid_email(e: &str) -> bool {
    let Some((local, domain)) = e.split_once('@') else { return false };
    !local.is_empty() && domain.contains('.') && !domain.starts_with('.') && !domain.ends_with('.') && e.len() <= 254 && !e.chars().any(|c| c.is_whitespace() || c == '<' || c == '>' || c == ',')
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
    if !valid_email(&email) {
        return Html(pages::login_page(Some("Please enter a valid email address."))).into_response();
    }
    if !login_allowed(&email) {
        return Html(pages::login_page(Some("Too many sign-in links requested for this address. Please wait a few minutes."))).into_response();
    }
    let token = match st.meta.create_login_token(&email) {
        Ok(t) => t,
        Err(e) => return internal(e),
    };
    let link = format!("{}/auth/verify?token={token}", st.cfg.server.public_url.trim_end_matches('/'));
    let text = format!("Sign in to Incisive:\n\n{link}\n\nThis link expires in 15 minutes and works once. If you did not ask for it, you can ignore this email.\n");
    let html = pages::email_html(&link);
    if let Err(e) = st.mailer.send(&email, "Your Incisive sign-in link", &text, &html).await {
        tracing::error!(error = %e, "sending sign-in email failed");
        return Html(pages::login_page(Some("We could not send the email just now. Please try again in a minute."))).into_response();
    }
    Html(pages::check_email_page(&email)).into_response()
}

/// Shows a button that signs in with a POST, so link scanners that fetch the URL do not use up
/// the one-time token. Browsers with scripts enabled continue automatically.
async fn verify_page(Query(q): Query<HashMap<String, String>>) -> Html<String> {
    Html(pages::verify_page(q.get("token").map(String::as_str).unwrap_or("")))
}

async fn verify_submit(State(st): State<St>, Form(form): Form<HashMap<String, String>>) -> Response {
    let token = form.get("token").cloned().unwrap_or_default();
    let email = match st.meta.consume_login_token(&token) {
        Ok(Some(e)) => e,
        Ok(None) => return Html(pages::login_page(Some("That sign-in link is invalid, already used, or expired. Request a new one below."))).into_response(),
        Err(e) => return internal(e),
    };
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

async fn app_home(State(st): State<St>, u: WebUser) -> Response {
    let projects = match st.meta.projects_for(&u.user.id) {
        Ok(p) => p,
        Err(e) => return internal(e),
    };
    let pid = match projects.first() {
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
