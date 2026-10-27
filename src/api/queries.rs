//! Analysis queries and saved queries.

use super::{authorize, err, iso, parse_body, ApiError, ApiResult, Client, St};
use crate::auth::{Access, Op};
use crate::query::exec::Executor;
use crate::query::{self, Query as Q, ANALYSES};
use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::{StatusCode, Uri};
use axum::response::{IntoResponse, Response};
use axum::Json;
use chrono::Utc;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::time::Instant;

fn pairs(uri: &Uri) -> Vec<(String, String)> {
    url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()).into_owned().collect()
}

pub async fn list_analyses(State(st): State<St>, Path(pid): Path<String>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    authorize(&st, &pid, &c.headers, &params, Op::Query)?;
    let out: Vec<Value> = ANALYSES.iter().map(|a| json!({"name": a, "url": format!("/3.0/projects/{pid}/queries/{a}")})).collect();
    Ok(Json(Value::Array(out)))
}

/// Runs a parsed query with the key's restrictions, a concurrency slot and a timeout.
async fn execute(st: &St, access: &Access, q: &Q) -> ApiResult<Map<String, Value>> {
    if !st.allow(&access.project_id, "query", 1.0) {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, "TooManyRequestsError", "query rate limit exceeded; retry shortly"));
    }
    let _slot = st.query_slots.acquire().await.map_err(|_| err(StatusCode::SERVICE_UNAVAILABLE, "ServiceUnavailableError", "shutting down"))?;
    let started = Instant::now();
    let ex = Executor::new(&st.engine, st.query_limits(), Utc::now());
    let scope = access.query_filters();
    let timeout = std::time::Duration::from_secs(st.cfg.limits.query_timeout_secs.max(1));
    st.metrics.queries.inc();
    let mut out = match tokio::time::timeout(timeout, ex.run(&access.project_id, q, &scope)).await {
        Ok(Ok(out)) => out,
        Ok(Err(e)) => {
            st.metrics.query_errors.inc();
            return Err(e.into());
        }
        Err(_) => {
            st.metrics.query_errors.inc();
            return Err(err(StatusCode::GATEWAY_TIMEOUT, "QueryTimeoutError", "the query took too long; narrow the timeframe or add filters"));
        }
    };
    if q.include_metadata {
        out.insert(
            "execution_metadata".into(),
            json!({
                "total_processing_time": started.elapsed().as_secs_f64(),
                "events_scanned": ex.scanned.load(std::sync::atomic::Ordering::Relaxed),
                "properties_per_event": 0,
                "total_properties_scanned": 0,
            }),
        );
    }
    Ok(out)
}

fn check_analysis(analysis: &str) -> ApiResult<()> {
    if !ANALYSES.contains(&analysis) {
        return Err(err(StatusCode::NOT_FOUND, "ResourceNotFoundError", format!("unknown analysis type {analysis:?}")));
    }
    Ok(())
}

pub async fn run_get(State(st): State<St>, Path((pid, analysis)): Path<(String, String)>, uri: Uri, c: Client) -> ApiResult<Json<Value>> {
    check_analysis(&analysis)?;
    let ps = pairs(&uri);
    let params: HashMap<String, String> = ps.iter().cloned().collect();
    let access = authorize(&st, &pid, &c.headers, &params, Op::Query)?;
    let m = query::from_query_pairs(ps.into_iter());
    let q = query::parse(&analysis, &m)?;
    Ok(Json(Value::Object(execute(&st, &access, &q).await?)))
}

pub async fn run_post(State(st): State<St>, Path((pid, analysis)): Path<(String, String)>, uri: Uri, c: Client, body: Bytes) -> ApiResult<Json<Value>> {
    check_analysis(&analysis)?;
    let ps = pairs(&uri);
    let params: HashMap<String, String> = ps.iter().cloned().collect();
    let access = authorize(&st, &pid, &c.headers, &params, Op::Query)?;
    // Query-string parameters apply too; the body wins on conflicts.
    let mut m = query::from_query_pairs(ps.into_iter());
    if !body.is_empty() {
        match parse_body(&body)? {
            Value::Object(b) => m.extend(b),
            _ => return Err(err(StatusCode::BAD_REQUEST, "InvalidJsonError", "the query body must be a JSON object")),
        }
    }
    let q = query::parse(&analysis, &m)?;
    Ok(Json(Value::Object(execute(&st, &access, &q).await?)))
}

// ---- saved queries ----

fn saved_json(pid: &str, sq: &crate::meta::SavedQuery, kind: &str) -> Value {
    let mut v = Map::new();
    v.insert("query_name".into(), json!(sq.name));
    for k in ["query", "metadata", "refresh_rate"] {
        v.insert(k.into(), sq.body.get(k).cloned().unwrap_or(match k {
            "refresh_rate" => json!(0),
            "metadata" => json!({}),
            _ => Value::Null,
        }));
    }
    v.insert("created_date".into(), json!(iso(sq.created_at)));
    v.insert("last_modified_date".into(), json!(iso(sq.updated_at)));
    v.insert("run_information".into(), Value::Null);
    let base = format!("/3.0/projects/{pid}/queries/{kind}/{}", sq.name);
    v.insert("urls".into(), json!({"self": base, "result": format!("{base}/result")}));
    Value::Object(v)
}

fn kind_of(uri: &Uri) -> &'static str {
    if uri.path().contains("/queries/cached") {
        "cached"
    } else {
        "saved"
    }
}

fn not_found(name: &str) -> ApiError {
    err(StatusCode::NOT_FOUND, "ResourceNotFoundError", format!("saved query {name:?} not found"))
}

pub async fn list_saved(State(st): State<St>, Path(pid): Path<String>, Query(params): Query<HashMap<String, String>>, uri: Uri, c: Client) -> ApiResult<Json<Value>> {
    let access = authorize(&st, &pid, &c.headers, &params, Op::SavedRead)?;
    let kind = kind_of(&uri);
    let out: Vec<Value> = st.meta.saved_queries(&pid)?.iter().filter(|s| access.can_read_saved(&s.name)).map(|s| saved_json(&pid, s, kind)).collect();
    Ok(Json(Value::Array(out)))
}

pub async fn get_saved(State(st): State<St>, Path((pid, name)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, uri: Uri, c: Client) -> ApiResult<Json<Value>> {
    let access = authorize(&st, &pid, &c.headers, &params, Op::SavedRead)?;
    if !access.can_read_saved(&name) {
        return Err(not_found(&name));
    }
    let sq = st.meta.saved_query(&pid, &name)?.ok_or_else(|| not_found(&name))?;
    Ok(Json(saved_json(&pid, &sq, kind_of(&uri))))
}

fn valid_name(name: &str) -> bool {
    !name.is_empty() && name.len() <= 200 && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.')
}

/// Parses the stored query of a saved query body.
fn saved_query_of(body: &Value) -> ApiResult<Q> {
    let Some(Value::Object(q)) = body.get("query") else {
        return Err(err(StatusCode::BAD_REQUEST, "InvalidParameterError", "a saved query needs a \"query\" object"));
    };
    let analysis = q.get("analysis_type").and_then(Value::as_str).ok_or_else(|| err(StatusCode::BAD_REQUEST, "InvalidParameterError", "query.analysis_type is required"))?;
    check_analysis(analysis)?;
    Ok(query::parse(analysis, q)?)
}

pub async fn put_saved(State(st): State<St>, Path((pid, name)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, uri: Uri, c: Client, body: Bytes) -> ApiResult<Response> {
    authorize(&st, &pid, &c.headers, &params, Op::SavedManage)?;
    if !valid_name(&name) {
        return Err(err(StatusCode::BAD_REQUEST, "InvalidParameterError", "saved query names may contain letters, digits, '-', '_' and '.'"));
    }
    let mut b = parse_body(&body)?;
    // An update may omit the query to change only metadata or refresh_rate.
    if b.get("query").is_none()
        && let Some(old) = st.meta.saved_query(&pid, &name)?
            && let (Value::Object(bm), Some(q)) = (&mut b, old.body.get("query")) {
                bm.insert("query".into(), q.clone());
            }
    let q = saved_query_of(&b)?;
    let mut stored = Map::new();
    stored.insert("query".into(), q.echo());
    stored.insert("metadata".into(), b.get("metadata").cloned().unwrap_or(json!({})));
    stored.insert("refresh_rate".into(), b.get("refresh_rate").cloned().unwrap_or(json!(0)));
    let created = st.meta.put_saved_query(&pid, &name, &Value::Object(stored))?;
    let sq = st.meta.saved_query(&pid, &name)?.ok_or_else(|| not_found(&name))?;
    let status = if created { StatusCode::CREATED } else { StatusCode::OK };
    Ok((status, Json(saved_json(&pid, &sq, kind_of(&uri)))).into_response())
}

pub async fn delete_saved(State(st): State<St>, Path((pid, name)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<StatusCode> {
    authorize(&st, &pid, &c.headers, &params, Op::SavedManage)?;
    if !st.meta.delete_saved_query(&pid, &name)? {
        return Err(not_found(&name));
    }
    Ok(StatusCode::NO_CONTENT)
}

pub async fn saved_result(State(st): State<St>, Path((pid, name)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, uri: Uri, c: Client) -> ApiResult<Json<Value>> {
    let access = authorize(&st, &pid, &c.headers, &params, Op::SavedRead)?;
    if !access.can_read_saved(&name) {
        return Err(not_found(&name));
    }
    let sq = st.meta.saved_query(&pid, &name)?.ok_or_else(|| not_found(&name))?;
    let q = saved_query_of(&sq.body)?;
    let mut out = execute(&st, &access, &q).await?;
    let mut v = saved_json(&pid, &sq, kind_of(&uri));
    let result = if q.analysis_type == "funnel" {
        // Saved funnels nest the steps with the result.
        let mut r = Map::new();
        r.insert("result".into(), out.remove("result").unwrap_or(Value::Null));
        r.insert("steps".into(), out.remove("steps").unwrap_or(Value::Null));
        if let Some(a) = out.remove("actors") {
            r.insert("actors".into(), a);
        }
        Value::Object(r)
    } else {
        out.remove("result").unwrap_or(Value::Null)
    };
    v.as_object_mut().unwrap().insert("result".into(), result);
    Ok(Json(v))
}
