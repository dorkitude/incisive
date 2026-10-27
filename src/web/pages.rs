//! Page handlers and HTML templates.

use super::dashboard::{self, Report, Row, Totals};
use super::{cookie, internal, WebUser, PROJECT_COOKIE, TZ_COOKIE};
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

/// Changes whenever the stylesheet does, so browsers never keep a stale copy.
static STYLE_VERSION: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| {
    use sha2::{Digest, Sha256};
    hex::encode(&Sha256::digest(super::STYLE_CSS.as_bytes())[..6])
});

fn head(title: &str) -> Markup {
    html! {
        (DOCTYPE)
        html lang="en" {
            head {
                meta charset="utf-8";
                meta name="viewport" content="width=device-width, initial-scale=1";
                meta name="theme-color" content="#16161e";
                title { (title) " · Incisive" }
                link rel="stylesheet" href=(format!("/static/style.css?v={}", *STYLE_VERSION));
            }
        }
    }
}

/// Copy buttons (`data-copy` holds the text, `data-copy-from` names an element) and tab switchers.
const UI_JS: &str = r#"(function(){
function copy(text,btn){
 function done(){var t=btn.dataset.label||btn.textContent;btn.dataset.label=t;btn.textContent='Copied ✓';btn.classList.add('done');
  clearTimeout(btn._t);btn._t=setTimeout(function(){btn.textContent=t;btn.classList.remove('done');},1500);}
 function fallback(){var ta=document.createElement('textarea');ta.value=text;ta.style.position='fixed';ta.style.opacity='0';
  document.body.appendChild(ta);ta.select();try{document.execCommand('copy');done();}catch(e){}document.body.removeChild(ta);}
 if(navigator.clipboard&&window.isSecureContext){navigator.clipboard.writeText(text).then(done,fallback);}else{fallback();}
}
document.addEventListener('click',function(e){
 var b=e.target.closest('button.copy');
 if(b){var src=b.getAttribute('data-copy-from');copy(src?(document.getElementById(src)||{}).textContent||'':b.getAttribute('data-copy')||'',b);return;}
 var t=e.target.closest('[data-tabs] .tabs button');
 if(t){var g=t.closest('[data-tabs]');
  g.querySelectorAll('.tabs button').forEach(function(x){x.classList.toggle('on',x===t);});
  g.querySelectorAll('[data-panel]').forEach(function(p){p.hidden=p.getAttribute('data-panel')!==t.getAttribute('data-tab');});}
});
})();"#;

/// A one-click copy button for a literal value.
pub(super) fn copy_button(text: &str) -> Markup {
    html! { button.copy type="button" data-copy=(text) { "Copy" } }
}

/// A labeled code block with a copy button. `id` must be unique on the page.
pub(super) fn snippet(label: &str, id: &str, text: &str) -> Markup {
    html! {
        div.code {
            div.code-hd { span.l { (label) } button.copy type="button" data-copy-from=(id) { "Copy" } }
            pre id=(id) { (text) }
        }
    }
}

/// A small labeled value with a copy button.
pub(super) fn fact(label: &str, value: &str) -> Markup {
    html! { div.fact { div.l { (label) } div.v { code title=(value) { (value) } (copy_button(value)) } } }
}

pub(super) fn page_head(title: &str, sub: Markup, actions: Markup) -> Markup {
    html! { div.ph { div { h1 { (title) } p.sub { (sub) } } div.row { (actions) } } }
}

#[derive(PartialEq, Clone, Copy)]
pub(super) enum Tab {
    Dashboard,
    Builder,
    Stream,
    Collections,
    Pivot,
    Funnels,
    Install,
    Keys,
    Schema,
    Settings,
}

impl Tab {
    fn icon(self) -> PreEscaped<&'static str> {
        PreEscaped(match self {
            Tab::Dashboard => r#"<svg viewBox="0 0 24 24"><path d="M3 3v18h18M7 15l4-4 3 3 6-6"/></svg>"#,
            Tab::Builder => r#"<svg viewBox="0 0 24 24"><path d="M3 5h18M6 12h12M10 19h4"/></svg>"#,
            Tab::Stream => r#"<svg viewBox="0 0 24 24"><path d="M3 6h13M3 12h18M3 18h9"/><circle cx="20" cy="6" r="1.5"/></svg>"#,
            Tab::Collections => r#"<svg viewBox="0 0 24 24"><rect x="3" y="3" width="7" height="7" rx="1"/><rect x="14" y="3" width="7" height="7" rx="1"/><rect x="3" y="14" width="7" height="7" rx="1"/><rect x="14" y="14" width="7" height="7" rx="1"/></svg>"#,
            Tab::Pivot => r#"<svg viewBox="0 0 24 24"><rect x="3" y="3" width="18" height="18" rx="2"/><path d="M3 9h18M9 3v18"/></svg>"#,
            Tab::Funnels => r#"<svg viewBox="0 0 24 24"><path d="M3 4h18l-7 9v6l-4 2v-8L3 4z"/></svg>"#,
            Tab::Install => r#"<svg viewBox="0 0 24 24"><path d="M16 18l6-6-6-6M8 6l-6 6 6 6"/></svg>"#,
            Tab::Keys => r#"<svg viewBox="0 0 24 24"><circle cx="7.5" cy="15.5" r="4.5"/><path d="M10.7 12.3L21 2M17 6l3 3M14.5 8.5l2 2"/></svg>"#,
            Tab::Schema => r#"<svg viewBox="0 0 24 24"><path d="M12 2L2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5"/></svg>"#,
            Tab::Settings => r#"<svg viewBox="0 0 24 24"><path d="M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6"/></svg>"#,
        })
    }
}

pub(super) fn layout(st: &St, u: &WebUser, p: &Project, tab: Tab, title: &str, body: Markup) -> String {
    let projects = st.meta.projects_for(&u.user.id).unwrap_or_default();
    let base = format!("/app/p/{}", p.id);
    let t = |t: Tab, label: &str, href: String| html! { a.on[tab == t] href=(href) { (t.icon()) (label) } };
    html! {
        (head(title))
        body {
            div.shell {
                aside.side {
                    a.brand href="/app" { "incisive" span { "." } }
                    details.switcher {
                        summary { span { (p.name) } }
                        div.menu {
                            @for q in &projects {
                                a.on[q.id == p.id] href=(format!("/app/p/{}", q.id)) { (q.name) }
                            }
                            form method="post" action="/app/projects" {
                                input type="hidden" name="csrf" value=(u.csrf);
                                input type="text" name="name" placeholder="New project" required;
                                button.small type="submit" { "Add" }
                            }
                        }
                    }
                    nav.nav {
                        (t(Tab::Dashboard, "Web Analytics", base.clone()))
                    }
                    div.nav-label { "Explore " span { "5 drafts" } }
                    nav.nav {
                        (t(Tab::Builder, "Query builder", format!("{base}/explore")))
                        (t(Tab::Stream, "Event stream", format!("{base}/stream")))
                        (t(Tab::Collections, "Collections", format!("{base}/collections")))
                        (t(Tab::Pivot, "Pivot", format!("{base}/pivot")))
                        (t(Tab::Funnels, "Funnels", format!("{base}/funnels")))
                    }
                    div.nav-label { "Project" }
                    nav.nav {
                        (t(Tab::Install, "Install", format!("{base}/install")))
                        (t(Tab::Keys, "API keys", format!("{base}/keys")))
                        (t(Tab::Schema, "Schema", format!("{base}/schema")))
                        (t(Tab::Settings, "Settings", format!("{base}/settings")))
                    }
                    div.side-foot {
                        div.who title=(u.user.email) { (u.user.email) }
                        form.inline method="post" action="/logout" { button.ghost.small type="submit" { "Sign out" } }
                    }
                }
                main { (body) }
            }
            script { (PreEscaped(UI_JS)) }
        }
    }
    .into_string()
}

pub fn message_page(title: &str, msg: &str) -> String {
    html! {
        (head(title))
        body { div.center { div.login { div.card { h1 { (title) } p.sub { (msg) } a href="/app" { "← Back to the app" } } } } }
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
                    span.brand { "incisive" span { "." } }
                    p.sub { "Event analytics on object storage. Sign in or create an account. We'll email you a 6-digit code." }
                    div.card {
                        @if let Some(e) = error { div.notice.err { (e) } }
                        form method="post" action="/login" {
                            div.field {
                                label.f for="email" { "Email" }
                                input #email type="email" name="email" required autofocus autocomplete="email" placeholder="you@company.com";
                            }
                            button.block type="submit" { "Email me a sign-in code" }
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
            span.brand { "incisive" span { "." } }
            div.card {
                h1 { "Check your email ✉️" }
                p.sub { "We sent a 6-digit code to " b { (email) } ". It expires in 15 minutes." }
                @if let Some(e) = error { div.notice.err { (e) } }
                form method="post" action="/login/code" {
                    input type="hidden" name="email" value=(email);
                    div.field {
                        label.f for="code" { "Sign-in code" }
                        input.otp #code type="text" name="code" required autofocus inputmode="numeric" autocomplete="one-time-code"
                            pattern="[0-9 ]{6,7}" maxlength="7" placeholder="123456";
                    }
                    button.block type="submit" { "Sign in" }
                }
                form method="post" action="/login" style="margin-top:12px" {
                    input type="hidden" name="email" value=(email);
                    button.ghost.block type="submit" { "Send a new code" }
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

pub(super) fn fmt_int(n: u64) -> String {
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

pub(super) fn fmt_compact(n: f64) -> String {
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

pub(super) fn fmt_duration(secs: f64) -> String {
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

pub(super) fn nice_max(v: f64) -> f64 {
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

pub(super) fn local_label(zone: Zone, t: chrono::DateTime<chrono::Utc>, hourly: bool) -> String {
    let s = zone.format(t);
    match chrono::DateTime::parse_from_rfc3339(&s) {
        Ok(l) if hourly => l.format("%-I %p").to_string(),
        Ok(l) => l.format("%b %-d").to_string(),
        Err(_) => s,
    }
}

/// An area chart with a crosshair tooltip, for one series.
fn chart(r: &Report, m: Metric, zone: Zone, hourly: bool) -> Markup {
    let vals: Vec<Option<f64>> = r.series.iter().map(|b| m.value(&b.totals)).collect();
    let ticks: Vec<String> = r.series.iter().map(|b| local_label(zone, b.range.start, hourly)).collect();
    let tips: Vec<String> = r
        .series
        .iter()
        .zip(&vals)
        .map(|(b, v)| {
            let when = if hourly { format!("{} {}", local_label(zone, b.range.start, false), local_label(zone, b.range.start, true)) } else { local_label(zone, b.range.start, false) };
            format!("{when}\u{1f}{}", m.fmt(*v))
        })
        .collect();
    let axis = |v: f64| if m == Metric::Bounce { format!("{v:.0}%") } else if m == Metric::Duration { fmt_duration(v) } else { fmt_compact(v) };
    line_chart(&ticks, &tips, &vals, m.label(), &axis)
}

/// A one-series area chart with a crosshair tooltip. `tips` are "label\x1fvalue" per point.
pub(super) fn line_chart(ticks: &[String], tips: &[String], vals: &[Option<f64>], name: &str, axis: &dyn Fn(f64) -> String) -> Markup {
    let (w, h, left, bottom, top) = (1000.0, 260.0, 44.0, 26.0, 10.0);
    let max = nice_max(vals.iter().flatten().cloned().fold(0.0, f64::max));
    let n = vals.len().max(1);
    let x = |i: usize| left + (w - left) * if n == 1 { 0.5 } else { i as f64 / (n - 1) as f64 };
    let y = |v: f64| top + (h - top - bottom) * (1.0 - v / max);
    let pts: Vec<(f64, f64)> = vals.iter().enumerate().map(|(i, v)| (x(i), y(v.unwrap_or(0.0)))).collect();
    let line: String = pts.iter().enumerate().map(|(i, (px, py))| format!("{}{px:.1},{py:.1}", if i == 0 { "M" } else { "L" })).collect();
    let area = format!("{line}L{:.1},{:.1}L{:.1},{:.1}Z", pts.last().map_or(left, |p| p.0), y(0.0), pts.first().map_or(left, |p| p.0), y(0.0));
    let label_every = (n as f64 / 7.0).ceil().max(1.0) as usize;
    let data: Vec<Value> = tips.iter().map(|t| {
        let (l, v) = t.split_once('\u{1f}').unwrap_or((t, ""));
        json!({"label": l, "value": v})
    }).collect();
    html! {
        div.chart data-points=(serde_json::to_string(&pts).unwrap()) data-series=(serde_json::to_string(&data).unwrap()) data-name=(name) {
            svg viewBox=(format!("0 0 {w} {h}")) preserveAspectRatio="none" role="img" aria-label=(format!("{name} over time")) {
                g.grid {
                    @for k in 0..=4 {
                        @let gy = y(max * k as f64 / 4.0);
                        line x1=(left) x2=(w) y1=(gy) y2=(gy) {}
                    }
                }
                path.area d=(area) {}
                path.line d=(line) vector-effect="non-scaling-stroke" {}
                line.cross x1="0" x2="0" y1=(top) y2=(h - bottom) style="display:none" vector-effect="non-scaling-stroke" {}
            }
            // Axis labels in HTML so they do not stretch with the SVG.
            div.axis style="position:absolute;inset:0;pointer-events:none;font-size:11px;color:var(--faint)" {
                @for k in 0..=4 {
                    @let v = max * k as f64 / 4.0;
                    div style=(format!("position:absolute;left:0;top:calc({:.2}% - 8px)", y(v) / h * 100.0)) { (axis(v)) }
                }
                @for (i, t) in ticks.iter().enumerate() {
                    @if i % label_every == 0 {
                        div style=(format!("position:absolute;bottom:0;left:{:.2}%;transform:translateX(-50%);white-space:nowrap", x(i) / w * 100.0)) { (t) }
                    }
                }
            }
            div.dot style="display:none;position:absolute;width:10px;height:10px;border-radius:50%;background:var(--accent);border:2px solid var(--surface);box-shadow:0 0 0 4px var(--accent-soft);transform:translate(-50%,-50%)" {}
            div.tip style="display:none" {}
        }
        script { (PreEscaped(CHART_JS)) }
    }
}

/// Wires every chart on the page that is not wired yet.
const CHART_JS: &str = r#"(function(){
document.querySelectorAll('.chart[data-points]:not([data-on])').forEach(function(c){ c.setAttribute('data-on','1');
var pts=JSON.parse(c.dataset.points), s=JSON.parse(c.dataset.series), svg=c.querySelector('svg');
var cross=c.querySelector('.cross'), tip=c.querySelector('.tip'), dot=c.querySelector('.dot');
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
});
})();"#;

/// Sets the browser's timezone in a cookie and reloads once when it changed.
pub(super) const TZ_JS: &str = r#"(function(){try{var tz=Intl.DateTimeFormat().resolvedOptions().timeZone;
var m=document.cookie.match(/(?:^|; )incisive_tz=([^;]*)/); var cur=m?decodeURIComponent(m[1]):'';
if(tz&&tz!==cur){document.cookie='incisive_tz='+encodeURIComponent(tz)+'; path=/; max-age=31536000; SameSite=Lax'; if(!sessionStorage.getItem('tzr')){sessionStorage.setItem('tzr','1');location.reload();}}}catch(e){}})();"#;

pub(super) fn bars(title: &str, col: &str, unit: &str, rows: &[Row]) -> Markup {
    let max = rows.iter().map(|r| r.count).max().unwrap_or(1).max(1);
    html! {
        div.card {
            h2 { (title) }
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
    let (custom, interactions) = tokio::join!(custom_events(&st, &pid, tf, &tz_name), interactions(&st, &pid, tf, &tz_name));
    let base = format!("/app/p/{pid}");
    let href = |range: &str, metric: Metric| format!("{base}?range={range}&metric={}", metric.key());
    let empty = report.totals.pageviews == 0 && report.previous.pageviews == 0;
    let range_seg = html! {
        div.seg {
            @for (k, l) in [("today", "Today"), ("24h", "24h"), ("7d", "7 days"), ("30d", "30 days"), ("90d", "90 days")] {
                a.on[k == range_key] href=(href(k, metric)) { (l) }
            }
        }
    };
    let body = html! {
        (page_head("Web Analytics", html! { span.live { i {} (fmt_int(live)) " visitors in the last 30 minutes" } }, range_seg))
        @if empty {
            div.notice {
                "No pageviews yet. " a href=(format!("{base}/install")) { "Install the tracker" } " on your site and visits will appear here within seconds."
            }
        }
        div.card {
            div.kpis {
                @for m in [Metric::Visitors, Metric::Pageviews, Metric::Sessions, Metric::Bounce, Metric::Duration] {
                    (kpi(m, m == metric, &report, href(range_key, m)))
                }
            }
            (chart(&report, metric, zone, hourly))
            p.footnote {
                "Times in " (tz_name) " · Pages per session "
                (report.totals.pages_per_session().map(|x| format!("{x:.2}")).unwrap_or("–".into()))
                @if let Some(e) = interactions.engaged_seconds { " · Avg. engaged time per page " (fmt_duration(e)) }
                @if let Some(sc) = interactions.scroll { " · Avg. scroll depth " (format!("{sc:.0}%")) }
                @if report.bots_excluded > 0 { " · " (fmt_int(report.bots_excluded)) " bot pageviews excluded" }
            }
        }
        h3.section { "Content" }
        div.grid.g2 {
            (bars("Top pages", "Page", "Pageviews", &report.pages))
            (bars("Landing pages", "Page", "Sessions", &report.landing))
        }
        h3.section { "Acquisition" }
        div.grid.g2 {
            (bars("Channels", "Channel", "Sessions", &report.channels))
            (bars("Sources", "Source", "Sessions", &report.sources))
            (bars("Campaigns", "utm_campaign", "Sessions", &report.campaigns))
            (bars("Countries", "Country", "Sessions", &report.countries))
        }
        h3.section { "Technology" }
        div.grid.g3 {
            (bars("Devices", "Device", "Sessions", &report.devices))
            (bars("Browsers", "Browser", "Sessions", &report.browsers))
            (bars("Operating systems", "OS", "Sessions", &report.oses))
        }
        h3.section { "Interactions" }
        div.grid.g2 {
            (bars("Clicks", "Element", "Clicks", &interactions.clicks))
            (bars("Outbound links", "Destination", "Clicks", &interactions.outbound))
            (bars("Form submissions", "Page", "Submissions", &interactions.forms))
            (bars("Custom events", "Collection", "Events", &custom))
        }
        script { (PreEscaped(TZ_JS)) }
    };
    let remember = format!("{PROJECT_COOKIE}={pid}; Path=/; HttpOnly; SameSite=Lax; Max-Age=34560000{}", if st.cfg.server.secure_cookies { "; Secure" } else { "" });
    ([(axum::http::header::SET_COOKIE, remember)], Html(layout(&st, &u, &p, Tab::Dashboard, "Web Analytics", body))).into_response()
}

#[derive(Default)]
struct Interactions {
    clicks: Vec<Row>,
    outbound: Vec<Row>,
    forms: Vec<Row>,
    engaged_seconds: Option<f64>,
    scroll: Option<f64>,
}

/// Breakdowns of the auto-tracked interaction collections.
async fn interactions(st: &St, pid: &str, tf: &str, tz: &str) -> Interactions {
    let ex = crate::query::exec::Executor::new(&st.engine, st.query_limits(), chrono::Utc::now());
    let run = |analysis: &'static str, m: Value| {
        let ex = &ex;
        async move {
            let mut m = m;
            m["timeframe"] = json!(tf);
            m["timezone"] = json!(tz);
            let q = crate::query::parse(analysis, m.as_object()?).ok()?;
            ex.run(pid, &q, &[]).await.ok().and_then(|mut o| o.remove("result"))
        }
    };
    let top = |res: Option<Value>, label: &str| -> Vec<Row> {
        let mut rows: Vec<Row> = res
            .and_then(|v| v.as_array().cloned())
            .unwrap_or_default()
            .into_iter()
            .filter_map(|r| {
                let l = r.get(label)?.as_str()?.to_string();
                Some(Row { label: if l.is_empty() { "(no text)".into() } else { l }, count: r.get("result")?.as_u64()?, visitors: 0 })
            })
            .collect();
        rows.sort_by(|a, b| b.count.cmp(&a.count));
        rows.truncate(10);
        rows
    };
    let (clicks, outbound, forms, engaged, scroll) = tokio::join!(
        run("count", json!({"event_collection": standard::CLICK, "group_by": "element.text",
            "filters": [{"property_name": "link.is_outbound", "operator": "ne", "property_value": true}]})),
        run("count", json!({"event_collection": standard::CLICK, "group_by": "link.hostname",
            "filters": [{"property_name": "link.is_outbound", "operator": "eq", "property_value": true}]})),
        run("count", json!({"event_collection": standard::FORM_SUBMIT, "group_by": "page.path"})),
        run("average", json!({"event_collection": standard::ENGAGEMENT, "target_property": "engagement.engaged_seconds"})),
        run("average", json!({"event_collection": standard::ENGAGEMENT, "target_property": "engagement.max_scroll_percent"})),
    );
    Interactions {
        clicks: top(clicks, "element.text"),
        outbound: top(outbound, "link.hostname"),
        forms: top(forms, "page.path"),
        engaged_seconds: engaged.and_then(|v| v.as_f64()),
        scroll: scroll.and_then(|v| v.as_f64()),
    }
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
    html! { div.secret { code { (secret) } (copy_button(secret)) } }
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
        (page_head("API keys", html! { "Send a key in the " code { "Authorization" } " header (or as " code { "api_key" } "). Custom keys can be limited to specific operations, stamp properties onto every event they write, and filter every query they run." }, html! {}))
        @if let Some((k, secret)) = created {
            div.notice.ok {
                b { "New key “" (k.name) "” created." }
                @if k.secret.is_none() { " Copy it now: it won't be shown again." }
                div style="margin-top:8px" { (copyable(secret)) }
            }
        }
        @if let Some(e) = error { div.notice.err { (e) } }
        div.card style="margin-bottom:16px;padding:8px 12px" {
            div.table-wrap {
                table {
                    thead { tr { th { "Name" } th { "Key" } th { "Permissions" } th { "Status" } th { "Last used" } th {} } }
                    tbody {
                        @for k in &keys {
                            tr {
                                td style="white-space:nowrap" { b { (k.name) } }
                                td {
                                    div.row style="gap:8px;flex-wrap:nowrap" {
                                        span.mono { (k.hint) }
                                        @if let Some(s) = &k.secret { (copy_button(s)) }
                                    }
                                }
                                td { (perm_pills(k)) @if k.options != KeyOptions::default() { details { summary { "options" } pre { (serde_json::to_string_pretty(&k.options).unwrap_or_default()) } } } }
                                td { @if k.is_active { span.pill.ok { "active" } } @else { span.pill.off { "revoked" } } }
                                td.muted style="white-space:nowrap" title=(k.last_used_at.map(crate::api::iso).unwrap_or_default()) {
                                    (k.last_used_at.and_then(|t| chrono::DateTime::from_timestamp(t, 0)).map(|t| t.format("%b %-d, %H:%M UTC").to_string()).unwrap_or_else(|| "never".into()))
                                }
                                td style="white-space:nowrap" {
                                    @if !k.master {
                                        @for (action, label, cls) in [(if k.is_active { "revoke" } else { "unrevoke" }, if k.is_active { "Revoke" } else { "Restore" }, "ghost"), ("delete", "Delete", "danger")] {
                                            form.inline method="post" action=(format!("{base}/{}/{action}", k.id)) {
                                                input type="hidden" name="csrf" value=(u.csrf);
                                                button class=(format!("small {cls}")) type="submit" { (label) }
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
            div.card-hd { h2 { "Create a key" } }
            form method="post" action=(base) {
                input type="hidden" name="csrf" value=(u.csrf);
                div.field style="max-width:420px" { label.f { "Name" } input type="text" name="name" required placeholder="e.g. Marketing site, Customer 42 dashboard"; }
                div.field {
                    label.f { "Permissions" }
                    div.checks {
                        @for (perm, doc) in PERM_DOCS {
                            label {
                                input type="checkbox" name=(format!("p_{perm}")) value="1" checked[perm == "writes"];
                                code style="min-width:120px" { (perm) } span.muted { (doc) }
                            }
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
    let tracker = if st.cfg.server.tracker_url.is_empty() { format!("{public}/js/incisive.js") } else { st.cfg.server.tracker_url.clone() };
    let keys = write_keys(&st, &pid);
    let api = format!("{public}/3.0/projects/{}", p.id);
    let body = html! {
        (page_head("Install", html! { "Start sending events: paste the tracker into your site, drop in a pixel, or call the API from your server." }, html! {}))
        @if keys.is_empty() {
            div.notice {
                "You need a write-only key to track. "
                form.inline method="post" action=(format!("/app/p/{pid}/install")) {
                    input type="hidden" name="csrf" value=(u.csrf);
                    button.small type="submit" { "Create a write key" }
                }
            }
        } @else {
            @let key = keys[0].secret.clone().unwrap_or_default();
            div.facts style="margin-bottom:16px" {
                (fact("Project ID", &p.id))
                (fact("Write key (public)", &key))
                (fact("API host", &public))
            }
            div.card style="margin-bottom:16px" data-tabs {
                div.card-hd {
                    h2 { span.step { "1" } "Website tracker" }
                    div.tabs { button.on type="button" data-tab="tag" { "Script tag" } button type="button" data-tab="js" { "trackAll()" } }
                }
                p.muted { "Auto-tracks sessions, pageviews (including single-page-app navigation), clicks with outbound-link and download detection, form submissions, engaged time and scroll depth. Every event carries the full context (visitor, session, campaign, channel, device, browser, location), so each collection answers questions on its own." }
                div data-panel="tag" {
                    (snippet("Paste before </head> on every page", "snip-tag", &format!("<script defer src=\"{tracker}\"\n  data-project=\"{}\"\n  data-key=\"{key}\"\n  data-host=\"{public}\"></script>", p.id)))
                }
                div data-panel="js" hidden {
                    (snippet("Start it from your own code (every flag is optional; the page load is always tracked)", "snip-js", &format!("<script src=\"{tracker}\"></script>\n<script>\n  incisive.trackAll({{\n    project: \"{}\",\n    key: \"{key}\",\n    host: \"{public}\",\n    clicks: true, forms: true, engagement: true, views: true, errors: false,\n    // Optional: properties added to every event. These are placeholders; a function is called on each event.\n    // properties: {{\n    //   app: {{ version: \"2.3.1\" }},\n    //   cart: {{ items: function () {{ return window.cart ? window.cart.length : 0; }} }}\n    // }}\n  }});\n</script>", p.id)))
                }
                (snippet("Custom events and signed-in users", "snip-api", "incisive.track(\"signup\", {plan: \"pro\"});\nincisive.identify(\"user-42\", {plan: \"pro\"});\nincisive.extend({experiment: {variant: \"b\"}});  // add to every later event"))
                details.disclose {
                    summary { "Options" }
                    dl.opts style="margin-top:12px" {
                        dt { code { "data-auto-track" } } dd { "Comma list of " code { "pageviews,clicks,forms,engagement,views,errors" } ", or " code { "false" } " for none. All but errors are on by default." }
                        dt { code { "data-track=\"name\"" } } dd { "Names a click target, e.g. " code { "signup-cta" } "." }
                        dt { code { "data-track-view" } } dd { "Records a " code { "web_element_view" } " when the element scrolls into view." }
                        dt { code { "data-incisive-ignore" } } dd { "Never track clicks on this element or its children." }
                        dt { code { "data-spa" } } dd { code { "false" } " turns off single-page-app navigation tracking." }
                        dt { code { "data-respect-dnt" } } dd { code { "true" } " skips visitors with Do Not Track on." }
                    }
                    p.muted.small style="margin:12px 0 0" { "Form field values are never sent." }
                }
            }
            div.card style="margin-bottom:16px" {
                div.card-hd { h2 { span.step { "2" } "Tracking pixel" } }
                p.muted { "A 1×1 image that records an event when it loads: for emails, or anywhere scripts can't run. The event is base64-encoded JSON in the URL." }
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
                        (snippet("Image tag", "px-tag", ""))
                        (snippet("URL", "px-url", ""))
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
                    serde_json::to_string(&api).unwrap()))) }
            }
            div.card {
                div.card-hd { h2 { span.step { "3" } "Server-side and other SDKs" } }
                p.muted { "Any client that speaks the event API works: point it at " code { (public) } " with project ID " code { (p.id) } " and a key from " a href=(format!("/app/p/{pid}/keys")) { "API keys" } "." }
                (snippet("Send an event with curl", "snip-curl", &format!("curl -X POST {api}/events/purchases \\\n  -H 'Authorization: {key}' -H 'Content-Type: application/json' \\\n  -d '{{\"item\": {{\"name\": \"hat\", \"price\": 25}}, \"customer\": {{\"id\": \"c-1\"}}}}'")))
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
        (page_head("Schema", html! { "Events are schemaless JSON. The web collections have a standard schema that powers the dashboard: the tracker sends these properties and the server derives the rest. Add any others you like." }, html! {}))
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
                                td.check { @if f.pageview { "✓" } }
                                td.check { @if f.session { "✓" } }
                                td { (f.doc) }
                            }
                        }
                    }
                }
            }
        }
        div.card style="margin-bottom:16px" {
            h2 { "Auto-tracked collections" }
            p.muted { "Each carries every context property above (visitor, session, page, device, browser, location), plus its own:" }
            ul { @for (c, doc) in standard::AUTO_COLLECTIONS { li { code { (c) } " — " (doc) } } }
            div.table-wrap {
                table {
                    thead { tr { th { "Property" } th { "Type" } th { "Collections" } th { "Description" } } }
                    tbody {
                        @for (path, kind, colls, doc) in standard::AUTO_FIELDS {
                            tr { td.mono { (path) } td.muted { (format!("{kind:?}").to_lowercase()) } td.mono { (colls) } td { (doc) } }
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
                    summary { code { (coll) } span.muted { " · " (props.len()) " properties" } }
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
    let public = st.cfg.server.public_url.trim_end_matches('/').to_string();
    let body = html! {
        (page_head("Settings", html! { "Project details and name." }, html! {}))
        div.card style="margin-bottom:16px" {
            div.card-hd { h2 { "Project" } }
            div.facts {
                (fact("Project ID", &p.id))
                (fact("API base", &format!("{public}/3.0/projects/{}", p.id)))
            }
        }
        div.card {
            div.card-hd { h2 { "Project name" } }
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
