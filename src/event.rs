//! Event validation and preparation: reserved metadata, timestamps, and data-enrichment add-ons.

use crate::store::table::lookup;
use crate::store::StoredEvent;
use chrono::{DateTime, Datelike, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Timelike, Utc};
use serde::Serialize;
use serde_json::{json, Map, Value};

/// Name of the reserved metadata object present on every event.
pub const RESERVED: &str = "keen";

#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct ValidationError {
    pub name: String,
    pub description: String,
}

pub fn invalid(name: &str, description: impl Into<String>) -> ValidationError {
    ValidationError { name: name.into(), description: description.into() }
}

/// Parses ISO-8601 timestamps. Values without an offset are UTC.
pub fn parse_time(s: &str) -> Option<DateTime<Utc>> {
    if let Ok(t) = DateTime::parse_from_rfc3339(s) {
        return Some(t.with_timezone(&Utc));
    }
    for f in ["%Y-%m-%dT%H:%M:%S%.f%z", "%Y-%m-%dT%H:%M%z", "%Y-%m-%dT%H:%M%:z"] {
        if let Ok(t) = DateTime::parse_from_str(s, f) {
            return Some(t.with_timezone(&Utc));
        }
    }
    for f in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M:%S", "%Y-%m-%dT%H:%M", "%Y-%m-%d %H:%M:%S%.f"] {
        if let Ok(t) = NaiveDateTime::parse_from_str(s, f) {
            return Some(Utc.from_utc_datetime(&t));
        }
    }
    NaiveDate::parse_from_str(s, "%Y-%m-%d").ok().map(|d| Utc.from_utc_datetime(&d.and_hms_opt(0, 0, 0).unwrap()))
}

/// Formats a timestamp as stored and returned: millisecond precision, `Z` for UTC.
pub fn fmt_time<Tz: TimeZone>(t: &DateTime<Tz>) -> String
where
    Tz::Offset: std::fmt::Display,
{
    t.to_rfc3339_opts(SecondsFormat::Millis, true)
}

/// The event's timestamp in milliseconds (from the reserved metadata object).
pub fn event_ts(e: &Value) -> i64 {
    lookup(e, "keen.timestamp").and_then(Value::as_str).and_then(parse_time).map(|t| t.timestamp_millis()).unwrap_or(0)
}

pub fn validate_collection(name: &str) -> Result<(), ValidationError> {
    let err = |d: &str| Err(invalid("InvalidCollectionNameError", d));
    if name.is_empty() {
        return err("event collection name must not be empty");
    }
    if name.chars().count() > 64 {
        return err("event collection name must be 64 characters or fewer");
    }
    if name.starts_with('$') {
        return err("event collection name must not start with $");
    }
    if name.contains('.') || name.contains('\0') {
        return err("event collection name must not contain periods or NUL");
    }
    Ok(())
}

fn validate_keys(m: &Map<String, Value>, depth: usize) -> Result<(), ValidationError> {
    if depth > 32 {
        return Err(invalid("InvalidEventError", "event nesting is deeper than 32 levels"));
    }
    for (k, v) in m {
        if k.is_empty() {
            return Err(invalid("InvalidPropertyNameError", "property names must not be empty"));
        }
        if k.chars().count() > 256 {
            return Err(invalid("InvalidPropertyNameError", "property names must be 256 characters or fewer"));
        }
        if k.starts_with('$') {
            return Err(invalid("InvalidPropertyNameError", format!("property name {k:?} must not start with $")));
        }
        if k.contains('.') {
            return Err(invalid("InvalidPropertyNameError", format!("property name {k:?} must not contain periods")));
        }
        validate_value(v, depth)?;
    }
    Ok(())
}

fn validate_value(v: &Value, depth: usize) -> Result<(), ValidationError> {
    match v {
        Value::Object(m) => validate_keys(m, depth + 1),
        Value::Array(a) => a.iter().try_for_each(|x| validate_value(x, depth + 1)),
        _ => Ok(()),
    }
}

/// Facts about the request that sent an event.
#[derive(Clone, Debug, Default)]
pub struct RequestCtx {
    pub ip: Option<String>,
    pub user_agent: Option<String>,
}

const IP_PLACEHOLDER: &str = "${keen.ip}";
const UA_PLACEHOLDER: &str = "${keen.user_agent}";

/// Replaces the IP and user agent placeholders in string values (whole-string matches only).
fn substitute(v: &mut Value, ctx: &RequestCtx) {
    match v {
        Value::String(s) if s == IP_PLACEHOLDER => *v = ctx.ip.clone().map(Value::String).unwrap_or(Value::Null),
        Value::String(s) if s == UA_PLACEHOLDER => *v = ctx.user_agent.clone().map(Value::String).unwrap_or(Value::Null),
        Value::Array(a) => a.iter_mut().for_each(|x| substitute(x, ctx)),
        Value::Object(m) => m.values_mut().for_each(|x| substitute(x, ctx)),
        _ => {}
    }
}

/// Context for preparing events.
pub struct Preparer<'a> {
    pub max_event_bytes: usize,
    pub geo: Option<&'a maxminddb::Reader<Vec<u8>>>,
}

impl Preparer<'_> {
    /// Validates an event and fills in the reserved metadata. `autofill` (from a restricted key)
    /// overrides client properties.
    pub fn prepare(&self, collection: &str, mut ev: Value, autofill: Option<&Map<String, Value>>, ctx: &RequestCtx, now: DateTime<Utc>) -> Result<StoredEvent, ValidationError> {
        validate_collection(collection)?;
        substitute(&mut ev, ctx);
        let Value::Object(mut ev) = ev else {
            return Err(invalid("InvalidEventError", "event must be a JSON object"));
        };
        if let Some(af) = autofill {
            for (k, v) in af {
                ev.insert(k.clone(), v.clone());
            }
        }
        let mut meta = match ev.remove(RESERVED) {
            None | Some(Value::Null) => Map::new(),
            Some(Value::Object(m)) => m,
            Some(_) => return Err(invalid("InvalidEventError", format!("the reserved {RESERVED:?} property must be an object"))),
        };
        validate_keys(&ev, 0)?;
        let ts = match meta.get("timestamp") {
            None | Some(Value::Null) => now,
            Some(Value::String(s)) => parse_time(s).ok_or_else(|| invalid("InvalidEventError", format!("{RESERVED}.timestamp {s:?} is not an ISO-8601 timestamp")))?,
            Some(_) => return Err(invalid("InvalidEventError", format!("{RESERVED}.timestamp must be an ISO-8601 string"))),
        };
        if !(1900..=2299).contains(&ts.year()) {
            return Err(invalid("InvalidEventError", format!("{RESERVED}.timestamp must be between 1900 and 2299")));
        }
        let id = hex::encode(rand::random::<[u8; 12]>());
        meta.insert("timestamp".into(), Value::String(fmt_time(&ts)));
        meta.insert("created_at".into(), Value::String(fmt_time(&now)));
        meta.insert("id".into(), Value::String(id.clone()));
        let mut ev = Value::Object(ev);
        if crate::standard::is_standard(collection) {
            crate::standard::validate(collection, &ev)?;
            if lookup(&ev, "ip_address").is_none_or(Value::is_null)
                && let Some(ip) = &ctx.ip {
                    set_path(&mut ev, "ip_address", Value::String(ip.clone()));
                }
            if lookup(&ev, "user_agent.raw").is_none_or(Value::is_null)
                && let Some(ua) = &ctx.user_agent {
                    set_path(&mut ev, "user_agent.raw", Value::String(ua.clone()));
                }
        }
        let mut meta = Value::Object(meta);
        apply_addons(&mut ev, &mut meta, self.geo)?;
        crate::standard::enrich(collection, &mut ev, &meta, self.geo);
        ev.as_object_mut().unwrap().insert(RESERVED.into(), meta);
        let size = serde_json::to_vec(&ev).map(|b| b.len()).unwrap_or(0);
        if self.max_event_bytes > 0 && size > self.max_event_bytes {
            return Err(invalid("EventTooLargeError", format!("event is {size} bytes; the limit is {}", self.max_event_bytes)));
        }
        Ok(StoredEvent { collection: collection.to_string(), ts: ts.timestamp_millis(), id, event: ev })
    }
}

/// Resolves a dotted path against the event, where the reserved prefix addresses `meta`.
fn addon_input<'a>(ev: &'a Value, meta: &'a Value, path: &str) -> Option<&'a Value> {
    match path.strip_prefix(&format!("{RESERVED}.")) {
        Some(rest) => lookup(meta, rest),
        None => lookup(ev, path),
    }
}

pub fn set_path(target: &mut Value, path: &str, v: Value) {
    let parts: Vec<&str> = path.split('.').collect();
    let mut cur = target;
    for p in &parts[..parts.len() - 1] {
        let obj = cur.as_object_mut().unwrap();
        let next = obj.entry(p.to_string()).or_insert_with(|| json!({}));
        if !next.is_object() {
            *next = json!({});
        }
        cur = next;
    }
    if let Some(obj) = cur.as_object_mut() {
        obj.insert(parts[parts.len() - 1].to_string(), v);
    }
}

fn remove_path(target: &mut Value, path: &str) {
    let parts: Vec<&str> = path.split('.').collect();
    let mut cur = target;
    for p in &parts[..parts.len() - 1] {
        match cur.get_mut(*p) {
            Some(n) => cur = n,
            None => return,
        }
    }
    if let Some(o) = cur.as_object_mut() {
        o.remove(parts[parts.len() - 1]);
    }
}

fn apply_addons(ev: &mut Value, meta: &mut Value, geo: Option<&maxminddb::Reader<Vec<u8>>>) -> Result<(), ValidationError> {
    let Some(addons) = meta.get("addons").cloned() else { return Ok(()) };
    if addons.is_null() {
        return Ok(());
    }
    let list = addons.as_array().ok_or_else(|| invalid("InvalidAddonError", "addons must be a list of {name, input, output} objects"))?;
    for a in list {
        let name = a.get("name").and_then(Value::as_str).unwrap_or("");
        let input = a.get("input").and_then(Value::as_object);
        let output = a.get("output").and_then(Value::as_str).unwrap_or("");
        let (Some(input), false) = (input, output.is_empty()) else {
            return Err(invalid("InvalidAddonError", format!("addon {name:?} requires input and output")));
        };
        let arg = |k: &str| -> Option<String> {
            let p = input.get(k)?.as_str()?;
            addon_input(ev, meta, p)?.as_str().map(str::to_string)
        };
        let out = match name.strip_prefix(&format!("{RESERVED}:")).unwrap_or(name) {
            "url_parser" => arg("url").map(|u| parse_url(&u)),
            "referrer_parser" => arg("referrer_url").map(|r| parse_referrer(&r, arg("page_url").as_deref())),
            "date_time_parser" | "datetime_parser" => arg("date_time").and_then(|d| parse_time(&d)).map(|t| datetime_parts(&t)),
            "ua_parser" => arg("ua_string").map(|ua| parse_ua(&ua)),
            "ip_to_geo" => {
                let Some(db) = geo else {
                    return Err(invalid("AddonNotAvailableError", "ip_to_geo is not available: no GeoIP database is configured on this server"));
                };
                let ip = arg("ip");
                let out = ip.as_deref().and_then(|ip| geo_lookup(db, ip));
                if (a.get("remove_ip_property").and_then(Value::as_bool) == Some(true) || input.get("remove_ip_property").and_then(Value::as_bool) == Some(true))
                    && let Some(p) = input.get("ip").and_then(Value::as_str) {
                        remove_path(ev, p);
                    }
                out
            }
            _ => return Err(invalid("InvalidAddonError", format!("unknown addon {name:?}"))),
        };
        if let Some(out) = out {
            match output.strip_prefix(&format!("{RESERVED}.")) {
                Some(rest) => set_path(meta, rest, out),
                None => set_path(ev, output, out),
            }
        }
    }
    Ok(())
}

pub fn parse_url(raw: &str) -> Value {
    let Ok(u) = url::Url::parse(raw) else { return Value::Null };
    let mut qs = Map::new();
    for (k, v) in u.query_pairs() {
        qs.insert(k.into_owned(), Value::String(v.into_owned()));
    }
    json!({
        "protocol": u.scheme(),
        "domain": u.host_str().unwrap_or(""),
        "path": u.path(),
        "anchor": u.fragment(),
        "query_string": qs,
    })
}

struct RefSource {
    medium: &'static str,
    source: &'static str,
    params: &'static [&'static str],
}

fn ref_db(label: &str) -> Option<RefSource> {
    let s = |medium, source, params| Some(RefSource { medium, source, params });
    match label {
        "google" => s("SEARCH", "Google", &["q"]),
        "bing" => s("SEARCH", "Bing", &["q"]),
        "yahoo" => s("SEARCH", "Yahoo!", &["p", "q"]),
        "duckduckgo" => s("SEARCH", "DuckDuckGo", &["q"]),
        "baidu" => s("SEARCH", "Baidu", &["wd", "word"]),
        "yandex" => s("SEARCH", "Yandex", &["text"]),
        "ecosia" => s("SEARCH", "Ecosia", &["q"]),
        "brave" => s("SEARCH", "Brave", &["q"]),
        "kagi" => s("SEARCH", "Kagi", &["q"]),
        "perplexity" => s("SEARCH", "Perplexity", &["q"]),
        "chatgpt" | "openai" => s("SEARCH", "ChatGPT", &[]),
        "facebook" | "fb" => s("SOCIAL", "Facebook", &[]),
        "twitter" | "t" | "x" => s("SOCIAL", "Twitter", &[]),
        "linkedin" | "lnkd" => s("SOCIAL", "LinkedIn", &[]),
        "reddit" => s("SOCIAL", "Reddit", &[]),
        "instagram" => s("SOCIAL", "Instagram", &[]),
        "pinterest" => s("SOCIAL", "Pinterest", &[]),
        "youtube" => s("SOCIAL", "YouTube", &[]),
        "tiktok" => s("SOCIAL", "TikTok", &[]),
        "ycombinator" => s("SOCIAL", "Hacker News", &[]),
        "bsky" => s("SOCIAL", "Bluesky", &[]),
        "threads" => s("SOCIAL", "Threads", &[]),
        "mastodon" => s("SOCIAL", "Mastodon", &[]),
        "mail" | "outlook" | "gmail" => s("EMAIL", "Email", &[]),
        _ => None,
    }
}

/// Classifies a referrer as SEARCH, SOCIAL, EMAIL, INTERNAL or UNKNOWN using a built-in table.
pub fn parse_referrer(referrer: &str, page: Option<&str>) -> Value {
    let unknown = json!({"medium": "UNKNOWN", "source": null, "term": null});
    let Ok(r) = url::Url::parse(referrer) else { return unknown };
    let Some(host) = r.host_str().map(str::to_lowercase) else { return unknown };
    if let Some(p) = page.and_then(|p| url::Url::parse(p).ok())
        && p.host_str().is_some_and(|h| h.eq_ignore_ascii_case(&host)) {
            return json!({"medium": "INTERNAL", "source": null, "term": null});
        }
    let labels: Vec<&str> = host.split('.').collect();
    for i in (0..labels.len().saturating_sub(1)).rev() {
        if let Some(s) = ref_db(labels[i]) {
            let term = s.params.iter().find_map(|k| r.query_pairs().find(|(q, v)| q == k && !v.is_empty()).map(|(_, v)| v.into_owned()));
            return json!({"medium": s.medium, "source": s.source, "term": term});
        }
    }
    unknown
}

pub fn datetime_parts(t: &DateTime<Utc>) -> Value {
    json!({
        "millisecond": t.timestamp_subsec_millis(),
        "second": t.second(),
        "minute": t.minute(),
        "hour": t.hour(),
        "day_of_month": t.day(),
        "day_of_week": t.weekday().num_days_from_sunday(),
        "day_of_week_string": t.weekday().to_string(),
        "month": t.month(),
        "year": t.year(),
        "timezone_offset": 0,
    })
}

fn version_parts(v: &str) -> (Value, Value, Value) {
    let mut it = v.split(['.', '_']).map(|s| if s.is_empty() || s == "UNKNOWN" { Value::Null } else { Value::String(s.into()) });
    (it.next().unwrap_or(Value::Null), it.next().unwrap_or(Value::Null), it.next().unwrap_or(Value::Null))
}

/// Parses a user agent into browser, OS and device families.
pub fn parse_ua(ua: &str) -> Value {
    let parser = woothee::parser::Parser::new();
    let Some(r) = parser.parse(ua) else {
        return json!({"browser": {"family": "Other", "major": null, "minor": null, "patch": null},
                      "os": {"family": "Other", "major": null, "minor": null, "patch": null, "patch_minor": null},
                      "device": {"family": "Other"}});
    };
    let (bmaj, bmin, bpatch) = version_parts(r.version);
    let (omaj, omin, opatch) = version_parts(&r.os_version);
    let device = match r.category {
        "smartphone" => "Smartphone",
        "mobilephone" => "Mobile phone",
        "pc" => "Other",
        "crawler" => "Spider",
        "appliance" => "Appliance",
        _ => "Other",
    };
    json!({
        "browser": {"family": if r.name == "UNKNOWN" { "Other" } else { r.name }, "major": bmaj, "minor": bmin, "patch": bpatch},
        "os": {"family": if r.os == "UNKNOWN" { "Other" } else { r.os }, "major": omaj, "minor": omin, "patch": opatch, "patch_minor": null},
        "device": {"family": device},
        "category": r.category,
    })
}

/// Looks up an IP in a MaxMind-format city database.
pub fn geo_lookup(db: &maxminddb::Reader<Vec<u8>>, ip: &str) -> Option<Value> {
    let addr: std::net::IpAddr = ip.parse().ok()?;
    let city: maxminddb::geoip2::City = db.lookup(addr).ok()?.decode().ok()??;
    let en = |names: &maxminddb::geoip2::Names| names.english.map(str::to_string);
    let province = city.subdivisions.first().and_then(|s| en(&s.names));
    Some(json!({
        "city": en(&city.city.names),
        "province": province,
        "country": en(&city.country.names),
        "country_code": city.country.iso_code,
        "continent": en(&city.continent.names),
        "postal_code": city.postal.code,
        "coordinates": [city.location.longitude, city.location.latitude],
    }))
}
