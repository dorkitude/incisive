//! The versioned HTTP API: event writes, queries, schema, deletes and keys.

mod keys;
mod queries;

use crate::auth::{Access, Op};
use crate::config::Config;
use crate::event::{Preparer, RequestCtx};
use crate::meta::Meta;
use crate::query::exec::Limits;
use crate::query::{self, QueryError};
use crate::store::{Engine, StoredEvent};
use axum::body::Bytes;
use axum::extract::{FromRequestParts, Path, Query, State};
use axum::http::{header, request::Parts, HeaderMap, HeaderValue, Method, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use base64::Engine as _;
use chrono::Utc;
use parking_lot::Mutex;
use serde_json::{json, Map, Value};
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

/// Shared server state.
pub struct AppState {
    pub cfg: Config,
    pub engine: Arc<Engine>,
    pub meta: Arc<Meta>,
    pub geo: Option<maxminddb::Reader<Vec<u8>>>,
    pub query_slots: tokio::sync::Semaphore,
    pub buckets: Mutex<HashMap<(String, &'static str), (f64, Instant)>>,
    pub metrics: crate::metrics::Metrics,
    pub mailer: crate::web::mail::Mailer,
}

pub type St = Arc<AppState>;

impl AppState {
    pub fn new(cfg: Config, engine: Arc<Engine>, meta: Arc<Meta>) -> anyhow::Result<AppState> {
        let geo = if cfg.geo.db_path.as_os_str().is_empty() {
            None
        } else {
            Some(maxminddb::Reader::open_readfile(&cfg.geo.db_path).map_err(|e| anyhow::anyhow!("opening GeoIP database {}: {e}", cfg.geo.db_path.display()))?)
        };
        Ok(AppState {
            query_slots: tokio::sync::Semaphore::new(cfg.limits.max_concurrent_queries.max(1)),
            mailer: crate::web::mail::Mailer::new(&cfg.email),
            cfg,
            engine,
            meta,
            geo,
            buckets: Mutex::default(),
            metrics: crate::metrics::Metrics::new(),
        })
    }

    pub fn preparer(&self) -> Preparer<'_> {
        Preparer { max_event_bytes: self.cfg.limits.max_event_bytes, geo: self.geo.as_ref() }
    }

    pub fn query_limits(&self) -> Limits {
        let l = &self.cfg.limits;
        Limits { max_extraction: l.max_extraction_events, max_funnel: l.max_funnel_actors, max_groups: l.max_groups, max_intervals: l.max_intervals, max_scan: l.max_scan_events }
    }

    /// Token-bucket rate limit per project and kind; returns false when over the limit.
    fn allow(&self, project: &str, kind: &'static str, cost: f64) -> bool {
        let (rate, burst) = match kind {
            "write" => (self.cfg.limits.write_events_per_second, self.cfg.limits.write_burst),
            _ => (self.cfg.limits.queries_per_second, self.cfg.limits.query_burst),
        };
        if rate <= 0.0 {
            return true;
        }
        let mut b = self.buckets.lock();
        let now = Instant::now();
        let e = b.entry((project.to_string(), kind)).or_insert((burst, now));
        e.0 = (e.0 + now.duration_since(e.1).as_secs_f64() * rate).min(burst);
        e.1 = now;
        if e.0 < cost {
            return false;
        }
        e.0 -= cost;
        true
    }
}

// ---- errors ----

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: String,
    pub message: String,
}

pub fn err(status: StatusCode, code: &str, message: impl Into<String>) -> ApiError {
    ApiError { status, code: code.into(), message: message.into() }
}

impl ApiError {
    fn body(&self) -> Value {
        json!({"message": self.message, "error_code": self.code})
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(self.body())).into_response()
    }
}

impl From<QueryError> for ApiError {
    fn from(e: QueryError) -> ApiError {
        err(StatusCode::from_u16(e.status).unwrap_or(StatusCode::BAD_REQUEST), e.code, e.message)
    }
}

impl From<anyhow::Error> for ApiError {
    fn from(e: anyhow::Error) -> ApiError {
        tracing::error!(error = %format!("{e:#}"), "internal error");
        err(StatusCode::INTERNAL_SERVER_ERROR, "InternalServerError", "internal server error")
    }
}

pub type ApiResult<T> = Result<T, ApiError>;

// ---- request context ----

/// The client address and headers of a request.
pub struct Client {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
    pub headers: HeaderMap,
}

impl FromRequestParts<St> for Client {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, st: &St) -> Result<Self, Self::Rejection> {
        Ok(client_of(parts, st.cfg.server.trust_forwarded_for))
    }
}

pub fn client_of(parts: &Parts, trust_forwarded: bool) -> Client {
    let h = &parts.headers;
    let forwarded = trust_forwarded
        .then(|| h.get("x-forwarded-for").and_then(|v| v.to_str().ok()).and_then(|v| v.split(',').next()).map(|s| s.trim().to_string()))
        .flatten()
        .filter(|s| !s.is_empty());
    let peer = parts.extensions.get::<axum::extract::ConnectInfo<SocketAddr>>().map(|c| c.0.ip().to_string());
    Client {
        ip: forwarded.or(peer),
        user_agent: h.get(header::USER_AGENT).and_then(|v| v.to_str().ok()).map(String::from),
        headers: h.clone(),
    }
}

impl Client {
    fn ctx(&self) -> RequestCtx {
        RequestCtx { ip: self.ip.clone(), user_agent: self.user_agent.clone() }
    }
}

/// The credential: the Authorization header (preferred) or the `api_key` parameter.
fn credential(headers: &HeaderMap, params: &HashMap<String, String>) -> Option<String> {
    let h = headers.get(header::AUTHORIZATION).and_then(|v| v.to_str().ok()).map(|s| s.trim().trim_start_matches("Bearer ").trim().to_string()).filter(|s| !s.is_empty());
    h.or_else(|| params.get("api_key").cloned().filter(|s| !s.is_empty()))
}

/// Authenticates a request for a project and checks the operation.
pub fn authorize(st: &AppState, pid: &str, headers: &HeaderMap, params: &HashMap<String, String>, op: Op) -> ApiResult<Access> {
    let Some(cred) = credential(headers, params) else {
        return Err(err(StatusCode::UNAUTHORIZED, "InvalidApiKeyError", "an API key is required (Authorization header or api_key parameter)"));
    };
    let Some(access) = st.meta.authenticate(pid, &cred)? else {
        return Err(err(StatusCode::UNAUTHORIZED, "InvalidApiKeyError", "the API key is not valid for this project"));
    };
    if !access.can(op) {
        return Err(err(StatusCode::UNAUTHORIZED, "InvalidApiKeyError", format!("this key is not permitted to {}", op_name(op))));
    }
    if let Some(id) = &access.key_id {
        st.meta.touch_key(id);
    }
    Ok(access)
}

fn op_name(op: Op) -> &'static str {
    match op {
        Op::Write => "write events",
        Op::Query => "run queries",
        Op::Schema => "read the schema",
        Op::SavedRead => "read saved queries",
        Op::SavedManage => "manage saved queries (master key required)",
        Op::Datasets => "use datasets",
        Op::Delete => "delete events (master key required)",
        Op::Keys => "manage keys (master key required)",
    }
}

fn parse_body(body: &Bytes) -> ApiResult<Value> {
    serde_json::from_slice(body).map_err(|e| err(StatusCode::BAD_REQUEST, "InvalidJsonError", format!("request body is not valid JSON: {e}")))
}

pub fn iso(secs: i64) -> String {
    chrono::DateTime::from_timestamp(secs, 0).map(|t| crate::event::fmt_time(&t)).unwrap_or_default()
}

// ---- router ----

pub fn router(st: St) -> Router {
    let p = "/3.0/projects/{pid}";
    let api = Router::new()
        .route(p, get(project_info))
        .route(&format!("{p}/events"), get(schema_all).post(write_batch))
        .route(&format!("{p}/events/{{coll}}"), get(get_collection).post(write_one).delete(delete_events))
        .route(&format!("{p}/events/{{coll}}/properties/{{prop}}"), get(property_info).delete(not_supported))
        .route(&format!("{p}/queries"), get(queries::list_analyses))
        .route(&format!("{p}/queries/{{analysis}}"), get(queries::run_get).post(queries::run_post))
        .route(&format!("{p}/queries/saved"), get(queries::list_saved))
        .route(&format!("{p}/queries/saved/{{name}}"), get(queries::get_saved).put(queries::put_saved).delete(queries::delete_saved))
        .route(&format!("{p}/queries/saved/{{name}}/result"), get(queries::saved_result))
        .route(&format!("{p}/queries/cached"), get(queries::list_saved))
        .route(&format!("{p}/queries/cached/{{name}}"), get(queries::get_saved).put(queries::put_saved).delete(queries::delete_saved))
        .route(&format!("{p}/queries/cached/{{name}}/result"), get(queries::saved_result))
        .route(&format!("{p}/datasets"), get(datasets).post(datasets))
        .route(&format!("{p}/datasets/{{*rest}}"), get(datasets).put(datasets).delete(datasets).post(datasets))
        .route(&format!("{p}/keys"), get(keys::list).post(keys::create))
        .route(&format!("{p}/keys/{{key}}"), get(keys::get_one).post(keys::update).delete(keys::delete))
        .route(&format!("{p}/keys/{{key}}/revoke"), post(keys::revoke))
        .route(&format!("{p}/keys/{{key}}/unrevoke"), post(keys::unrevoke))
        .layer(
            tower_http::cors::CorsLayer::new()
                .allow_origin(tower_http::cors::Any)
                .allow_methods([Method::GET, Method::POST, Method::PUT, Method::DELETE, Method::OPTIONS])
                .allow_headers(tower_http::cors::Any)
                .max_age(std::time::Duration::from_secs(86400)),
        );
    Router::new()
        .merge(api)
        .route("/health", get(health))
        .route("/metrics", get(metrics))
        .layer(axum::extract::DefaultBodyLimit::max(st.cfg.limits.max_request_bytes))
        .with_state(st)
}

async fn health(State(st): State<St>) -> Response {
    match st.engine.health().await {
        Ok(()) => Json(json!({"status": "ok", "pending_wal": st.engine.pending_wal()})).into_response(),
        Err(e) => (StatusCode::SERVICE_UNAVAILABLE, Json(json!({"status": "unavailable", "error": e.to_string()}))).into_response(),
    }
}

async fn metrics(State(st): State<St>) -> Response {
    ([(header::CONTENT_TYPE, "text/plain; version=0.0.4")], st.metrics.render(&st.engine)).into_response()
}

async fn datasets() -> ApiError {
    err(StatusCode::NOT_IMPLEMENTED, "NotImplementedError", "cached datasets are not supported; use saved queries")
}

async fn not_supported() -> ApiError {
    err(StatusCode::NOT_IMPLEMENTED, "NotImplementedError", "deleting a property is not supported")
}

async fn project_info(State(st): State<St>, Path(pid): Path<String>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    authorize(&st, &pid, &c.headers, &params, Op::Schema)?;
    let p = st.meta.project(&pid)?.ok_or_else(|| err(StatusCode::NOT_FOUND, "ResourceNotFoundError", "project not found"))?;
    let base = format!("{}/3.0/projects/{}", st.cfg.server.public_url.trim_end_matches('/'), p.id);
    Ok(Json(json!({"id": p.id, "name": p.name, "events_url": format!("{base}/events"), "queries_url": format!("{base}/queries")})))
}

// ---- writes ----

async fn write_events(st: &AppState, access: &Access, c: &Client, items: Vec<(String, Value)>) -> ApiResult<Vec<Result<(), crate::event::ValidationError>>> {
    if !st.allow(&access.project_id, "write", items.len() as f64) {
        return Err(err(StatusCode::TOO_MANY_REQUESTS, "TooManyRequestsError", "write rate limit exceeded; retry shortly"));
    }
    let prep = st.preparer();
    let ctx = c.ctx();
    let now = Utc::now();
    let mut results = Vec::with_capacity(items.len());
    let mut ok: Vec<StoredEvent> = vec![];
    for (coll, ev) in items {
        match prep.prepare(&coll, ev, access.autofill(), &ctx, now) {
            Ok(se) => {
                ok.push(se);
                results.push(Ok(()));
            }
            Err(e) => results.push(Err(e)),
        }
    }
    let n = ok.len();
    if let Err(e) = st.engine.append(&access.project_id, ok).await {
        st.metrics.write_errors.inc();
        return Err(e.into());
    }
    st.metrics.events_written.inc_by(n as u64);
    Ok(results)
}

fn validation_error(e: crate::event::ValidationError) -> ApiError {
    err(StatusCode::BAD_REQUEST, &e.name, e.description)
}

async fn write_one(State(st): State<St>, Path((pid, coll)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, c: Client, body: Bytes) -> ApiResult<Response> {
    let access = authorize(&st, &pid, &c.headers, &params, Op::Write)?;
    let ev = parse_body(&body)?;
    if let Value::Array(list) = ev {
        // An array of events for one collection: answer like a batch.
        let items: Vec<(String, Value)> = list.into_iter().map(|e| (coll.clone(), e)).collect();
        check_batch_size(&st, items.len())?;
        let res = write_events(&st, &access, &c, items).await?;
        return Ok(Json(json!({ coll: res.into_iter().map(batch_item).collect::<Vec<_>>() })).into_response());
    }
    let res = write_events(&st, &access, &c, vec![(coll, ev)]).await?;
    match res.into_iter().next().unwrap() {
        Ok(()) => Ok((StatusCode::CREATED, Json(json!({"created": true}))).into_response()),
        Err(e) => Err(validation_error(e)),
    }
}

fn check_batch_size(st: &AppState, n: usize) -> ApiResult<()> {
    if n > st.cfg.limits.max_batch_events {
        return Err(err(StatusCode::BAD_REQUEST, "BatchTooLargeError", format!("a batch may contain at most {} events", st.cfg.limits.max_batch_events)));
    }
    Ok(())
}

fn batch_item(r: Result<(), crate::event::ValidationError>) -> Value {
    match r {
        Ok(()) => json!({"success": true}),
        Err(e) => json!({"success": false, "error": {"name": e.name, "description": e.description}}),
    }
}

async fn write_batch(State(st): State<St>, Path(pid): Path<String>, Query(params): Query<HashMap<String, String>>, c: Client, body: Bytes) -> ApiResult<Json<Value>> {
    let access = authorize(&st, &pid, &c.headers, &params, Op::Write)?;
    let Value::Object(m) = parse_body(&body)? else {
        return Err(err(StatusCode::BAD_REQUEST, "InvalidBatchError", "a batch must be an object mapping collection names to lists of events"));
    };
    let mut items = vec![];
    let mut shape: Vec<(String, usize)> = vec![];
    for (coll, evs) in m {
        let Value::Array(evs) = evs else {
            return Err(err(StatusCode::BAD_REQUEST, "InvalidBatchError", format!("the value for {coll:?} must be a list of events")));
        };
        shape.push((coll.clone(), evs.len()));
        items.extend(evs.into_iter().map(|e| (coll.clone(), e)));
    }
    check_batch_size(&st, items.len())?;
    let mut res = write_events(&st, &access, &c, items).await?.into_iter();
    let mut out = Map::new();
    for (coll, n) in shape {
        out.insert(coll, Value::Array(res.by_ref().take(n).map(batch_item).collect()));
    }
    Ok(Json(Value::Object(out)))
}

/// 1x1 transparent GIF.
pub const GIF: &[u8] = b"GIF89a\x01\x00\x01\x00\x80\x00\x00\x00\x00\x00\xff\xff\xff\x21\xf9\x04\x01\x00\x00\x00\x00\x2c\x00\x00\x00\x00\x01\x00\x01\x00\x00\x02\x02\x44\x01\x00\x3b";

fn decode_data(data: &str) -> Option<Value> {
    let t = data.trim();
    if t.starts_with('{') || t.starts_with('[') {
        return serde_json::from_str(t).ok();
    }
    // Form decoding may have turned '+' into ' '.
    let t = t.replace(' ', "+");
    use base64::engine::general_purpose::{STANDARD, STANDARD_NO_PAD, URL_SAFE, URL_SAFE_NO_PAD};
    for e in [&STANDARD, &URL_SAFE, &STANDARD_NO_PAD, &URL_SAFE_NO_PAD] {
        if let Ok(b) = e.decode(&t)
            && let Ok(v) = serde_json::from_slice(&b) {
                return Some(v);
            }
    }
    None
}

fn valid_callback(cb: &str) -> bool {
    !cb.is_empty() && cb.len() <= 128 && cb.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'.' || b == b'$')
}

/// How a GET write should answer.
enum Reply {
    Json,
    Gif,
    Jsonp(String),
    Redirect(String),
}

fn reply(r: &Reply, status: StatusCode, body: Value) -> Response {
    match r {
        Reply::Json => (status, Json(body)).into_response(),
        Reply::Gif => (status, [(header::CONTENT_TYPE, "image/gif"), (header::CACHE_CONTROL, "no-cache, no-store, must-revalidate")], GIF).into_response(),
        // Script tags cannot see the status, so JSONP always answers 200.
        Reply::Jsonp(cb) => ([(header::CONTENT_TYPE, "application/javascript; charset=utf-8")], format!("/**/{cb}({body});")).into_response(),
        Reply::Redirect(to) if status.is_success() => {
            (StatusCode::FOUND, [(header::LOCATION, HeaderValue::from_str(to).unwrap_or(HeaderValue::from_static("/")))]).into_response()
        }
        Reply::Redirect(_) => (status, Json(body)).into_response(),
    }
}

fn redirect_allowed(st: &AppState, to: &str) -> bool {
    let Ok(u) = url::Url::parse(to) else { return false };
    let Some(host) = u.host_str() else { return false };
    matches!(u.scheme(), "http" | "https") && st.cfg.server.redirect_allow_hosts.iter().any(|h| h.eq_ignore_ascii_case(host))
}

/// GET on a collection: a write when `data` is present, otherwise the collection's schema.
async fn get_collection(State(st): State<St>, Path((pid, coll)): Path<(String, String)>, Query(params): Query<HashMap<String, String>>, c: Client) -> Response {
    let Some(data) = params.get("data") else {
        return match collection_schema(&st, &pid, &coll, &params, &c).await {
            Ok(v) => Json(v).into_response(),
            Err(e) => e.into_response(),
        };
    };
    let accept = c.headers.get(header::ACCEPT).and_then(|v| v.to_str().ok()).unwrap_or("");
    let r = if let Some(cb) = params.get("jsonp").or_else(|| params.get("callback")) {
        if !valid_callback(cb) {
            return err(StatusCode::BAD_REQUEST, "InvalidParameterError", "invalid JSONP callback name").into_response();
        }
        Reply::Jsonp(cb.clone())
    } else if let Some(to) = params.get("redirect") {
        // Checked before writing, so a rejected redirect records nothing.
        if !redirect_allowed(&st, to) {
            return err(StatusCode::BAD_REQUEST, "InvalidRedirectError", "redirect target is not in the server's allowed hosts").into_response();
        }
        Reply::Redirect(to.clone())
    } else if params.contains_key("c") || (accept.contains("image/") && !accept.contains("application/json")) {
        Reply::Gif
    } else {
        Reply::Json
    };
    let result = async {
        let access = authorize(&st, &pid, &c.headers, &params, Op::Write)?;
        let ev = decode_data(data).ok_or_else(|| err(StatusCode::BAD_REQUEST, "InvalidEventError", "data must be base64-encoded JSON"))?;
        match write_events(&st, &access, &c, vec![(coll.clone(), ev)]).await?.into_iter().next().unwrap() {
            Ok(()) => Ok(()),
            Err(e) => Err(validation_error(e)),
        }
    }
    .await;
    match result {
        Ok(()) => reply(&r, StatusCode::CREATED, json!({"created": true})),
        Err(e) => reply(&r, e.status, e.body()),
    }
}

// ---- schema ----

async fn schema_all(State(st): State<St>, Path(pid): Path<String>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    authorize(&st, &pid, &c.headers, &params, Op::Schema)?;
    let base = format!("/3.0/projects/{pid}/events");
    let out: Vec<Value> = st
        .engine
        .schema(&pid)
        .await?
        .into_iter()
        .map(|(name, props)| json!({"name": name, "properties": props, "url": format!("{base}/{}", urlencode(&name))}))
        .collect();
    Ok(Json(Value::Array(out)))
}

fn urlencode(s: &str) -> String {
    percent_encoding::utf8_percent_encode(s, percent_encoding::NON_ALPHANUMERIC).to_string()
}

async fn collection_schema(st: &AppState, pid: &str, coll: &str, params: &HashMap<String, String>, c: &Client) -> ApiResult<Value> {
    authorize(st, pid, &c.headers, params, Op::Schema)?;
    let s = st.engine.schema(pid).await?;
    let props = s.get(coll).cloned().unwrap_or_default();
    Ok(json!({"properties": props}))
}

async fn property_info(State(st): State<St>, Path((pid, coll, prop)): Path<(String, String, String)>, Query(params): Query<HashMap<String, String>>, c: Client) -> ApiResult<Json<Value>> {
    authorize(&st, &pid, &c.headers, &params, Op::Schema)?;
    let s = st.engine.schema(&pid).await?;
    let t = s.get(&coll).and_then(|p| p.get(&prop)).ok_or_else(|| err(StatusCode::NOT_FOUND, "ResourceNotFoundError", format!("property {prop:?} not found in {coll:?}")))?;
    Ok(Json(json!({"property_name": prop, "type": t, "url": format!("/3.0/projects/{pid}/events/{}/properties/{}", urlencode(&coll), urlencode(&prop))})))
}

// ---- deletes ----

async fn delete_events(State(st): State<St>, Path((pid, coll)): Path<(String, String)>, uri: axum::http::Uri, c: Client) -> ApiResult<Response> {
    let pairs: Vec<(String, String)> = url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()).into_owned().collect();
    let params: HashMap<String, String> = pairs.iter().cloned().collect();
    authorize(&st, &pid, &c.headers, &params, Op::Delete)?;
    // Unknown parameters are rejected: ignoring a misspelled filter would delete everything.
    let mut unknown: Vec<&str> = pairs.iter().map(|(k, _)| k.as_str()).filter(|k| !matches!(*k, "filters" | "timeframe" | "timezone" | "api_key")).collect();
    if !unknown.is_empty() {
        unknown.sort();
        unknown.dedup();
        return Err(err(StatusCode::BAD_REQUEST, "InvalidParameterError", format!("unsupported delete parameter(s): {}; expected filters, timeframe, timezone", unknown.join(", "))));
    }
    let m = query::from_query_pairs(pairs.into_iter());
    let filters: Vec<query::Filter> = match m.get("filters") {
        None => vec![],
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| err(StatusCode::BAD_REQUEST, "InvalidFilterError", format!("invalid filters: {e}")))?,
    };
    query::validate_filters(&filters)?;
    let zone = query::timeframe::parse_timezone(m.get("timezone")).map_err(|e| err(StatusCode::BAD_REQUEST, "InvalidTimezoneError", e))?;
    let range = query::timeframe::parse_timeframe(m.get("timeframe"), zone, Utc::now()).map_err(|e| err(StatusCode::BAD_REQUEST, "TimeframeDefinitionError", e))?;
    let n = if filters.is_empty() && range.is_none() {
        st.engine.delete_events(&pid, &coll, None).await?
    } else {
        let pred = query::exec::event_matcher(&filters, range)?;
        st.engine.delete_events(&pid, &coll, Some(&pred)).await?
    };
    tracing::info!(project = %pid, collection = %coll, deleted = n, "deleted events");
    Ok(StatusCode::NO_CONTENT.into_response())
}
