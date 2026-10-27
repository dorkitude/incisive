//! Reserved collections with standard schemas: `web_pageview` and `web_session`.
//!
//! Events are denormalized: each one carries the full context needed to analyze it on its own
//! (visitor, session and acquisition, page, device, browser, location, time), so any question can be
//! answered from a single collection without joins. Clients may add any other properties.
//!
//! The web tracker sends what only a browser knows; the server derives the rest (user-agent
//! parsing, referrer classification, campaign parameters, channel grouping, location, local time).

use crate::event::{parse_referrer, parse_ua, set_path, ValidationError};
use crate::store::table::lookup;
use chrono::{Datelike, TimeZone, Timelike};
use serde_json::{json, Map, Value};

pub const PAGEVIEW: &str = "web_pageview";
pub const SESSION: &str = "web_session";

pub fn is_standard(collection: &str) -> bool {
    collection == PAGEVIEW || collection == SESSION
}

/// Expected JSON type of a standard field.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Str,
    Num,
    Bool,
    Obj,
    List,
}

/// One documented field: (path, type, which collections, description).
pub struct Field {
    pub path: &'static str,
    pub kind: Kind,
    pub pageview: bool,
    pub session: bool,
    pub doc: &'static str,
}

const fn f(path: &'static str, kind: Kind, pageview: bool, session: bool, doc: &'static str) -> Field {
    Field { path, kind, pageview, session, doc }
}

use Kind::*;

/// The standard schema. `B` = both collections.
pub static FIELDS: &[Field] = &[
    // Visitor: a pseudonymous browser identity, plus an optional known user.
    f("visitor.id", Str, true, true, "Anonymous visitor ID (first-party, persists across sessions)."),
    f("visitor.user_id", Str, true, true, "Your own user ID, once the visitor is identified."),
    f("visitor.is_new", Bool, true, true, "True on the visitor's first session."),
    f("visitor.first_seen_at", Str, true, true, "When this visitor was first seen (ISO-8601)."),
    f("visitor.session_count", Num, true, true, "Sessions this visitor has started, including this one."),
    f("visitor.traits", Obj, true, true, "Your own properties for the visitor (plan, company, ...)."),
    // Session and acquisition, repeated on every pageview.
    f("session.id", Str, true, true, "Session ID. A session ends after 30 minutes of inactivity or at midnight UTC."),
    f("session.started_at", Str, true, true, "Session start time (ISO-8601)."),
    f("session.pageview_index", Num, true, false, "1 for the session's landing page, 2 for the next, ..."),
    f("session.elapsed_seconds", Num, true, false, "Seconds since the session started."),
    f("session.landing_page.url", Str, true, true, "First page of the session."),
    f("session.landing_page.path", Str, true, true, "Path of the first page of the session."),
    f("session.referrer.url", Str, true, true, "External referrer that started the session."),
    f("session.referrer.hostname", Str, true, true, "Derived: referrer host."),
    f("session.referrer.medium", Str, true, true, "Derived: SEARCH, SOCIAL, EMAIL, INTERNAL or UNKNOWN."),
    f("session.referrer.source", Str, true, true, "Derived: e.g. Google, Twitter."),
    f("session.referrer.term", Str, true, true, "Derived: search terms, when the referrer exposes them."),
    f("session.utm.source", Str, true, true, "Campaign source (utm_source) of the landing page."),
    f("session.utm.medium", Str, true, true, "Campaign medium (utm_medium)."),
    f("session.utm.campaign", Str, true, true, "Campaign name (utm_campaign)."),
    f("session.utm.term", Str, true, true, "Campaign term (utm_term)."),
    f("session.utm.content", Str, true, true, "Campaign content (utm_content)."),
    f("session.click_id.type", Str, true, true, "Ad click ID parameter present on the landing page (gclid, fbclid, msclkid, ...)."),
    f("session.channel", Str, true, true, "Derived default channel: Direct, Organic Search, Paid Search, Organic Social, Paid Social, Email, Referral, Affiliates, Display, Other."),
    // Page.
    f("page.url", Str, true, false, "Full page URL. Required on web_pageview."),
    f("page.path", Str, true, false, "Derived from page.url when absent."),
    f("page.hostname", Str, true, false, "Derived from page.url when absent."),
    f("page.title", Str, true, false, "Document title."),
    f("page.hash", Str, true, false, "URL fragment."),
    f("page.query_string", Obj, true, false, "Derived: query parameters."),
    f("page.referrer", Str, true, false, "document.referrer (may be internal)."),
    f("page.utm.source", Str, true, false, "Campaign parameters on this page's URL (derived)."),
    f("referrer.hostname", Str, true, false, "Derived from page.referrer."),
    f("referrer.medium", Str, true, false, "Derived from page.referrer."),
    f("referrer.source", Str, true, false, "Derived from page.referrer."),
    f("referrer.term", Str, true, false, "Derived from page.referrer."),
    // Device, browser, OS.
    f("user_agent.raw", Str, true, true, "User-Agent header (filled in by the server when absent)."),
    f("user_agent.is_bot", Bool, true, true, "Derived: crawler or automation detected."),
    f("device.type", Str, true, true, "desktop, mobile, tablet or bot (derived when absent)."),
    f("device.screen.width", Num, true, true, "Screen width in CSS pixels."),
    f("device.screen.height", Num, true, true, "Screen height in CSS pixels."),
    f("device.screen.pixel_ratio", Num, true, true, "devicePixelRatio."),
    f("device.viewport.width", Num, true, true, "Viewport width."),
    f("device.viewport.height", Num, true, true, "Viewport height."),
    f("device.touch", Bool, true, true, "Touch input available."),
    f("browser.name", Str, true, true, "Derived browser family."),
    f("browser.version", Str, true, true, "Derived browser version."),
    f("browser.language", Str, true, true, "navigator.language."),
    f("browser.timezone", Str, true, true, "IANA timezone from the browser."),
    f("os.name", Str, true, true, "Derived OS family."),
    f("os.version", Str, true, true, "Derived OS version."),
    f("connection.effective_type", Str, true, false, "Network Information API: 4g, 3g, ..."),
    // Network and location.
    f("ip_address", Str, true, true, "Client IP (filled in by the server when absent)."),
    f("geo.country", Str, true, true, "Derived from ip_address when a GeoIP database is configured."),
    f("geo.country_code", Str, true, true, "ISO country code."),
    f("geo.province", Str, true, true, "Region or state."),
    f("geo.city", Str, true, true, "City."),
    // Performance.
    f("performance.ttfb_ms", Num, true, false, "Time to first byte."),
    f("performance.dom_interactive_ms", Num, true, false, "DOM interactive."),
    f("performance.load_ms", Num, true, false, "Load event end."),
    // Local time, derived from the timestamp and browser.timezone.
    f("local_time.hour", Num, true, true, "Derived: hour of day in the visitor's timezone."),
    f("local_time.day_of_week", Str, true, true, "Derived: weekday in the visitor's timezone."),
    // Tracker.
    f("tracker.name", Str, true, true, "Sending library."),
    f("tracker.version", Str, true, true, "Sending library version."),
];

fn kind_ok(kind: Kind, v: &Value) -> bool {
    match kind {
        Str => v.is_string(),
        Num => v.is_number(),
        Bool => v.is_boolean(),
        Obj => v.is_object(),
        List => v.is_array(),
    }
}

/// Checks standard-field types and required fields. Extra properties are always allowed.
pub fn validate(collection: &str, ev: &Value) -> Result<(), ValidationError> {
    if !is_standard(collection) {
        return Ok(());
    }
    for fd in FIELDS {
        let applies = if collection == PAGEVIEW { fd.pageview } else { fd.session };
        if !applies {
            continue;
        }
        if let Some(v) = lookup(ev, fd.path)
            && !v.is_null() && !kind_ok(fd.kind, v) {
                return Err(ValidationError {
                    name: "InvalidStandardPropertyError".into(),
                    description: format!("{collection}: {} must be a {:?}", fd.path, fd.kind).to_lowercase(),
                });
            }
    }
    let required = if collection == PAGEVIEW { "page.url" } else { "session.id" };
    if lookup(ev, required).and_then(Value::as_str).is_none_or(str::is_empty) {
        return Err(ValidationError {
            name: "MissingStandardPropertyError".into(),
            description: format!("{collection} requires {required}"),
        });
    }
    Ok(())
}

fn missing(ev: &Value, path: &str) -> bool {
    lookup(ev, path).is_none_or(Value::is_null)
}

fn put_if_missing(ev: &mut Value, path: &str, v: Value) {
    if !v.is_null() && missing(ev, path) {
        set_path(ev, path, v);
    }
}

const UTM: [&str; 6] = ["source", "medium", "campaign", "term", "content", "id"];
const CLICK_IDS: [&str; 8] = ["gclid", "gbraid", "wbraid", "fbclid", "msclkid", "ttclid", "li_fat_id", "twclid"];

fn utm_from(url: &url::Url) -> (Map<String, Value>, Option<String>) {
    let mut utm = Map::new();
    let mut click = None;
    for (k, v) in url.query_pairs() {
        if let Some(name) = k.strip_prefix("utm_") {
            if UTM.contains(&name) && !v.is_empty() {
                utm.insert(name.to_string(), Value::String(v.into_owned()));
            }
        } else if CLICK_IDS.contains(&k.as_ref()) && click.is_none() {
            click = Some(k.into_owned());
        }
    }
    (utm, click)
}

/// Fills in derived standard fields. Values the client sent are never overwritten.
pub fn enrich(collection: &str, ev: &mut Value, meta: &Value, geo: Option<&maxminddb::Reader<Vec<u8>>>) {
    if !is_standard(collection) {
        return;
    }
    // Page URL parts and campaign parameters.
    if let Some(u) = lookup(ev, "page.url").and_then(Value::as_str).and_then(|s| url::Url::parse(s).ok()) {
        put_if_missing(ev, "page.path", json!(u.path()));
        put_if_missing(ev, "page.hostname", json!(u.host_str()));
        put_if_missing(ev, "page.hash", json!(u.fragment()));
        let qs: Map<String, Value> = u.query_pairs().map(|(k, v)| (k.into_owned(), Value::String(v.into_owned()))).collect();
        if !qs.is_empty() {
            put_if_missing(ev, "page.query_string", Value::Object(qs));
        }
        let (utm, _) = utm_from(&u);
        if !utm.is_empty() {
            put_if_missing(ev, "page.utm", Value::Object(utm));
        }
    }
    if let Some(u) = lookup(ev, "session.landing_page.url").and_then(Value::as_str).and_then(|s| url::Url::parse(s).ok()) {
        put_if_missing(ev, "session.landing_page.path", json!(u.path()));
        let (utm, click) = utm_from(&u);
        if !utm.is_empty() {
            put_if_missing(ev, "session.utm", Value::Object(utm));
        }
        if let Some(c) = click {
            put_if_missing(ev, "session.click_id.type", json!(c));
        }
    }
    // Referrers.
    let page_url = lookup(ev, "page.url").or_else(|| lookup(ev, "session.landing_page.url")).and_then(Value::as_str).map(str::to_string);
    if let Some(r) = lookup(ev, "page.referrer").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string) {
        let p = parse_referrer(&r, page_url.as_deref());
        let host = url::Url::parse(&r).ok().and_then(|u| u.host_str().map(str::to_string));
        put_if_missing(ev, "referrer.hostname", json!(host));
        for k in ["medium", "source", "term"] {
            put_if_missing(ev, &format!("referrer.{k}"), p[k].clone());
        }
    }
    if let Some(r) = lookup(ev, "session.referrer.url").and_then(Value::as_str).filter(|s| !s.is_empty()).map(str::to_string) {
        let p = parse_referrer(&r, page_url.as_deref());
        let host = url::Url::parse(&r).ok().and_then(|u| u.host_str().map(str::to_string));
        put_if_missing(ev, "session.referrer.hostname", json!(host));
        for k in ["medium", "source", "term"] {
            put_if_missing(ev, &format!("session.referrer.{k}"), p[k].clone());
        }
    }
    if missing(ev, "session.channel") && (lookup(ev, "session.id").is_some() || collection == SESSION) {
        let ch = channel(ev);
        set_path(ev, "session.channel", json!(ch));
    }
    // User agent.
    if let Some(ua) = lookup(ev, "user_agent.raw").and_then(Value::as_str).map(str::to_string) {
        let p = parse_ua(&ua);
        let category = p["category"].as_str().unwrap_or("");
        let is_bot = category == "crawler" || ua.contains("HeadlessChrome") || ua.to_lowercase().contains("bot");
        put_if_missing(ev, "user_agent.is_bot", json!(is_bot));
        let bname = p["browser"]["family"].clone();
        put_if_missing(ev, "browser.name", bname);
        let bver: Vec<String> = ["major", "minor", "patch"].iter().filter_map(|k| p["browser"][*k].as_str().map(str::to_string)).collect();
        if !bver.is_empty() {
            put_if_missing(ev, "browser.version", json!(bver.join(".")));
        }
        let over: Vec<String> = ["major", "minor", "patch"].iter().filter_map(|k| p["os"][*k].as_str().map(str::to_string)).collect();
        let (os, os_version) = os_name(p["os"]["family"].as_str().unwrap_or("Other"), (!over.is_empty()).then(|| over.join(".")));
        put_if_missing(ev, "os.name", json!(os));
        if let Some(v) = os_version {
            put_if_missing(ev, "os.version", json!(v));
        }
        let lower = ua.to_lowercase();
        let dtype = if is_bot {
            "bot"
        } else if lower.contains("ipad") || lower.contains("tablet") || (lower.contains("android") && !lower.contains("mobile")) {
            "tablet"
        } else if category == "smartphone" || category == "mobilephone" || lower.contains("mobi") {
            "mobile"
        } else {
            "desktop"
        };
        put_if_missing(ev, "device.type", json!(dtype));
    }
    // Location.
    if let (Some(db), Some(ip)) = (geo, lookup(ev, "ip_address").and_then(Value::as_str).map(str::to_string))
        && missing(ev, "geo")
            && let Some(g) = crate::event::geo_lookup(db, &ip) {
                set_path(ev, "geo", g);
            }
    // Local time in the visitor's timezone.
    if let (Some(ts), Some(tz)) = (
        lookup(meta, "timestamp").and_then(Value::as_str).and_then(crate::event::parse_time),
        lookup(ev, "browser.timezone").and_then(Value::as_str).and_then(|s| s.parse::<chrono_tz::Tz>().ok()),
    ) {
        let local = tz.from_utc_datetime(&ts.naive_utc());
        put_if_missing(ev, "local_time.hour", json!(local.hour()));
        put_if_missing(ev, "local_time.day_of_week", json!(local.weekday().to_string()));
        put_if_missing(ev, "local_time.date", json!(local.format("%Y-%m-%d").to_string()));
    }
}

/// Default channel grouping from the session's campaign parameters and referrer.
/// Maps parser OS families to the names analytics tools usually report.
fn os_name(family: &str, version: Option<String>) -> (String, Option<String>) {
    match family {
        "iPhone" | "iPad" | "iPod" | "iOS" => ("iOS".into(), version),
        "Mac OSX" => ("macOS".into(), version),
        "ChromeOS" => ("Chrome OS".into(), version),
        f if f.starts_with("Windows ") => {
            let v = f.trim_start_matches("Windows ").to_string();
            ("Windows".into(), version.or(Some(v)))
        }
        f => (f.to_string(), version),
    }
}

pub fn channel(ev: &Value) -> &'static str {
    let s = |p: &str| lookup(ev, p).and_then(Value::as_str).unwrap_or("").to_lowercase();
    let (src, med) = (s("session.utm.source"), s("session.utm.medium"));
    let click = s("session.click_id.type");
    let rmed = s("session.referrer.medium");
    let paid = matches!(med.as_str(), "cpc" | "ppc" | "paid" | "paidsearch" | "paid_search" | "paid-search") || !click.is_empty();
    let social_src = ["facebook", "instagram", "twitter", "x", "linkedin", "reddit", "tiktok", "pinterest", "youtube", "bluesky", "threads", "mastodon"]
        .iter()
        .any(|x| src.contains(x));
    let search_src = ["google", "bing", "yahoo", "duckduckgo", "baidu", "yandex", "ecosia", "brave", "kagi"].iter().any(|x| src.contains(x));
    if med == "email" || src == "newsletter" || rmed == "email" {
        return "Email";
    }
    if med == "affiliate" {
        return "Affiliates";
    }
    if matches!(med.as_str(), "display" | "banner" | "cpm") {
        return "Display";
    }
    if paid && (social_src || matches!(click.as_str(), "fbclid" | "ttclid" | "li_fat_id" | "twclid") || med.contains("social")) {
        return "Paid Social";
    }
    if paid {
        return "Paid Search";
    }
    if social_src || med == "social" || rmed == "social" {
        return "Organic Social";
    }
    if search_src || med == "organic" || rmed == "search" {
        return "Organic Search";
    }
    if !src.is_empty() || !med.is_empty() {
        return if rmed.is_empty() || rmed == "unknown" { "Other" } else { "Referral" };
    }
    if rmed == "unknown" {
        return "Referral";
    }
    "Direct"
}
