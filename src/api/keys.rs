//! The keys API: custom keys with permissions, managed with the master key.

use super::{authorize, err, iso, parse_body, ApiError, ApiResult, Client, St};
use crate::auth::{self, KeyOptions, Op};
use crate::meta::ApiKey;
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;
use serde_json::{json, Value};
use std::collections::HashMap;

/// The wire form of a key. The secret appears only when it is known: at creation, and for
/// master keys.
pub fn key_json(k: &ApiKey, secret: Option<&str>) -> Value {
    json!({
        "id": k.id,
        "key": secret.or(k.secret.as_deref()),
        "key_hint": k.hint,
        "name": k.name,
        "project_id": k.project_id,
        "is_active": k.is_active,
        "master": k.master,
        "permitted": k.permitted,
        "options": k.options,
        "created_date": iso(k.created_at),
        "last_used_date": k.last_used_at.map(iso),
    })
}

/// Finds a key by id or by its secret.
fn find(st: &St, pid: &str, key: &str) -> ApiResult<ApiKey> {
    let k = if key.starts_with(auth::KEY_PREFIX) { st.meta.key_by_secret(pid, key)? } else { st.meta.key(pid, key)? };
    k.ok_or_else(|| err(StatusCode::NOT_FOUND, "ResourceNotFoundError", "key not found"))
}

fn spec_from(body: &Value, base: Option<&ApiKey>) -> ApiResult<(String, bool, Vec<String>, KeyOptions)> {
    let bad = |m: String| err(StatusCode::BAD_REQUEST, "InvalidParameterError", m);
    let name = match body.get("name") {
        Some(Value::String(s)) if !s.trim().is_empty() => s.trim().to_string(),
        None => base.map(|b| b.name.clone()).ok_or_else(|| bad("name is required".into()))?,
        _ => return Err(bad("name must be a non-empty string".into())),
    };
    let active = match body.get("is_active") {
        Some(Value::Bool(b)) => *b,
        None => base.is_none_or(|b| b.is_active),
        _ => return Err(bad("is_active must be a boolean".into())),
    };
    let permitted: Vec<String> = match body.get("permitted") {
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| bad(format!("permitted must be a list of strings: {e}")))?,
        None => base.map(|b| b.permitted.clone()).unwrap_or_default(),
    };
    let options: KeyOptions = match body.get("options") {
        Some(Value::Null) | None => base.map(|b| b.options.clone()).unwrap_or_default(),
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| bad(format!("invalid options: {e}")))?,
    };
    auth::validate_key_spec(&permitted, &options).map_err(bad)?;
    if permitted.is_empty() {
        return Err(bad(format!("permitted must list at least one of {}", auth::PERMISSIONS.join(", "))));
    }
    Ok((name, active, permitted, options))
}

pub async fn list(State(st): State<St>, Path(pid): Path<String>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    authorize(&st, &pid, &c.headers, &params, Op::Keys)?;
    let mut keys: Vec<ApiKey> = st.meta.keys(&pid)?;
    if let Some(n) = params.get("name") {
        keys.retain(|k| &k.name == n);
    }
    Ok(Json(Value::Array(keys.iter().map(|k| key_json(k, None)).collect())))
}

pub async fn create(State(st): State<St>, Path(pid): Path<String>, Query(params): Query<HashMap<String, String>>, c: Client, body: Bytes) -> ApiResult<Response> {
    authorize(&st, &pid, &c.headers, &params, Op::Keys)?;
    let (name, active, permitted, options) = spec_from(&parse_body(&body)?, None)?;
    let (mut k, secret) = st.meta.create_key(&pid, &name, false, permitted, options)?;
    if !active {
        k.is_active = false;
        st.meta.update_key(&k)?;
    }
    Ok((StatusCode::CREATED, Json(key_json(&k, Some(&secret)))).into_response())
}

pub async fn get_one(State(st): State<St>, Path((pid, key)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    authorize(&st, &pid, &c.headers, &params, Op::Keys)?;
    Ok(Json(key_json(&find(&st, &pid, &key)?, None)))
}

fn no_master_edits(k: &ApiKey) -> ApiResult<()> {
    if k.master {
        return Err(err(StatusCode::BAD_REQUEST, "InvalidParameterError", "master keys cannot be changed through the API"));
    }
    Ok(())
}

pub async fn update(State(st): State<St>, Path((pid, key)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, c: Client, body: Bytes) -> ApiResult<Json<Value>> {
    authorize(&st, &pid, &c.headers, &params, Op::Keys)?;
    let mut k = find(&st, &pid, &key)?;
    no_master_edits(&k)?;
    let (name, active, permitted, options) = spec_from(&parse_body(&body)?, Some(&k))?;
    k.name = name;
    k.is_active = active;
    k.permitted = permitted;
    k.options = options;
    st.meta.update_key(&k)?;
    Ok(Json(key_json(&k, None)))
}

async fn set_active(st: St, pid: String, key: String, params: HashMap<String, String>, c: Client, active: bool) -> ApiResult<Json<Value>> {
    authorize(&st, &pid, &c.headers, &params, Op::Keys)?;
    let mut k = find(&st, &pid, &key)?;
    no_master_edits(&k)?;
    k.is_active = active;
    st.meta.update_key(&k)?;
    Ok(Json(key_json(&k, None)))
}

pub async fn revoke(State(st): State<St>, Path((pid, key)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    set_active(st, pid, key, params, c, false).await
}

pub async fn unrevoke(State(st): State<St>, Path((pid, key)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    set_active(st, pid, key, params, c, true).await
}

pub async fn delete(State(st): State<St>, Path((pid, key)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, c: Client) -> Result<StatusCode, ApiError> {
    authorize(&st, &pid, &c.headers, &params, Op::Keys)?;
    let k = find(&st, &pid, &key)?;
    no_master_edits(&k)?;
    st.meta.delete_key(&pid, &k.id)?;
    Ok(StatusCode::NO_CONTENT)
}
