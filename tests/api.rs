//! HTTP API behaviour, exercised through the router.

use axum::body::Body;
use axum::http::{Request, StatusCode};
use base64::Engine as _;
use http_body_util::BodyExt;
use incisive::api::{self, AppState, St};
use incisive::config::Config;
use incisive::meta::Meta;
use incisive::store::{objstore, Engine};
use serde_json::{json, Value};
use std::sync::Arc;
use tower::ServiceExt;

struct Env {
    _dir: tempfile::TempDir,
    st: St,
    app: axum::Router,
    pid: String,
    master: String,
    read: String,
    write: String,
}

async fn env() -> Env {
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::default();
    cfg.storage.backend = "fs".into();
    cfg.storage.fs_path = dir.path().join("bucket");
    cfg.cache.dir = dir.path().join("cache");
    cfg.compaction.enabled = false;
    cfg.wal.group_commit_ms = 1;
    cfg.server.trust_forwarded_for = true;
    cfg.server.redirect_allow_hosts = vec!["example.com".into()];
    cfg.limits.max_batch_events = 10;
    let engine = Engine::open(&cfg, objstore::build(&cfg.storage).unwrap()).await.unwrap();
    let meta = Meta::open(&dir.path().join("meta.sqlite")).unwrap();
    let (p, keys) = meta.create_project(None, "test").unwrap();
    let st: St = Arc::new(AppState::new(cfg, engine, meta).unwrap());
    let app = api::router(st.clone());
    Env { _dir: dir, app, pid: p.id, master: keys[0].1.clone(), read: keys[1].1.clone(), write: keys[2].1.clone(), st }
}

struct Resp {
    status: StatusCode,
    headers: axum::http::HeaderMap,
    body: Vec<u8>,
}

impl Resp {
    fn json(&self) -> Value {
        serde_json::from_slice(&self.body).unwrap_or_else(|_| panic!("not JSON: {}", String::from_utf8_lossy(&self.body)))
    }
}

impl Env {
    async fn call(&self, method: &str, path: &str, key: Option<&str>, body: Option<Value>, headers: &[(&str, &str)]) -> Resp {
        let mut b = Request::builder().method(method).uri(path);
        if let Some(k) = key {
            b = b.header("authorization", k);
        }
        for (h, v) in headers {
            b = b.header(*h, *v);
        }
        let req = match body {
            Some(v) => b.header("content-type", "application/json").body(Body::from(v.to_string())).unwrap(),
            None => b.body(Body::empty()).unwrap(),
        };
        let resp = self.app.clone().oneshot(req).await.unwrap();
        let status = resp.status();
        let headers = resp.headers().clone();
        let body = resp.into_body().collect().await.unwrap().to_bytes().to_vec();
        Resp { status, headers, body }
    }

    fn url(&self, rest: &str) -> String {
        format!("/1.0/projects/{}{rest}", self.pid)
    }

    async fn count(&self, coll: &str) -> Value {
        let r = self.call("GET", &self.url(&format!("/queries/count?event_collection={coll}")), Some(&self.read), None, &[]).await;
        assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
        r.json()["result"].clone()
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn write_transports() {
    let e = env().await;
    // Single event.
    let r = e.call("POST", &e.url("/events/clicks"), Some(&e.write), Some(json!({"a": 1})), &[]).await;
    assert_eq!(r.status, 201);
    assert_eq!(r.json(), json!({"created": true}));
    // Batch with one invalid event.
    let r = e.call("POST", &e.url("/events"), Some(&e.write), Some(json!({"clicks": [{"a": 2}, {"$bad": 1}], "views": [{"b": 1}]})), &[]).await;
    assert_eq!(r.status, 200);
    let j = r.json();
    assert_eq!(j["clicks"][0], json!({"success": true}));
    assert_eq!(j["clicks"][1]["success"], json!(false));
    assert!(j["clicks"][1]["error"]["name"].is_string());
    assert_eq!(j["views"][0], json!({"success": true}));
    // Batch too large.
    let big: Vec<Value> = (0..11).map(|i| json!({"i": i})).collect();
    let r = e.call("POST", &e.url("/events"), Some(&e.write), Some(json!({"clicks": big})), &[]).await;
    assert_eq!(r.status, 400);
    // GET with base64 data and api_key: JSON by default.
    let data = base64::engine::general_purpose::STANDARD.encode(json!({"a": 3}).to_string());
    let enc = urlencode(&data);
    let r = e.call("GET", &e.url(&format!("/events/clicks?api_key={}&data={enc}", e.write)), None, None, &[]).await;
    assert_eq!(r.status, 201, "{}", String::from_utf8_lossy(&r.body));
    // Image beacon.
    let r = e.call("GET", &e.url(&format!("/events/clicks?api_key={}&data={enc}&c=clv1", e.write)), None, None, &[]).await;
    assert_eq!(r.headers["content-type"], "image/gif");
    assert!(r.body.starts_with(b"GIF89a"));
    let r = e.call("GET", &e.url(&format!("/events/clicks?api_key={}&data={enc}", e.write)), None, None, &[("accept", "image/webp,image/*")]).await;
    assert_eq!(r.headers["content-type"], "image/gif");
    // JSONP.
    let r = e.call("GET", &e.url(&format!("/events/clicks?api_key={}&data={enc}&jsonp=cb_1", e.write)), None, None, &[]).await;
    assert_eq!(String::from_utf8_lossy(&r.body), "/**/cb_1({\"created\":true});");
    let r = e.call("GET", &e.url(&format!("/events/clicks?api_key={}&data={enc}&callback=alert(1)", e.write)), None, None, &[]).await;
    assert_eq!(r.status, 400);
    // Redirect: allowed host, then a rejected host that must not write.
    let r = e.call("GET", &e.url(&format!("/events/clicks?api_key={}&data={enc}&redirect=https%3A%2F%2Fexample.com%2Fx", e.write)), None, None, &[]).await;
    assert_eq!(r.status, 302);
    assert_eq!(r.headers["location"], "https://example.com/x");
    let r = e.call("GET", &e.url(&format!("/events/clicks?api_key={}&data={enc}&redirect=https%3A%2F%2Fevil.com%2F", e.write)), None, None, &[]).await;
    assert_eq!(r.status, 400);
    // sendBeacon: text/plain JSON body, key in the query string.
    let req = Request::builder()
        .method("POST")
        .uri(e.url(&format!("/events/clicks?api_key={}", e.write)))
        .header("content-type", "text/plain;charset=UTF-8")
        .body(Body::from(json!({"a": 4}).to_string()))
        .unwrap();
    assert_eq!(e.app.clone().oneshot(req).await.unwrap().status(), 201);
    // 1 + 1 (batch) + 1 + 2 (gif) + 1 (jsonp) + 1 (redirect) + 1 (beacon)
    assert_eq!(e.count("clicks").await, json!(8));
    assert_eq!(e.count("views").await, json!(1));
}

fn urlencode(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

#[tokio::test(flavor = "multi_thread")]
async fn permissions_and_isolation() {
    let e = env().await;
    // Write key cannot query or read schema; read key cannot write.
    assert_eq!(e.call("GET", &e.url("/queries/count?event_collection=x"), Some(&e.write), None, &[]).await.status, 401);
    assert_eq!(e.call("GET", &e.url("/events"), Some(&e.write), None, &[]).await.status, 401);
    assert_eq!(e.call("POST", &e.url("/events/x"), Some(&e.read), Some(json!({})), &[]).await.status, 401);
    // Read key can discover the schema.
    e.call("POST", &e.url("/events/x"), Some(&e.write), Some(json!({"n": {"m": 1}})), &[]).await;
    let r = e.call("GET", &e.url("/events"), Some(&e.read), None, &[]).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()[0]["name"], "x");
    assert_eq!(r.json()[0]["properties"]["n.m"], "num");
    // Only the master key deletes or manages keys.
    assert_eq!(e.call("DELETE", &e.url("/events/x"), Some(&e.read), None, &[]).await.status, 401);
    assert_eq!(e.call("GET", &e.url("/keys"), Some(&e.read), None, &[]).await.status, 401);
    // Missing and unknown keys.
    assert_eq!(e.call("GET", &e.url("/events"), None, None, &[]).await.status, 401);
    assert_eq!(e.call("GET", &e.url("/events"), Some("sk_incisive_nope"), None, &[]).await.status, 401);
    // Another project's key does not work here, and this project's key does not work there.
    let (p2, k2) = e.st.meta.create_project(None, "other").unwrap();
    assert_eq!(e.call("POST", &e.url("/events/x"), Some(&k2[2].1), Some(json!({})), &[]).await.status, 401);
    let r = e.call("GET", &format!("/1.0/projects/{}/queries/count?event_collection=x", p2.id), Some(&e.read), None, &[]).await;
    assert_eq!(r.status, 401);
    let r = e.call("GET", &format!("/1.0/projects/{}/queries/count?event_collection=x", p2.id), Some(&k2[1].1), None, &[]).await;
    assert_eq!(r.json()["result"], json!(0), "projects do not see each other's events");
}

#[tokio::test(flavor = "multi_thread")]
async fn queries_get_post_and_metadata() {
    let e = env().await;
    let evs: Vec<Value> = (1..=4).map(|i| json!({"user": format!("u{}", i % 2), "price": i})).collect();
    e.call("POST", &e.url("/events"), Some(&e.write), Some(json!({"buys": evs})), &[]).await;
    let r = e.call("POST", &e.url("/queries/sum"), Some(&e.read), Some(json!({"event_collection": "buys", "target_property": "price", "analysis_type": "sum", "include_metadata": true})), &[]).await;
    assert_eq!(r.status, 200, "{}", String::from_utf8_lossy(&r.body));
    let j = r.json();
    assert_eq!(j["result"], json!(10));
    assert_eq!(j["query"]["analysis_type"], "sum");
    assert_eq!(j["execution_metadata"]["events_scanned"], json!(4));
    let f = urlencode(&json!([{"property_name": "user", "operator": "eq", "property_value": "u1"}]).to_string());
    let r = e.call("GET", &e.url(&format!("/queries/count?event_collection=buys&filters={f}&group_by=user")), Some(&e.read), None, &[]).await;
    assert_eq!(r.json()["result"], json!([{"user": "u1", "result": 2}]));
    // Unknown parameter and unknown analysis.
    assert_eq!(e.call("GET", &e.url("/queries/count?event_collection=buys&filter=x"), Some(&e.read), None, &[]).await.status, 400);
    assert_eq!(e.call("GET", &e.url("/queries/bogus?event_collection=buys"), Some(&e.read), None, &[]).await.status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn saved_queries() {
    let e = env().await;
    e.call("POST", &e.url("/events"), Some(&e.write), Some(json!({"a": [{"u": 1}, {"u": 2}], "b": [{"u": 1}]})), &[]).await;
    let body = json!({"query": {"analysis_type": "count", "event_collection": "a"}, "metadata": {"display_name": "A"}});
    assert_eq!(e.call("PUT", &e.url("/queries/saved/a-count"), Some(&e.read), Some(body.clone()), &[]).await.status, 401, "read keys cannot save");
    let r = e.call("PUT", &e.url("/queries/saved/a-count"), Some(&e.master), Some(body.clone()), &[]).await;
    assert_eq!(r.status, 201, "{}", String::from_utf8_lossy(&r.body));
    assert_eq!(e.call("PUT", &e.url("/queries/saved/a-count"), Some(&e.master), Some(body), &[]).await.status, 200);
    let r = e.call("GET", &e.url("/queries/saved/a-count/result"), Some(&e.read), None, &[]).await;
    assert_eq!(r.json()["result"], json!(2));
    assert_eq!(r.json()["metadata"]["display_name"], "A");
    let funnel = json!({"query": {"analysis_type": "funnel", "steps": [
        {"event_collection": "a", "actor_property": "u"}, {"event_collection": "b", "actor_property": "u"}]}});
    e.call("PUT", &e.url("/queries/saved/f"), Some(&e.master), Some(funnel), &[]).await;
    let r = e.call("GET", &e.url("/queries/saved/f/result"), Some(&e.read), None, &[]).await;
    assert_eq!(r.json()["result"]["result"], json!([2, 1]), "saved funnels nest result and steps");
    assert_eq!(r.json()["result"]["steps"].as_array().unwrap().len(), 2);
    let r = e.call("GET", &e.url("/queries/saved"), Some(&e.read), None, &[]).await;
    assert_eq!(r.json().as_array().unwrap().len(), 2);
    assert_eq!(e.call("DELETE", &e.url("/queries/saved/f"), Some(&e.master), None, &[]).await.status, 204);
    assert_eq!(e.call("GET", &e.url("/queries/saved/f"), Some(&e.read), None, &[]).await.status, 404);
}

#[tokio::test(flavor = "multi_thread")]
async fn deletes_reject_unknown_params() {
    let e = env().await;
    e.call("POST", &e.url("/events"), Some(&e.write), Some(json!({"d": [{"x": 1}, {"x": 2}, {"x": 3}]})), &[]).await;
    // A PHP-style bracketed filter must not be ignored (that would delete everything).
    let r = e.call("DELETE", &e.url("/events/d?filters%5B0%5D%5Bproperty_name%5D=x"), Some(&e.master), None, &[]).await;
    assert_eq!(r.status, 400);
    assert_eq!(e.count("d").await, json!(3));
    let f = urlencode(&json!([{"property_name": "x", "operator": "gte", "property_value": 2}]).to_string());
    let r = e.call("DELETE", &e.url(&format!("/events/d?filters={f}")), Some(&e.master), None, &[]).await;
    assert_eq!(r.status, 204);
    assert_eq!(e.count("d").await, json!(1));
    assert_eq!(e.call("DELETE", &e.url("/events/d"), Some(&e.master), None, &[]).await.status, 204);
    assert_eq!(e.count("d").await, json!(0));
}

#[tokio::test(flavor = "multi_thread")]
async fn keys_api_and_scoped_keys() {
    let e = env().await;
    let r = e.call("POST", &e.url("/keys"), Some(&e.master), Some(json!({
        "name": "tenant-7",
        "permitted": ["writes", "queries"],
        "options": {"writes": {"autofill": {"tenant": 7}}, "queries": {"filters": [{"property_name": "tenant", "operator": "eq", "property_value": 7}]}}
    })), &[]).await;
    assert_eq!(r.status, 201, "{}", String::from_utf8_lossy(&r.body));
    let k = r.json();
    let secret = k["key"].as_str().unwrap().to_string();
    assert!(secret.starts_with("sk_incisive_"));
    // Autofill overrides the client; query filters restrict reads.
    e.call("POST", &e.url("/events/t"), Some(&secret), Some(json!({"tenant": 9})), &[]).await;
    e.call("POST", &e.url("/events/t"), Some(&e.write), Some(json!({"tenant": 9})), &[]).await;
    let r = e.call("GET", &e.url("/queries/count?event_collection=t"), Some(&secret), None, &[]).await;
    assert_eq!(r.json()["result"], json!(1));
    assert_eq!(e.count("t").await, json!(2));
    // Listing hides custom secrets; revoke and unrevoke by id.
    let r = e.call("GET", &e.url("/keys"), Some(&e.master), None, &[]).await;
    let listed: Vec<Value> = r.json().as_array().unwrap().iter().filter(|k| k["name"] == "tenant-7").cloned().collect();
    assert!(listed[0]["key"].is_null());
    let id = k["id"].as_str().unwrap();
    e.call("POST", &e.url(&format!("/keys/{id}/revoke")), Some(&e.master), None, &[]).await;
    assert_eq!(e.call("POST", &e.url("/events/t"), Some(&secret), Some(json!({})), &[]).await.status, 401);
    e.call("POST", &e.url(&format!("/keys/{id}/unrevoke")), Some(&e.master), None, &[]).await;
    assert_eq!(e.call("POST", &e.url("/events/t"), Some(&secret), Some(json!({})), &[]).await.status, 201);
    // Bad permission names are rejected.
    let r = e.call("POST", &e.url("/keys"), Some(&e.master), Some(json!({"name": "x", "permitted": ["everything"]})), &[]).await;
    assert_eq!(r.status, 400);
    // A legacy scoped key minted from the master key.
    let scoped = incisive::auth::encrypt_scoped(&e.master, &json!({"allowed_operations": ["read"], "filters": [{"property_name": "tenant", "operator": "eq", "property_value": 9}]})).unwrap();
    let r = e.call("GET", &e.url("/queries/count?event_collection=t"), Some(&scoped), None, &[]).await;
    assert_eq!(r.json()["result"], json!(1));
    assert_eq!(e.call("POST", &e.url("/events/t"), Some(&scoped), Some(json!({})), &[]).await.status, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn standard_collections_and_placeholders() {
    let e = env().await;
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
    let hdrs = [("x-forwarded-for", "203.0.113.9, 10.0.0.1"), ("user-agent", ua)];
    let pv = json!({
        "page": {"url": "https://shop.example/pricing?utm_source=news&utm_medium=email", "referrer": "https://www.google.com/"},
        "session": {"id": "s1", "landing_page": {"url": "https://shop.example/pricing?utm_source=news&utm_medium=email"}, "referrer": {"url": "https://www.google.com/"}},
        "visitor": {"id": "v1"},
        "custom": "${incisive.ip}",
        "legacy": "${keen.ip}",
        "agent": "${incisive.user_agent}"
    });
    let r = e.call("POST", &e.url("/events/web_pageview"), Some(&e.write), Some(pv), &hdrs).await;
    assert_eq!(r.status, 201, "{}", String::from_utf8_lossy(&r.body));
    // Missing required page.url.
    let r = e.call("POST", &e.url("/events/web_pageview"), Some(&e.write), Some(json!({"page": {"title": "x"}})), &hdrs).await;
    assert_eq!(r.status, 400);
    assert_eq!(r.json()["error_code"], "MissingStandardPropertyError");
    let r = e.call("GET", &e.url("/queries/extraction?event_collection=web_pageview"), Some(&e.read), None, &[]).await;
    let ev = &r.json()["result"][0];
    assert_eq!(ev["ip_address"], "203.0.113.9");
    assert_eq!(ev["custom"], "203.0.113.9");
    assert_eq!(ev["legacy"], "203.0.113.9", "Keen's placeholder spelling works too");
    assert_eq!(ev["user_agent"]["raw"], ua);
    assert_eq!(ev["agent"], ua, "the user agent placeholder is filled in");
    assert_eq!(ev["browser"]["name"], "Chrome");
    assert_eq!(ev["device"]["type"], "desktop");
    assert_eq!(ev["page"]["path"], "/pricing");
    assert_eq!(ev["page"]["utm"]["source"], "news");
    assert_eq!(ev["session"]["channel"], "Email", "{ev}");
}

#[tokio::test(flavor = "multi_thread")]
async fn ignored_addresses() {
    let e = env().await;
    let (user, _) = e.st.meta.ensure_user("me@example.com").unwrap();
    let (mine, keys) = e.st.meta.create_project(Some(&user.id), "mine").unwrap();
    let (read, write) = (keys[1].1.clone(), keys[2].1.clone());
    let me = "203.0.113.9";
    e.st.meta.ignore_ip(&user.id, me).unwrap();
    let send = |pid: String, key: String, ip: &'static str| {
        let e = &e;
        async move {
            let r = e.call("POST", &format!("/1.0/projects/{pid}/events/visits"), Some(&key), Some(json!({"a": 1})), &[("x-forwarded-for", ip)]).await;
            assert_eq!(r.status, 201, "accepted even when ignored, so trackers don't retry");
        }
    };
    let count = |pid: String, key: String| {
        let e = &e;
        async move { e.call("GET", &format!("/1.0/projects/{pid}/queries/count?event_collection=visits"), Some(&key), None, &[]).await.json()["result"].clone() }
    };
    send(mine.id.clone(), write.clone(), me).await;
    assert_eq!(count(mine.id.clone(), read.clone()).await, json!(0), "the member's own address is not stored");
    send(mine.id.clone(), write.clone(), "198.51.100.1").await;
    assert_eq!(count(mine.id.clone(), read.clone()).await, json!(1), "other visitors are");
    send(e.pid.clone(), e.write.clone(), me).await;
    assert_eq!(count(e.pid.clone(), e.read.clone()).await, json!(1), "projects the user isn't a member of are unaffected");
    e.st.meta.unignore_ip(&user.id, me).unwrap();
    send(mine.id.clone(), write.clone(), me).await;
    assert_eq!(count(mine.id.clone(), read.clone()).await, json!(2), "unticking counts the member again");
}

#[tokio::test(flavor = "multi_thread")]
async fn account_master_keys() {
    let e = env().await;
    let (user, _) = e.st.meta.ensure_user("me@example.com").unwrap();
    let (_, ak) = e.st.meta.create_account_key(&user.id, "ops").unwrap();
    assert_eq!(e.call("GET", "/1.0/account", None, None, &[]).await.status, 401);
    assert_eq!(e.call("GET", "/1.0/account", Some(&e.master), None, &[]).await.status, 401, "project master keys are not account keys");
    let r = e.call("GET", "/1.0/account", Some(&ak), None, &[]).await;
    assert_eq!(r.status, 200);
    assert_eq!(r.json()["email"], "me@example.com");
    assert_eq!(r.json()["projects"], json!([]));

    // Create a project: its keys come back once, and the account key is its master.
    let r = e.call("POST", "/1.0/account/projects", Some(&ak), Some(json!({"name": "Site"})), &[]).await;
    assert_eq!(r.status, 201, "{}", String::from_utf8_lossy(&r.body));
    let p = r.json();
    let pid = p["id"].as_str().unwrap().to_string();
    assert_eq!(p["keys"].as_array().unwrap().len(), 3);
    let write = p["keys"][2]["key"].as_str().unwrap().to_string();
    let r = e.call("POST", &format!("/1.0/projects/{pid}/keys"), Some(&ak), Some(json!({"name": "blog writes", "permitted": ["writes"]})), &[]).await;
    assert_eq!(r.status, 201);
    assert_eq!(e.call("POST", &format!("/1.0/projects/{pid}/events/visits"), Some(&write), Some(json!({"a": 1})), &[]).await.status, 201);
    let r = e.call("GET", &format!("/1.0/projects/{pid}/queries/count?event_collection=visits"), Some(&ak), None, &[]).await;
    assert_eq!(r.json()["result"], json!(1));
    let r = e.call("POST", &format!("/1.0/projects/{pid}"), Some(&ak), Some(json!({"name": "Renamed"})), &[]).await;
    assert_eq!(r.json()["name"], "Renamed");
    assert_eq!(e.call("POST", &format!("/1.0/projects/{pid}"), Some(&write), Some(json!({"name": "x"})), &[]).await.status, 401, "write keys cannot rename");
    assert_eq!(e.call("GET", "/1.0/account/projects", Some(&ak), None, &[]).await.json()[0]["name"], "Renamed");

    // No reach into projects the user isn't a member of.
    assert_eq!(e.call("GET", &e.url("/keys"), Some(&ak), None, &[]).await.status, 401);
    assert_eq!(e.call("POST", &e.url(""), Some(&ak), Some(json!({"name": "x"})), &[]).await.status, 401);

    // Account keys manage each other.
    let r = e.call("POST", "/1.0/account/keys", Some(&ak), Some(json!({"name": "second"})), &[]).await;
    assert_eq!(r.status, 201);
    let (id2, ak2) = (r.json()["id"].as_str().unwrap().to_string(), r.json()["key"].as_str().unwrap().to_string());
    let r = e.call("GET", "/1.0/account/keys", Some(&ak2), None, &[]).await;
    assert_eq!(r.json().as_array().unwrap().len(), 2);
    assert!(r.json()[0]["key"].is_null(), "secrets are never listed");
    assert_eq!(e.call("DELETE", &format!("/1.0/account/keys/{id2}"), Some(&ak), None, &[]).await.status, 204);
    assert_eq!(e.call("GET", "/1.0/account", Some(&ak2), None, &[]).await.status, 401, "deleted");
    assert_eq!(e.call("GET", &format!("/1.0/projects/{pid}/keys"), Some(&ak2), None, &[]).await.status, 401);
}

#[tokio::test(flavor = "multi_thread")]
async fn keen_spellings_are_accepted() {
    let e = env().await;
    let ua = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/130.0.0.0 Safari/537.36";
    // An event as a Keen client library sends it.
    let ev = json!({"ua": ua, "keen": {"timestamp": "2026-01-02T03:04:05.000Z", "addons": [
        {"name": "keen:ua_parser", "input": {"ua_string": "ua"}, "output": "tech"},
        {"name": "keen:date_time_parser", "input": {"date_time": "keen.timestamp"}, "output": "when"}
    ]}});
    assert_eq!(e.call("POST", &e.url("/events/legacy"), Some(&e.write), Some(ev), &[]).await.status, 201);
    let r = e.call("GET", &e.url("/queries/extraction?event_collection=legacy&timeframe=this_10_years"), Some(&e.read), None, &[]).await;
    let got = &r.json()["result"][0];
    assert!(got.get("keen").is_none(), "{got}");
    assert_eq!(got["incisive"]["timestamp"], "2026-01-02T03:04:05.000Z");
    assert_eq!(got["tech"]["browser"]["family"], "Chrome", "{got}");
    assert_eq!(got["when"]["year"], 2026, "{got}");
    // Queries may name reserved properties either way.
    for name in ["keen.timestamp", "incisive.timestamp"] {
        let f = urlencode(&json!([{"property_name": name, "operator": "gt", "property_value": "2026-01-01T00:00:00Z"}]).to_string());
        let r = e.call("GET", &e.url(&format!("/queries/count?event_collection=legacy&timeframe=this_10_years&filters={f}")), Some(&e.read), None, &[]).await;
        assert_eq!(r.json()["result"], json!(1), "{name}: {}", String::from_utf8_lossy(&r.body));
    }
}

#[tokio::test(flavor = "multi_thread")]
async fn events_stored_under_keen_are_migrated() {
    let e = env().await;
    let st = &e.st;
    // Data written before the rename: the reserved object is `keen`, in WAL and in segments.
    let old = |n: i64| incisive::store::StoredEvent {
        collection: "old".into(),
        ts: 1_767_225_600_000 + n,
        id: format!("id{n}"),
        event: json!({"n": n, "keen": {"timestamp": format!("2026-01-01T00:00:00.00{n}Z"), "id": format!("id{n}"), "created_at": "2026-01-01T00:00:00.000Z"}}),
    };
    st.engine.append(&e.pid, vec![old(1), old(2)]).await.unwrap();
    st.engine.compact_all(true).await.unwrap();
    st.engine.append(&e.pid, vec![old(3)]).await.unwrap();
    let schema = st.engine.schema(&e.pid).await.unwrap();
    assert!(schema["old"].contains_key("keen.timestamp"), "segment holds the old name");
    st.engine.migrate_reserved_name().await.unwrap();
    let schema = st.engine.schema(&e.pid).await.unwrap();
    assert!(schema["old"].keys().all(|p| !p.starts_with("keen.")), "{:?}", schema["old"]);
    assert!(schema["old"].contains_key("incisive.timestamp"));
    let r = e.call("GET", &e.url("/queries/count?event_collection=old&timeframe=this_10_years&group_by=incisive.id"), Some(&e.read), None, &[]).await;
    assert_eq!(r.json()["result"].as_array().unwrap().len(), 3, "{}", String::from_utf8_lossy(&r.body));
    let r = e.call("GET", &e.url("/queries/extraction?event_collection=old&timeframe=this_10_years"), Some(&e.read), None, &[]).await;
    assert_eq!(r.json()["result"][2]["incisive"]["timestamp"], "2026-01-01T00:00:00.003Z");
    st.engine.migrate_reserved_name().await.unwrap(); // runs once; a second call is a no-op
}
