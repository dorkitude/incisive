//! The account API: projects and account master keys, managed with an account master key.

use super::keys::key_json;
use super::{credential, err, iso, parse_body, ApiResult, Client, St};
use crate::meta::{AccountKey, Project, User};
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;

/// Authenticates a request made with an account master key.
fn authorize_account(st: &St, c: &Client, params: &HashMap<String, String>) -> ApiResult<User> {
    let Some(cred) = credential(&c.headers, params) else {
        return Err(err(StatusCode::UNAUTHORIZED, "InvalidApiKeyError", "an account master key is required (Authorization header or api_key parameter)"));
    };
    match st.meta.authenticate_account(&cred)? {
        Some((u, _)) => Ok(u),
        None => Err(err(StatusCode::UNAUTHORIZED, "InvalidApiKeyError", "this route needs a valid account master key (ak_incisive_…)")),
    }
}

fn project_json(st: &St, p: &Project) -> Value {
    let base = format!("{}/1.0/projects/{}", st.cfg.server.public_url.trim_end_matches('/'), p.id);
    json!({"id": p.id, "name": p.name, "created_date": iso(p.created_at), "api_url": base})
}

fn account_key_json(k: &AccountKey, secret: Option<&str>) -> Value {
    json!({
        "id": k.id,
        "key": secret,
        "key_hint": k.hint,
        "name": k.name,
        "created_date": iso(k.created_at),
        "last_used_date": k.last_used_at.map(iso),
    })
}

fn name_from(body: &Value) -> ApiResult<String> {
    match body.get("name") {
        Some(Value::String(s)) if !s.trim().is_empty() => Ok(s.trim().chars().take(80).collect()),
        _ => Err(err(StatusCode::BAD_REQUEST, "InvalidParameterError", "name must be a non-empty string")),
    }
}

pub async fn info(State(st): State<St>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    let u = authorize_account(&st, &c, &params)?;
    let projects: Vec<Value> = st.meta.projects_for(&u.id)?.iter().map(|p| project_json(&st, p)).collect();
    Ok(Json(json!({"email": u.email, "projects": projects})))
}

pub async fn list_projects(State(st): State<St>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    let u = authorize_account(&st, &c, &params)?;
    Ok(Json(Value::Array(st.meta.projects_for(&u.id)?.iter().map(|p| project_json(&st, p)).collect())))
}

/// Creates a project with its master, read and write keys, returning their secrets.
pub async fn create_project(State(st): State<St>, Query(params): Query<HashMap<String, String>>, c: Client, body: Bytes) -> ApiResult<Response> {
    let u = authorize_account(&st, &c, &params)?;
    let name = name_from(&parse_body(&body)?)?;
    let (p, keys) = st.meta.create_project(Some(&u.id), &name)?;
    let mut out = project_json(&st, &p);
    out["keys"] = Value::Array(keys.iter().map(|(k, s)| key_json(k, Some(s))).collect());
    Ok((StatusCode::CREATED, Json(out)).into_response())
}

pub async fn list_keys(State(st): State<St>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    let u = authorize_account(&st, &c, &params)?;
    Ok(Json(Value::Array(st.meta.account_keys(&u.id)?.iter().map(|k| account_key_json(k, None)).collect())))
}

pub async fn create_key(State(st): State<St>, Query(params): Query<HashMap<String, String>>, c: Client, body: Bytes) -> ApiResult<Response> {
    let u = authorize_account(&st, &c, &params)?;
    let name = name_from(&parse_body(&body)?)?;
    let (k, secret) = st.meta.create_account_key(&u.id, &name)?;
    Ok((StatusCode::CREATED, Json(account_key_json(&k, Some(&secret)))).into_response())
}

pub async fn delete_key(State(st): State<St>, Path(id): Path<String>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<StatusCode> {
    let u = authorize_account(&st, &c, &params)?;
    if !st.meta.delete_account_key(&u.id, &id)? {
        return Err(err(StatusCode::NOT_FOUND, "ResourceNotFoundError", "account key not found"));
    }
    Ok(StatusCode::NO_CONTENT)
}
