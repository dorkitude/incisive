//! Page handlers and HTML templates.

use super::dashboard::{self, Report, Row, Totals};
use super::{cookie, internal, WebUser, TZ_COOKIE};
use crate::api::St;
use crate::auth::{self, KeyOptions, NamedOptions, QueriesOptions, WritesOptions};
use crate::meta::{ApiKey, Project};
use crate::query::timeframe::{parse_timeframe, parse_timezone, Zone};
use crate::standard;
use axum::extract::{Path, Query, State};
use axum::http::HeaderMap;
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use maud::{html, Markup, PreEscaped, DOCTYPE};
use serde_json::{json, Value};
use std::collections::HashMap;

// ---- layout ----

fn head(title: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                title { (title) " · Incisive" }
                link rel="stylesheet" href="/static/style.css";
            }
        }
    }
}

#[derive(PartialEq, Clone, Copy)]
enum Tab {
    Dashboard,
    Install,
    Keys,
    Schema,
    Settings,
}

fn layout(st: &St, u: &WebUser, p: &Project, tab: Tab, title: &str, body: Markup) -> String {
    let projects = st.meta.projects_for(&u.user.id).unwrap_or_default();
    let base = format!("/app/p/{}", p.id);
    let t = |t: Tab, label: &str, href: String| html! { a.on[tab == t] href=(href) { (label) } };
    html! {
        (head(title))
        body {
            header.top {
                a.brand href="/app" { "incisive" span { "." } }
                details style="position:relative" {
                    summary { (p.name) }
                    div.card style="position:absolute;z-index:10;min-width:240px;margin-top:6px" {
                        @for q in &projects {
                            div { a href=(format!("/app/p/{}", q.id)) { (q.name) } }
                        }
                        form method="post" action="/app/projects" style="margin-top:10px" {
                            input type="hidden" name="csrf" value=(u.csrf);
                            div.row { input type="text" name="name" placeholder="New project name" style="flex:1"; button.small type="submit" { "Add" } }
                        }
                    }
                }
                nav.tabs {
                    (t(Tab::Dashboard, "Dashboard", base.clone()))
                    (t(Tab::Install, "Install", format!("{base}/install")))
                    (t(Tab::Keys, "API keys", format!("{base}/keys")))
                    (t(Tab::Schema, "Schema", format!("{base}/schema")))
                    (t(Tab::Settings, "Settings", format!("{base}/settings")))
                }
                div.spacer {}
                span.muted { (u.user.email) }
                form.inline method="post" action="/logout" { button.ghost.small type="submit" { "Sign out" } }
            }
            main { (body) }
        }
    }
    .into_string()
}

pub fn message_page(title: &str, msg: &str) -> String {
    html! {
        (head(title))
        body { div.center { div.card.login { h1 { (title) } p.sub { (msg) } a href="/app" { "Back to the app" } } } }
    }
    .into_string()
}

// ---- sign-in ----

pub fn login_page(error: Option<&str>) -> String {
    html! {
        (head("Sign in"))
        body {
            div.center {
                div.login {
                    span.brand { "incisive" span style="color:var(--accent)" { "." } }
                    p.sub { "Event analytics on object storage. Sign in or create an account. We'll email you a 6-digit code." }
                    div.card {
                        @if let Some(e) = error { div.notice style="margin-bottom:14px" { (e) } }
                        form method="post" action="/login" {
                            div.field {
                                label.f for="email" { "Email" }
                                input #email type="email" name="email" required autofocus autocomplete="email" placeholder="you@company.com";
                            }
                            button type="submit" style="width:100%" { "Email me a sign-in code" }
                        }
                    }
                    p.muted style="margin-top:14px;font-size:12.5px" { "Codes expire after 15 minutes. Once signed in, you stay signed in on this browser." }
                }
            }
        }
    }
    .into_string()
}

pub fn code_page(email: &str, error: Option<&str>) -> String {
    html! {
        (head("Enter your code"))
        body { div.center { div.login {
            span.brand { "incisive" span style="color:var(--accent)" { "." } }
            div.card style="margin-top:14px" {
                h1 { "Check your email ✉️" }
                p.sub { "We sent a 6-digit code to " b { (email) } ". It expires in 15 minutes." }
                @if let Some(e) = error { div.notice style="margin-bottom:14px" { (e) } }
                form method="post" action="/login/code" {
                    input type="hidden" name="email" value=(email);
                    div.field {
                        label.f for="code" { "Sign-in code" }
                        input #code type="text" name="code" required autofocus inputmode="numeric" autocomplete="one-time-code"
                            pattern="[0-9 ]{6,7}" maxlength="7" placeholder="123456"
                            style="font-size:24px;letter-spacing:0.3em;text-align:center;font-family:var(--mono)";
                    }
                    button type="submit" style="width:100%" { "Sign in" }
                }
                form method="post" action="/login" style="margin-top:12px" {
                    input type="hidden" name="email" value=(email);
                    button.ghost type="submit" style="width:100%" { "Send a new code" }
                }
            }
            p.muted style="margin-top:14px;font-size:12.5px" { a href="/login" { "Use a different address" } }
        } } }
    }
    .into_string()
}

pub fn code_email_html(code: &str) -> String {
    html! {
        div style="font-family:-apple-system,Segoe UI,sans-serif;max-width:480px;color:#0b0b0b" {
            p { "Your Incisive sign-in code is:" }
            p style="font-size:28px;font-weight:700;letter-spacing:0.2em;font-family:ui-monospace,Menlo,monospace;margin:16px 0" { (code) }
            p style="color:#52514e;font-size:13px" { "Enter it on the sign-in page. It expires in 15 minutes and works once. If you did not ask for it, you can ignore this email." }
        }
    }
    .into_string()
}

// ---- formatting ----

fn fmt_int(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn fmt_compact(n: f64) -> String {
    if n >= 1e6 {
        format!("{:.1}M", n / 1e6)
    } else if n >= 1e4 {
        format!("{:.0}k", n / 1e3)
    } else if n >= 1e3 {
        format!("{:.1}k", n / 1e3)
    } else if n.fract() == 0.0 {
        format!("{n:.0}")
    } else {
        format!("{n:.1}")
    }
}

fn fmt_duration(secs: f64) -> String {
    let s = secs.round() as u64;
    if s >= 3600 {
        format!("{}h {}m", s / 3600, (s % 3600) / 60)
    } else if s >= 60 {
        format!("{}m {}s", s / 60, s % 60)
    } else {
        format!("{s}s")
    }
}

// ---- dashboard ----

#[derive(Clone, Copy, PartialEq)]
enum Metric {
    Visitors,
    Pageviews,
    Sessions,
    Bounce,
    Duration,
}

impl Metric {
    fn parse(s: &str) -> Metric {
        match s {
            "pageviews" => Metric::Pageviews,
            "sessions" => Metric::Sessions,
            "bounce" => Metric::Bounce,
            "duration" => Metric::Duration,
            _ => Metric::Visitors,
        }
    }
    fn key(self) -> &'static str {
        match self {
            Metric::Visitors => "visitors",
            Metric::Pageviews => "pageviews",
            Metric::Sessions => "sessions",
            Metric::Bounce => "bounce",
            Metric::Duration => "duration",
        }
    }
    fn label(self) -> &'static str {
        match self {
            Metric::Visitors => "Visitors",
            Metric::Pageviews => "Pageviews",
            Metric::Sessions => "Sessions",
            Metric::Bounce => "Bounce rate",
            Metric::Duration => "Avg. session",
        }
    }
    fn value(self, t: &Totals) -> Option<f64> {
        match self {
            Metric::Visitors => Some(t.visitors as f64),
            Metric::Pageviews => Some(t.pageviews as f64),
            Metric::Sessions => Some(t.sessions as f64),
            Metric::Bounce => t.bounce_rate().map(|x| x * 100.0),
            Metric::Duration => t.avg_duration(),
        }
    }
    fn fmt(self, v: Option<f64>) -> String {
        match (self, v) {
            (_, None) => "–".into(),
            (Metric::Bounce, Some(x)) => format!("{x:.0}%"),
            (Metric::Duration, Some(x)) => fmt_duration(x),
            (_, Some(x)) => fmt_int(x as u64),
        }
    }
    /// Lower is better for bounce rate.
    fn lower_is_better(self) -> bool {
        self == Metric::Bounce
    }
}

fn kpi(m: Metric, on: bool, r: &Report, href: String) -> Markup {
    let cur = m.value(&r.totals);
    let prev = m.value(&r.previous);
    let delta = match (cur, prev) {
        (Some(c), Some(p)) if p > 0.0 => Some((c - p) / p * 100.0),
        _ => None,
    };
    let (cls, txt) = match delta {
        Some(d) if d.abs() < 0.5 => ("d", "no change vs. previous period".to_string()),
        Some(d) => {
            let good = (d > 0.0) != m.lower_is_better();
            (if good { "d up" } else { "d down" }, format!("{}{:.0}% vs. previous period", if d > 0.0 { "↑ " } else { "↓ " }, d.abs()))
        }
        None => ("d", "no previous data".into()),
    };
    html! {
        a.kpi.on[on] href=(href) {
            div.l { (m.label()) }
            div.v { (m.fmt(cur)) }
            div class=(cls) { (txt) }
        }
    }
}

fn nice_max(v: f64) -> f64 {
    if v <= 0.0 {
        return 1.0;
    }
    let mag = 10f64.powf(v.log10().floor());
    for m in [1.0, 2.0, 2.5, 5.0, 10.0] {
        if m * mag >= v {
            return m * mag;
        }
    }
    10.0 * mag
}

fn local_label(zone: Zone, t: chrono::DateTime<chrono::Utc>, hourly: bool) -> String {
    let s = zone.format(t);
    match chrono::DateTime::parse_from_rfc3339(&s) {
        Ok(l) if hourly => l.format("%-I %p").to_string(),
        Ok(l) => l.format("%b %-d").to_string(),
        Err(_) => s,
    }
}

/// An area chart with a crosshair tooltip, for one series.
fn chart(r: &Report, m: Metric, zone: Zone, hourly: bool) -> Markup {
    let (w, h, left, bottom, top) = (1000.0, 260.0, 44.0, 26.0, 10.0);
    let vals: Vec<Option<f64>> = r.series.iter().map(|b| m.value(&b.totals)).collect();
    let max = nice_max(vals.iter().flatten().cloned().fold(0.0, f64::max));
    let n = vals.len().max(1);
    let x = |i: usize| left + (w - left) * if n == 1 { 0.5 } else { i as f64 / (n - 1) as f64 };
    let y = |v: f64| top + (h - top - bottom) * (1.0 - v / max);
    let pts: Vec<(f64, f64)> = vals.iter().enumerate().map(|(i, v)| (x(i), y(v.unwrap_or(0.0)))).collect();
    let line: String = pts.iter().enumerate().map(|(i, (px, py))| format!("{}{px:.1},{py:.1}", if i == 0 { "M" } else { "L" })).collect();
    let area = format!("{line}L{:.1},{:.1}L{:.1},{:.1}Z", pts.last().map_or(left, |p| p.0), y(0.0), pts.first().map_or(left, |p| p.0), y(0.0));
    let label_every = (n as f64 / 7.0).ceil().max(1.0) as usize;
    let data: Vec<Value> = r
        .series
        .iter()
        .zip(&vals)
        .map(|(b, v)| json!({"label": if hourly { format!("{} {}", local_label(zone, b.range.start, false), local_label(zone, b.range.start, true)) } else { local_label(zone, b.range.start, false) }, "value": m.fmt(*v)}))
        .collect();
    html! {
        div.chart #chart data-points=(serde_json::to_string(&pts).unwrap()) data-series=(serde_json::to_string(&data).unwrap()) data-name=(m.label()) {
            svg viewBox=(format!("0 0 {w} {h}")) preserveAspectRatio="none" role="img" aria-label=(format!("{} over time", m.label())) {
                g.grid {
                    @for k in 0..=4 {
                        @let gy = y(max * k as f64 / 4.0);
                        line x1=(left) x2=(w) y1=(gy) y2=(gy) {}
                    }
                }
                path.area d=(area) {}
                path.line d=(line) vector-effect="non-scaling-stroke" {}
                line.cross #cross x1="0" x2="0" y1=(top) y2=(h - bottom) style="display:none" vector-effect="non-scaling-stroke" {}
            }
            // Axis labels in HTML so they do not stretch with the SVG.
            div.axis style="position:absolute;inset:0;pointer-events:none;font-size:11px;color:var(--muted)" {
                @for k in 0..=4 {
                    @let v = max * k as f64 / 4.0;
                    div style=(format!("position:absolute;left:0;top:calc({:.2}% - 8px)", y(v) / h * 100.0)) {
                        (if m == Metric::Bounce { format!("{v:.0}%") } else if m == Metric::Duration { fmt_duration(v) } else { fmt_compact(v) })
                    }
                }
                @for (i, b) in r.series.iter().enumerate() {
                    @if i % label_every == 0 {
                        div style=(format!("position:absolute;bottom:0;left:{:.2}%;transform:translateX(-50%);white-space:nowrap", x(i) / w * 100.0)) {
                            (local_label(zone, b.range.start, hourly))
                        }
                    }
                }
            }
            div.dot #dot style="display:none;position:absolute;width:10px;height:10px;border-radius:50%;background:var(--accent);border:2px solid var(--surface);transform:translate(-50%,-50%)" {}
            div.tip #tip style="display:none" {}
        }
        script { (PreEscaped(CHART_JS)) }
    }
}

const CHART_JS: &str = r#"(function(){
var c=document.getElementById('chart'); if(!c) return;
var pts=JSON.parse(c.dataset.points), s=JSON.parse(c.dataset.series), svg=c.querySelector('svg');
var cross=document.getElementById('cross'), tip=document.getElementById('tip'), dot=document.getElementById('dot');
function show(ev){ var r=svg.getBoundingClientRect(); var fx=(ev.clientX-r.left)/r.width*1000;
 var best=0; for(var i=1;i<pts.length;i++){ if(Math.abs(pts[i][0]-fx)<Math.abs(pts[best][0]-fx)) best=i; }
 var p=pts[best]; cross.setAttribute('x1',p[0]); cross.setAttribute('x2',p[0]); cross.style.display='';
 var px=p[0]/1000*r.width, py=p[1]/260*r.height; dot.style.display=''; dot.style.left=px+'px'; dot.style.top=py+'px';
 tip.innerHTML=''; var l=document.createElement('div'); l.className='muted'; l.textContent=s[best].label; var v=document.createElement('div');
 v.textContent=c.dataset.name+': '; var b=document.createElement('b'); b.textContent=s[best].value; v.appendChild(b); tip.appendChild(l); tip.appendChild(v);
 tip.style.display=''; var tx=px+12; if(tx+tip.offsetWidth>r.width) tx=px-12-tip.offsetWidth; tip.style.left=tx+'px'; tip.style.top=Math.max(0,py-50)+'px'; }
function hide(){ cross.style.display='none'; tip.style.display='none'; dot.style.display='none'; }
c.addEventListener('mousemove',show); c.addEventListener('mouseleave',hide);
c.addEventListener('touchstart',function(e){show(e.touches[0]);},{passive:true});
})();"#;

/// Sets the browser's timezone in a cookie and reloads once when it changed.
const TZ_JS: &str = r#"(function(){try{var tz=Intl.DateTimeFormat().resolvedOptions().timeZone;
var m=document.cookie.match(/(?:^|; )incisive_tz=([^;]*)/); var cur=m?decodeURIComponent(m[1]):'';
if(tz&&tz!==cur){document.cookie='incisive_tz='+encodeURIComponent(tz)+'; path=/; max-age=31536000; SameSite=Lax'; if(!sessionStorage.getItem('tzr')){sessionStorage.setItem('tzr','1');location.reload();}}}catch(e){}})();"#;

fn bars(title: &str, col: &str, unit: &str, rows: &[Row]) -> Markup {
    let max = rows.iter().map(|r| r.count).max().unwrap_or(1).max(1);
    html! {
        div.card {
            h3 { (title) }
            @if rows.is_empty() {
                div.empty { "No data yet" }
            } @else {
                div.bars role="table" aria-label=(title) {
                    div.hd role="row" { span role="columnheader" { (col) } span role="columnheader" { (unit) } }
                    @for r in rows {
                        div.r role="row" title=(format!("{}: {} {} · {} visitors", r.label, fmt_int(r.count), unit.to_lowercase(), fmt_int(r.visitors))) {
                            div.fill style=(format!("width:{:.1}%", r.count as f64 / max as f64 * 100.0)) {}
                            span role="cell" { (r.label) }
                            span.n role="cell" { (fmt_int(r.count)) }
                        }
                    }
                }
            }
        }
    }
}

pub async fn dashboard(State(st): State<St>, u: WebUser, Path(pid): Path<String>, Query(q): Query<HashMap<String, String>>, headers: HeaderMap) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let range_key = q.get("range").map(String::as_str).unwrap_or("7d");
    let (tf, interval, hourly) = match range_key {
        "today" => ("this_1_days", "hourly", true),
        "24h" => ("this_24_hours", "hourly", true),
        "30d" => ("this_30_days", "daily", false),
        "90d" => ("this_90_days", "daily", false),
        _ => ("this_7_days", "daily", false),
    };
    let metric = Metric::parse(q.get("metric").map(String::as_str).unwrap_or("visitors"));
    let tz_name = cookie(&headers, TZ_COOKIE).unwrap_or_else(|| "UTC".into());
    let zone = parse_timezone(Some(&Value::String(tz_name.clone()))).unwrap_or(Zone::utc());
    let now = chrono::Utc::now();
    let range = match parse_timeframe(Some(&Value::String(tf.into())), zone, now) {
        Ok(Some(r)) => r,
        _ => return internal(anyhow::anyhow!("bad timeframe")),
    };
    let (report, live) = match tokio::try_join!(dashboard::report(&st.engine, &pid, range, interval, zone), dashboard::live_visitors(&st.engine, &pid)) {
        Ok(x) => x,
        Err(e) => return internal(e),
    };
    let custom = custom_events(&st, &pid, tf, &tz_name).await;
    let base = format!("/app/p/{pid}");
    let href = |range: &str, metric: Metric| format!("{base}?range={range}&metric={}", metric.key());
    let empty = report.totals.pageviews == 0 && report.previous.pageviews == 0;
    let body = html! {
        div.row style="margin-bottom:16px" {
            div { h1 { "Dashboard" } span.live { i {} (fmt_int(live)) " visitors in the last 30 minutes" } }
            div.spacer {}
            div.seg {
                @for (k, l) in [("today", "Today"), ("24h", "24 hours"), ("7d", "7 days"), ("30d", "30 days"), ("90d", "90 days")] {
                    a.on[k == range_key] href=(href(k, metric)) { (l) }
                }
            }
        }
        @if empty {
            div.notice style="margin-bottom:16px" {
                "No pageviews yet. " a href=(format!("{base}/install")) { "Install the tracker" } " on your site and visits will appear here within seconds."
            }
        }
        div.card style="margin-bottom:16px" {
            div.kpis {
                @for m in [Metric::Visitors, Metric::Pageviews, Metric::Sessions, Metric::Bounce, Metric::Duration] {
                    (kpi(m, m == metric, &report, href(range_key, m)))
                }
            }
            (chart(&report, metric, zone, hourly))
            p.muted style="margin:8px 0 0;font-size:12px" {
                "Times in " (tz_name) ". Pages per session: "
                (report.totals.pages_per_session().map(|x| format!("{x:.2}")).unwrap_or("–".into()))
                @if report.bots_excluded > 0 { ". " (fmt_int(report.bots_excluded)) " bot pageviews excluded" }
                "."
            }
        }
        div.grid.g2 {
            (bars("Top pages", "Page", "Pageviews", &report.pages))
            (bars("Landing pages", "Page", "Sessions", &report.landing))
            (bars("Channels", "Channel", "Sessions", &report.channels))
            (bars("Sources", "Source", "Sessions", &report.sources))
            (bars("Campaigns", "utm_campaign", "Sessions", &report.campaigns))
            (bars("Countries", "Country", "Sessions", &report.countries))
            (bars("Devices", "Device", "Sessions", &report.devices))
            (bars("Browsers", "Browser", "Sessions", &report.browsers))
            (bars("Operating systems", "OS", "Sessions", &report.oses))
            (bars("Custom events", "Collection", "Events", &custom))
        }
        script { (PreEscaped(TZ_JS)) }
    };
    Html(layout(&st, &u, &p, Tab::Dashboard, "Dashboard", body)).into_response()
}

/// Event counts for the project's non-standard collections.
async fn custom_events(st: &St, pid: &str, tf: &str, tz: &str) -> Vec<Row> {
    let Ok(schema) = st.engine.schema(pid).await else { return vec![] };
    let ex = crate::query::exec::Executor::new(&st.engine, st.query_limits(), chrono::Utc::now());
    let mut rows = vec![];
    for coll in schema.keys().filter(|c| !standard::is_standard(c)).take(20) {
        let m = json!({"event_collection": coll, "timeframe": tf, "timezone": tz});
        let Ok(q) = crate::query::parse("count", m.as_object().unwrap()) else { continue };
        if let Ok(out) = ex.run(pid, &q, &[]).await {
            let n = out.get("result").and_then(Value::as_u64).unwrap_or(0);
            if n > 0 {
                rows.push(Row { label: coll.clone(), count: n, visitors: 0 });
            }
        }
    }
    rows.sort_by(|a, b| b.count.cmp(&a.count));
    rows.truncate(10);
    rows
}

// ---- keys ----

fn perm_pills(k: &ApiKey) -> Markup {
    html! {
        @if k.master { span.pill.on { "everything (master)" } }
        @for p in &k.permitted { span.pill { (p) } }
    }
}

fn copyable(secret: &str) -> Markup {
    html! {
        div.secret {
            code { (secret) }
            button.ghost.small type="button" onclick=(format!("navigator.clipboard.writeText('{secret}');this.textContent='Copied'")) { "Copy" }
        }
    }
}

const PERM_DOCS: [(&str, &str); 6] = [
    ("writes", "Send events (tracking). Safe to embed in web pages and pixels."),
    ("queries", "Run analyses and read the schema."),
    ("saved_queries", "Read saved queries and their results."),
    ("cached_queries", "Read cached queries and their results."),
    ("schema", "List collections and properties."),
    ("datasets", "Reserved for cached datasets."),
];

fn keys_body(st: &St, u: &WebUser, p: &Project, created: Option<(&ApiKey, &str)>, error: Option<&str>) -> Markup {
    let keys = st.meta.keys(&p.id).unwrap_or_default();
    let base = format!("/app/p/{}/keys", p.id);
    html! {
        h1 { "API keys" }
        p.sub { "Keys start with " code { (auth::KEY_PREFIX) } ". Send one in the " code { "Authorization" } " header (or as " code { "api_key" } "). Custom keys can be limited to specific operations, can stamp properties onto every event they write, and can restrict every query they run with filters." }
        @if let Some((k, secret)) = created {
            div.notice style="margin-bottom:16px" {
                b { "New key “" (k.name) "” created." }
                @if k.secret.is_none() { " Copy it now: it won't be shown again." }
                div style="margin-top:8px" { (copyable(secret)) }
            }
        }
        @if let Some(e) = error { div.notice style="margin-bottom:16px" { (e) } }
        div.card style="margin-bottom:16px" {
            div.table-wrap {
                table {
                    thead { tr { th { "Name" } th { "Key" } th { "Permissions" } th { "Status" } th { "Last used" } th {} } }
                    tbody {
                        @for k in &keys {
                            tr {
                                td { b { (k.name) } }
                                td {
                                    @if let Some(s) = &k.secret {
                                        details { summary.mono { (k.hint) } div style="margin-top:6px;min-width:280px" { (copyable(s)) } }
                                    } @else { span.mono { (k.hint) } }
                                }
                                td { (perm_pills(k)) @if k.options != KeyOptions::default() { details { summary { "options" } pre { (serde_json::to_string_pretty(&k.options).unwrap_or_default()) } } } }
                                td { @if k.is_active { span.pill.on { "active" } } @else { span.pill.off { "revoked" } } }
                                td.muted { (k.last_used_at.map(crate::api::iso).unwrap_or_else(|| "never".into())) }
                                td style="white-space:nowrap" {
                                    @if !k.master {
                                        @for (action, label, cls) in [(if k.is_active { "revoke" } else { "unrevoke" }, if k.is_active { "Revoke" } else { "Restore" }, "ghost"), ("delete", "Delete", "danger")] {
                                            form.inline method="post" action=(format!("{base}/{}/{action}", k.id)) {
                                                input type="hidden" name="csrf" value=(u.csrf);
                                                button.small class=(cls) type="submit" { (label) }
                                            }
                                            " "
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
        div.card {
            h2 { "Create a key" }
            form method="post" action=(base) {
                input type="hidden" name="csrf" value=(u.csrf);
                div.field { label.f { "Name" } input type="text" name="name" required placeholder="e.g. Marketing site, Customer 42 dashboard"; }
                div.field {
                    label.f { "Permissions" }
                    @for (perm, doc) in PERM_DOCS {
                        label style="display:block;margin:4px 0" {
                            input type="checkbox" name=(format!("p_{perm}")) value="1" checked[perm == "writes"];
                            " " code { (perm) } " — " span.muted { (doc) }
                        }
                    }
                }
                details style="margin-bottom:14px" {
                    summary { "Advanced options" }
                    div style="margin-top:10px" {
                        div.field {
                            label.f { "Autofill (writes)" }
                            textarea name="autofill" placeholder=r#"{"customer": {"id": 42}}"# {}
                            div.hint { "JSON object merged into every event written with this key, overriding what the client sends." }
                        }
                        div.field {
                            label.f { "Query filters (queries)" }
                            textarea name="filters" placeholder=r#"[{"property_name": "customer.id", "operator": "eq", "property_value": 42}]"# {}
                            div.hint { "Filters added to every query run with this key, so it only ever sees matching events." }
                        }
                        div.field {
                            label.f { "Allowed saved queries" }
                            input type="text" name="saved_allowed" placeholder="weekly-signups, revenue-by-plan";
                            div.hint { "Comma-separated names. Empty allows all." }
                        }
                    }
                }
                button type="submit" { "Create key" }
            }
        }
    }
}

pub async fn keys_page(State(st): State<St>, u: WebUser, Path(pid): Path<String>) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    Html(layout(&st, &u, &p, Tab::Keys, "API keys", keys_body(&st, &u, &p, None, None))).into_response()
}

fn key_spec(form: &HashMap<String, String>) -> Result<(String, Vec<String>, KeyOptions), String> {
    let name = form.get("name").map(|s| s.trim().to_string()).filter(|s| !s.is_empty()).ok_or("A name is required.")?;
    let permitted: Vec<String> = auth::PERMISSIONS.iter().filter(|p| form.contains_key(&format!("p_{p}"))).map(|p| p.to_string()).collect();
    if permitted.is_empty() {
        return Err("Choose at least one permission.".into());
    }
    let mut options = KeyOptions::default();
    if let Some(a) = form.get("autofill").map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let v: serde_json::Map<String, Value> = serde_json::from_str(a).map_err(|e| format!("Autofill must be a JSON object: {e}"))?;
        options.writes = Some(WritesOptions { autofill: Some(v) });
    }
    if let Some(f) = form.get("filters").map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let v = serde_json::from_str(f).map_err(|e| format!("Query filters must be a JSON list of filters: {e}"))?;
        options.queries = Some(QueriesOptions { filters: v });
    }
    if let Some(s) = form.get("saved_allowed").map(|s| s.trim()).filter(|s| !s.is_empty()) {
        let names: Vec<String> = s.split(',').map(|x| x.trim().to_string()).filter(|x| !x.is_empty()).collect();
        options.saved_queries = Some(NamedOptions { allowed: Some(names), blocked: None });
    }
    auth::validate_key_spec(&permitted, &options)?;
    Ok((name, permitted, options))
}

pub async fn create_key(State(st): State<St>, u: WebUser, Path(pid): Path<String>, Form(form): Form<HashMap<String, String>>) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    if let Err(r) = u.check_csrf(&form) {
        return r;
    }
    let body = match key_spec(&form) {
        Ok((name, permitted, options)) => match st.meta.create_key(&pid, &name, false, permitted, options) {
            Ok((k, secret)) => keys_body(&st, &u, &p, Some((&k, &secret)), None),
            Err(e) => return internal(e),
        },
        Err(msg) => keys_body(&st, &u, &p, None, Some(&msg)),
    };
    Html(layout(&st, &u, &p, Tab::Keys, "API keys", body)).into_response()
}

pub async fn key_action(State(st): State<St>, u: WebUser, Path((pid, id, action)): Path<(String, String, String)>, Form(form): Form<HashMap<String, String>>) -> Response {
    if let Err(r) = u.project(&st, &pid) {
        return r;
    }
    if let Err(r) = u.check_csrf(&form) {
        return r;
    }
    let Ok(Some(mut k)) = st.meta.key(&pid, &id) else { return Redirect::to(&format!("/app/p/{pid}/keys")).into_response() };
    if !k.master {
        let r = match action.as_str() {
            "revoke" | "unrevoke" => {
                k.is_active = action == "unrevoke";
                st.meta.update_key(&k)
            }
            "delete" => st.meta.delete_key(&pid, &id).map(|_| ()),
            _ => Ok(()),
        };
        if let Err(e) = r {
            return internal(e);
        }
    }
    Redirect::to(&format!("/app/p/{pid}/keys")).into_response()
}

// ---- install ----

fn write_keys(st: &St, pid: &str) -> Vec<ApiKey> {
    st.meta.keys(pid).unwrap_or_default().into_iter().filter(|k| k.is_active && !k.master && k.secret.is_some() && crate::meta::is_write_only(&k.permitted)).collect()
}

pub async fn install_page(State(st): State<St>, u: WebUser, Path(pid): Path<String>) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let public = st.cfg.server.public_url.trim_end_matches('/').to_string();
    let keys = write_keys(&st, &pid);
    let body = html! {
        h1 { "Install" }
        p.sub { "Project ID " code { (p.id) } " · API base " code { (public) "/3.0/projects/" (p.id) } }
        @if keys.is_empty() {
            div.notice style="margin-bottom:16px" {
                "You need a write-only key to track. "
                form.inline method="post" action=(format!("/app/p/{pid}/install")) {
                    input type="hidden" name="csrf" value=(u.csrf);
                    button.small type="submit" { "Create a write key" }
                }
            }
        } @else {
            @let key = keys[0].secret.clone().unwrap_or_default();
            div.card style="margin-bottom:16px" {
                h2 { "1. Website tracker" }
                p { "Paste this before " code { "</head>" } " on every page. It records " code { "web_session" } " and " code { "web_pageview" } " events with the standard schema: visitor and session IDs, landing page, referrer, campaign parameters, device, browser, language, timezone, performance timings and more. Single-page apps are tracked automatically." }
                pre { (format!("<script defer src=\"{public}/js/incisive.js\"\n  data-project=\"{}\"\n  data-key=\"{key}\"></script>", p.id)) }
                p { "Track your own events and identify signed-in users:" }
                pre { "incisive.track(\"signup\", {plan: \"pro\"});\nincisive.identify(\"user-42\", {plan: \"pro\"});" }
                p.muted { "Calls made before the script loads can be queued: " code { "window.incisive = window.incisive || {q: []}; incisive.q.push([\"track\", \"signup\", {}]);" } }
            }
            div.card style="margin-bottom:16px" {
                h2 { "2. Tracking pixel" }
                p { "A 1×1 image that records an event when it loads: for emails, or anywhere scripts can't run. The event is base64-encoded JSON in the URL." }
                div.grid.g2 {
                    div {
                        div.field { label.f { "Collection" } input #px-coll type="text" value="email_opens"; }
                        div.field { label.f { "Event properties (JSON)" } textarea #px-props { "{\n  \"campaign\": \"october-newsletter\",\n  \"recipient\": {\"id\": \"123\"}\n}" } }
                        div.field {
                            label.f { "Write key" }
                            select #px-key { @for k in &keys { option value=(k.secret.clone().unwrap_or_default()) { (k.name) " (" (k.hint) ")" } } }
                        }
                    }
                    div {
                        label.f { "Image tag" }
                        pre #px-tag {}
                        label.f { "URL" }
                        pre #px-url {}
                        div.hint #px-err {}
                    }
                }
                script { (PreEscaped(format!(r#"(function(){{
var base={}; function b64(s){{return btoa(unescape(encodeURIComponent(s)));}}
function upd(){{var e=document.getElementById('px-err'); e.textContent='';
 var coll=document.getElementById('px-coll').value.trim(), key=document.getElementById('px-key').value, props;
 try{{props=JSON.parse(document.getElementById('px-props').value||'{{}}');}}catch(x){{e.textContent='Properties are not valid JSON: '+x.message;return;}}
 var url=base+'/events/'+encodeURIComponent(coll)+'?api_key='+encodeURIComponent(key)+'&data='+encodeURIComponent(b64(JSON.stringify(props)))+'&c=pixel';
 document.getElementById('px-url').textContent=url;
 document.getElementById('px-tag').textContent='<img src="'+url.replace(/&/g,'&amp;')+'" width="1" height="1" alt="" style="display:none">';}}
['px-coll','px-props','px-key'].forEach(function(id){{document.getElementById(id).addEventListener('input',upd);}}); upd();}})();"#,
                    serde_json::to_string(&format!("{public}/3.0/projects/{}", p.id)).unwrap()))) }
            }
            div.card {
                h2 { "3. Server-side and other SDKs" }
                p { "Any client that speaks the event API works: point it at " code { (public) } ", use project ID " code { (p.id) } " and a key from " a href=(format!("/app/p/{pid}/keys")) { "API keys" } "." }
                pre { (format!("curl -X POST {public}/3.0/projects/{}/events/purchases \\\n  -H 'Authorization: {key}' -H 'Content-Type: application/json' \\\n  -d '{{\"item\": {{\"name\": \"hat\", \"price\": 25}}, \"customer\": {{\"id\": \"c-1\"}}}}'", p.id)) }
            }
        }
    };
    Html(layout(&st, &u, &p, Tab::Install, "Install", body)).into_response()
}

pub async fn mint_write_key(State(st): State<St>, u: WebUser, Path(pid): Path<String>, Form(form): Form<HashMap<String, String>>) -> Response {
    if let Err(r) = u.project(&st, &pid) {
        return r;
    }
    if let Err(r) = u.check_csrf(&form) {
        return r;
    }
    if let Err(e) = st.meta.create_key(&pid, "Write key", false, vec!["writes".into()], KeyOptions::default()) {
        return internal(e);
    }
    Redirect::to(&format!("/app/p/{pid}/install")).into_response()
}

// ---- schema ----

pub async fn schema_page(State(st): State<St>, u: WebUser, Path(pid): Path<String>) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let observed = st.engine.schema(&pid).await.unwrap_or_default();
    let body = html! {
        h1 { "Schema" }
        p.sub { "Events are schemaless JSON. Two collections have a standard schema that powers the dashboard: send these properties (the tracker does) and the server derives the rest. You can add any other properties." }
        div.card style="margin-bottom:16px" {
            h2 { "Standard collections: " code { (standard::PAGEVIEW) } " and " code { (standard::SESSION) } }
            p.muted { "Properties are denormalized: every pageview repeats its session's acquisition data (landing page, referrer, campaign, channel) and its visitor's identity, so any question is one query on one collection with no joins. Derived properties are only filled when absent, so you can always override them." }
            div.table-wrap {
                table {
                    thead { tr { th { "Property" } th { "Type" } th { "Pageview" } th { "Session" } th { "Description" } } }
                    tbody {
                        @for f in standard::FIELDS {
                            tr {
                                td.mono { (f.path) }
                                td.muted { (format!("{:?}", f.kind).to_lowercase()) }
                                td { @if f.pageview { "✓" } }
                                td { @if f.session { "✓" } }
                                td { (f.doc) }
                            }
                        }
                    }
                }
            }
        }
        div.card {
            h2 { "Collections in this project" }
            @if observed.is_empty() { div.empty { "No events yet." } }
            @for (coll, props) in &observed {
                details style="margin-bottom:8px" {
                    summary { b { (coll) } span.muted { " · " (props.len()) " properties" } }
                    div.table-wrap { table { tbody { @for (path, t) in props { tr { td.mono { (path) } td.muted { (t) } } } } } }
                }
            }
        }
    };
    Html(layout(&st, &u, &p, Tab::Schema, "Schema", body)).into_response()
}

// ---- settings ----

pub async fn settings_page(State(st): State<St>, u: WebUser, Path(pid): Path<String>) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let body = html! {
        h1 { "Settings" }
        p.sub { "Project ID " code { (p.id) } }
        div.card {
            h2 { "Project name" }
            form method="post" action=(format!("/app/p/{pid}/settings")) {
                input type="hidden" name="csrf" value=(u.csrf);
                div.row { input type="text" name="name" value=(p.name) style="flex:1;max-width:360px"; button type="submit" { "Save" } }
            }
        }
    };
    Html(layout(&st, &u, &p, Tab::Settings, "Settings", body)).into_response()
}

pub async fn rename_project(State(st): State<St>, u: WebUser, Path(pid): Path<String>, Form(form): Form<HashMap<String, String>>) -> Response {
    if let Err(r) = u.project(&st, &pid) {
        return r;
    }
    if let Err(r) = u.check_csrf(&form) {
        return r;
    }
    if let Some(n) = form.get("name").map(|s| s.trim()).filter(|s| !s.is_empty())
        && let Err(e) = st.meta.rename_project(&pid, &n.chars().take(80).collect::<String>()) {
            return internal(e);
        }
    Redirect::to(&format!("/app/p/{pid}/settings")).into_response()
}
