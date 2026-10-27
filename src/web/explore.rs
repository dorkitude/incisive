//! Five drafts of a general-purpose explorer for any collection, each on its own page:
//! a query builder, a live event stream, a collection and property profiler, a pivot
//! heatmap, and a funnel builder. They share the helpers at the top of this file.

use super::pages::{fmt_compact, fmt_int, layout, line_chart, page_head, snippet, Tab, TZ_JS};
use super::{cookie, internal, WebUser, TZ_COOKIE};
use crate::api::St;
use crate::meta::Project;
use crate::query::exec::Executor;
use crate::standard;
use axum::extract::{Path, State};
use axum::http::{HeaderMap, Uri};
use axum::response::{Html, IntoResponse, Redirect, Response};
use axum::Form;
use futures::stream::{self, StreamExt};
use maud::{html, Markup, PreEscaped};
use serde_json::{json, Map, Value};
use std::collections::{BTreeMap, HashMap};

type Schema = BTreeMap<String, BTreeMap<String, String>>;

// ---- shared ----

/// (key, label, timeframe, natural interval)
const RANGES: [(&str, &str, &str, &str); 5] = [
    ("24h", "24 hours", "this_24_hours", "hourly"),
    ("7d", "7 days", "this_7_days", "daily"),
    ("30d", "30 days", "this_30_days", "daily"),
    ("90d", "90 days", "this_90_days", "weekly"),
    ("1y", "12 months", "this_12_months", "monthly"),
];

const BUILDER_ANALYSES: [(&str, &str); 10] = [
    ("count", "Count"),
    ("count_unique", "Count unique"),
    ("sum", "Sum"),
    ("average", "Average"),
    ("minimum", "Minimum"),
    ("maximum", "Maximum"),
    ("median", "Median"),
    ("percentile", "Percentile"),
    ("select_unique", "Select unique"),
    ("extraction", "Extraction (raw events)"),
];

const OPS: [(&str, &str); 10] = [
    ("eq", "="),
    ("ne", "≠"),
    ("gt", ">"),
    ("gte", "≥"),
    ("lt", "<"),
    ("lte", "≤"),
    ("contains", "contains"),
    ("not_contains", "does not contain"),
    ("in", "in (comma list)"),
    ("exists", "exists (true/false)"),
];

fn params(uri: &Uri) -> Vec<(String, String)> {
    url::form_urlencoded::parse(uri.query().unwrap_or("").as_bytes()).into_owned().collect()
}

fn get<'a>(ps: &'a [(String, String)], k: &str) -> Option<&'a str> {
    ps.iter().find(|(a, b)| a == k && !b.trim().is_empty()).map(|(_, b)| b.trim())
}

fn all<'a>(ps: &'a [(String, String)], k: &str) -> Vec<&'a str> {
    ps.iter().filter(|(a, _)| a == k).map(|(_, b)| b.trim()).collect()
}

fn range(ps: &[(String, String)]) -> (&'static str, &'static str, &'static str) {
    let k = get(ps, "range").unwrap_or("7d");
    let r = RANGES.iter().find(|r| r.0 == k).unwrap_or(&RANGES[1]);
    (r.0, r.2, r.3)
}

fn tz(headers: &HeaderMap) -> String {
    cookie(headers, TZ_COOKIE).unwrap_or_else(|| "UTC".into())
}

fn needs_target(a: &str) -> bool {
    crate::query::needs_target(a)
}

fn is_numeric(kind: &str) -> bool {
    kind.starts_with("num") || kind == "float" || kind == "int" || kind == "integer"
}

/// Runs one query as the signed-in member (no key scope) and returns its `result`.
async fn run(st: &St, pid: &str, analysis: &str, mut m: Value, tz: &str) -> Result<Value, String> {
    m["timezone"] = json!(tz);
    let q = crate::query::parse(analysis, m.as_object().ok_or("bad query")?).map_err(|e| e.to_string())?;
    let ex = Executor::new(&st.engine, st.query_limits(), chrono::Utc::now());
    let timeout = std::time::Duration::from_secs(st.cfg.limits.query_timeout_secs.max(1));
    match tokio::time::timeout(timeout, ex.run(pid, &q, &[])).await {
        Ok(Ok(mut out)) => Ok(out.remove("result").unwrap_or(Value::Null)),
        Ok(Err(e)) => Err(e.to_string()),
        Err(_) => Err("The query took too long. Narrow the timeframe or add filters.".into()),
    }
}

/// A filter value typed by the property's observed type.
fn typed(v: &str, kind: Option<&str>, op: &str) -> Value {
    let one = |s: &str| -> Value {
        match kind {
            Some(k) if is_numeric(k) => s.parse::<f64>().ok().map(Value::from).unwrap_or_else(|| json!(s)),
            Some(k) if k.starts_with("bool") => json!(s == "true"),
            Some(_) => json!(s),
            None => match s {
                "true" => json!(true),
                "false" => json!(false),
                _ => s.parse::<f64>().ok().map(Value::from).unwrap_or_else(|| json!(s)),
            },
        }
    };
    match op {
        "exists" => json!(v != "false"),
        "in" => Value::Array(v.split(',').map(|x| one(x.trim())).collect()),
        _ => one(v),
    }
}

fn filters_from(ps: &[(String, String)], props: &BTreeMap<String, String>, p: &str, o: &str, v: &str) -> Vec<Value> {
    let (fp, fo, fv) = (all(ps, p), all(ps, o), all(ps, v));
    fp.iter()
        .enumerate()
        .filter(|(_, p)| !p.is_empty())
        .map(|(i, prop)| {
            let op = fo.get(i).copied().filter(|o| !o.is_empty()).unwrap_or("eq");
            let val = fv.get(i).copied().unwrap_or("");
            json!({"property_name": prop, "operator": op, "property_value": typed(val, props.get(*prop).map(String::as_str), op)})
        })
        .collect()
}

fn fmt_num(x: f64) -> String {
    if x.fract() == 0.0 && x.abs() < 1e15 {
        let s = fmt_int(x.abs() as u64);
        if x < 0.0 { format!("-{s}") } else { s }
    } else if x.abs() >= 100.0 {
        format!("{x:.1}")
    } else {
        format!("{x:.2}")
    }
}

fn fmt_val(v: &Value) -> String {
    match v {
        Value::Null => "(none)".into(),
        Value::Number(n) => n.as_f64().map(fmt_num).unwrap_or_else(|| n.to_string()),
        Value::String(s) if s.is_empty() => "(empty)".into(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

fn coll_select(name: &str, schema: &Schema, selected: Option<&str>, submit: bool) -> Markup {
    html! {
        select name=(name) onchange=[submit.then_some("this.form.submit()")] {
            @for c in schema.keys() { option value=(c) selected[Some(c.as_str()) == selected] { (c) } }
        }
    }
}

fn prop_select(name: &str, props: &BTreeMap<String, String>, selected: Option<&str>, none: Option<&str>) -> Markup {
    html! {
        select name=(name) {
            @if let Some(n) = none { option value="" { (n) } }
            @for (p, kind) in props { option value=(p) selected[Some(p.as_str()) == selected] { (p) " · " (kind) } }
        }
    }
}

fn range_select(selected: &str) -> Markup {
    html! { select name="range" { @for (k, l, _, _) in RANGES { option value=(k) selected[k == selected] { "Last " (l) } } } }
}

fn props_of(schema: &Schema, coll: Option<&str>) -> BTreeMap<String, String> {
    coll.and_then(|c| schema.get(c)).cloned().unwrap_or_default().into_iter().filter(|(p, _)| !p.starts_with("keen.")).collect()
}

fn page(st: &St, u: &WebUser, p: &Project, tab: Tab, title: &str, body: Markup) -> Response {
    let body = html! { (body) script { (PreEscaped(EXPLORE_JS)) } script { (PreEscaped(TZ_JS)) } };
    Html(layout(st, u, p, tab, title, body)).into_response()
}

fn no_events(pid: &str) -> Markup {
    html! { div.notice { "No events yet. " a href=(format!("/app/p/{pid}/install")) { "Send some" } " and they will show up here." } }
}

/// "+ add" buttons clone the last `.rep` row in their group and clear its inputs.
const EXPLORE_JS: &str = r#"(function(){
document.addEventListener('click',function(e){
 var b=e.target.closest('[data-add]'); if(b){ var g=b.closest('[data-rows]'); var rows=g.querySelectorAll('.rep'); var last=rows[rows.length-1];
  var n=last.cloneNode(true); n.querySelectorAll('input').forEach(function(i){i.value='';}); n.querySelectorAll('select').forEach(function(s){s.selectedIndex=0;});
  last.parentNode.insertBefore(n,last.nextSibling); return; }
 var r=e.target.closest('[data-remove]'); if(r){ var row=r.closest('.rep'); var g2=row.closest('[data-rows]');
  if(g2.querySelectorAll('.rep').length>1) row.remove(); else { row.querySelectorAll('input').forEach(function(i){i.value='';}); row.querySelectorAll('select').forEach(function(s){s.selectedIndex=0;}); } }
});
var a=document.querySelector('select[name=analysis]'); function sync(){ if(!a) return; var v=a.value;
 document.querySelectorAll('[data-show]').forEach(function(el){ el.hidden=el.getAttribute('data-show').split(' ').indexOf(v)<0; }); }
if(a){ a.addEventListener('change',sync); sync(); }
})();"#;

/// Horizontal bars for float-valued groups.
fn value_bars(title: &str, col: &str, unit: &str, rows: &[(String, f64)]) -> Markup {
    let max = rows.iter().map(|r| r.1).fold(0.0, f64::max).max(f64::MIN_POSITIVE);
    html! {
        div.card {
            @if !title.is_empty() { h2 { (title) } }
            @if rows.is_empty() { div.empty { "No results" } } @else {
                div.bars {
                    div.hd { span { (col) } span { (unit) } }
                    @for (l, v) in rows {
                        div.r title=(format!("{l}: {}", fmt_num(*v))) {
                            div.fill style=(format!("width:{:.1}%", (v / max * 100.0).max(0.0))) {}
                            span { (l) }
                            span.n { (fmt_num(*v)) }
                        }
                    }
                }
            }
        }
    }
}

fn tf_label(v: &Value, interval: &str) -> String {
    let s = v.pointer("/timeframe/start").and_then(Value::as_str).unwrap_or("");
    match chrono::DateTime::parse_from_rfc3339(s) {
        Ok(t) => match interval {
            "hourly" => t.format("%b %-d %-I %p").to_string(),
            "monthly" => t.format("%b %Y").to_string(),
            _ => t.format("%b %-d").to_string(),
        },
        Err(_) => s.to_string(),
    }
}

fn tick_label(v: &Value, interval: &str) -> String {
    let s = v.pointer("/timeframe/start").and_then(Value::as_str).unwrap_or("");
    match chrono::DateTime::parse_from_rfc3339(s) {
        Ok(t) if interval == "hourly" => t.format("%-I %p").to_string(),
        Ok(t) if interval == "monthly" => t.format("%b").to_string(),
        Ok(t) => t.format("%b %-d").to_string(),
        Err(_) => s.to_string(),
    }
}

/// Renders any analysis result in the form that suits its shape.
fn render_result(result: &Value, analysis: &str, label: &str, group: Option<&str>, interval: &str) -> Markup {
    match result {
        Value::Array(items) if items.is_empty() => html! { div.card { div.empty { "No matching events in this timeframe." } } },
        Value::Array(items) if analysis == "extraction" => event_table(items),
        Value::Array(items) if analysis == "select_unique" => html! {
            div.card {
                h2 { (fmt_int(items.len() as u64)) " unique values" }
                div { @for v in items { span.pill { (fmt_val(v)) } } }
            }
        },
        Value::Array(items) if items[0].get("timeframe").is_some() => {
            let grouped = items.iter().any(|i| i.get("value").is_some_and(Value::is_array));
            if grouped {
                interval_groups(items, group.unwrap_or("group"), interval)
            } else {
                let vals: Vec<Option<f64>> = items.iter().map(|i| i.get("value").and_then(Value::as_f64)).collect();
                let ticks: Vec<String> = items.iter().map(|i| tick_label(i, interval)).collect();
                let tips: Vec<String> = items.iter().map(|i| format!("{}\u{1f}{}", tf_label(i, interval), fmt_val(i.get("value").unwrap_or(&Value::Null)))).collect();
                let total: f64 = vals.iter().flatten().sum();
                html! {
                    div.card {
                        div.card-hd { h2 { (label) } @if analysis == "count" || analysis == "sum" { span.muted { "Total " b { (fmt_num(total)) } } } }
                        (line_chart(&ticks, &tips, &vals, label, &|v| fmt_compact(v)))
                    }
                }
            }
        }
        Value::Array(items) => {
            let g = group.unwrap_or("group");
            let rows: Vec<(String, f64)> = items.iter().map(|i| (fmt_val(i.get(g).unwrap_or(&Value::Null)), i.get("result").and_then(Value::as_f64).unwrap_or(0.0))).collect();
            let mut rows = rows;
            rows.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            let shown = rows.len().min(50);
            html! {
                (value_bars(label, g, "Result", &rows[..shown]))
                @if rows.len() > shown { p.footnote { "Showing the top " (shown) " of " (fmt_int(rows.len() as u64)) " groups." } }
            }
        }
        v => html! {
            div.card.stat {
                div.l { (label) }
                div.v { (fmt_val(v)) }
            }
        },
    }
}

/// An interval × group table, with the busiest groups as columns.
fn interval_groups(items: &[Value], g: &str, interval: &str) -> Markup {
    let mut totals: HashMap<String, f64> = HashMap::new();
    for i in items {
        for r in i.get("value").and_then(Value::as_array).into_iter().flatten() {
            *totals.entry(fmt_val(r.get(g).unwrap_or(&Value::Null))).or_default() += r.get("result").and_then(Value::as_f64).unwrap_or(0.0);
        }
    }
    let mut cols: Vec<(String, f64)> = totals.into_iter().collect();
    cols.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    cols.truncate(8);
    let max = items
        .iter()
        .flat_map(|i| i.get("value").and_then(Value::as_array).into_iter().flatten())
        .filter_map(|r| r.get("result").and_then(Value::as_f64))
        .fold(0.0, f64::max)
        .max(f64::MIN_POSITIVE);
    html! {
        div.card {
            div.table-wrap {
                table.heat {
                    thead { tr { th { "Period" } @for (c, _) in &cols { th.num { (c) } } } }
                    tbody {
                        @for i in items {
                            @let cells: HashMap<String, f64> = i.get("value").and_then(Value::as_array).into_iter().flatten()
                                .map(|r| (fmt_val(r.get(g).unwrap_or(&Value::Null)), r.get("result").and_then(Value::as_f64).unwrap_or(0.0))).collect();
                            tr {
                                td { (tf_label(i, interval)) }
                                @for (c, _) in &cols {
                                    @let v = cells.get(c).copied().unwrap_or(0.0);
                                    td.num style=(heat(v, max)) { (fmt_num(v)) }
                                }
                            }
                        }
                    }
                }
            }
            p.footnote { "Columns are the " (cols.len()) " largest values of " code { (g) } "." }
        }
    }
}

fn heat(v: f64, max: f64) -> String {
    if v <= 0.0 {
        return String::new();
    }
    format!("background:rgba(122,162,247,{:.3})", 0.06 + 0.5 * (v / max).min(1.0))
}

/// A short, human summary of an event for one-line lists.
fn summary(ev: &Value) -> String {
    let pick = |p: &str| ev.pointer(p).filter(|v| !v.is_null()).map(fmt_val);
    let known = [
        "/page/path", "/element/track_name", "/element/text", "/link/hostname", "/form/id", "/error/message", "/engagement/engaged_seconds",
    ];
    let mut parts: Vec<String> = known.iter().filter_map(|p| pick(p).map(|v| format!("{}: {v}", p.trim_start_matches('/').replace('/', ".")))).collect();
    if parts.is_empty() {
        let mut leaves = vec![];
        leaf_paths(ev, "", &mut leaves);
        parts = leaves.into_iter().filter(|(p, _)| !p.starts_with("keen.")).take(4).map(|(p, v)| format!("{p}: {v}")).collect();
    }
    parts.truncate(4);
    parts.join(" · ")
}

fn leaf_paths(v: &Value, prefix: &str, out: &mut Vec<(String, String)>) {
    if let Value::Object(m) = v {
        for (k, x) in m {
            let p = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
            match x {
                Value::Object(_) => leaf_paths(x, &p, out),
                _ => out.push((p, fmt_val(x))),
            }
        }
    }
}

fn event_time(ev: &Value) -> String {
    ev.pointer("/keen/timestamp").and_then(Value::as_str).and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok()).map(|t| t.format("%b %-d %H:%M:%S").to_string()).unwrap_or_default()
}

fn event_table(items: &[Value]) -> Markup {
    html! {
        div.card style="padding:8px 12px" {
            div.feed {
                @for (i, ev) in items.iter().rev().enumerate() {
                    (event_row(ev, None, &format!("ev{i}")))
                }
            }
        }
    }
}

fn event_row(ev: &Value, coll: Option<&str>, id: &str) -> Markup {
    let pretty = serde_json::to_string_pretty(ev).unwrap_or_default();
    html! {
        details.ev {
            summary {
                span.t { (event_time(ev)) }
                @if let Some(c) = coll { span.pill.on { (c) } }
                span.s { (summary(ev)) }
            }
            div.code style="margin:8px 0 4px" {
                div.code-hd { span.l { "Event JSON" } button.copy type="button" data-copy-from=(id) { "Copy" } }
                pre id=(id) { (pretty) }
            }
        }
    }
}

// ---- 1. Query builder ----

/// The builder's query from its URL parameters: (analysis, query body, group property).
fn builder_query(ps: &[(String, String)], schema: &Schema) -> (String, Option<String>, Map<String, Value>, Option<String>, String) {
    let coll = get(ps, "collection").filter(|c| schema.contains_key(*c)).map(String::from).or_else(|| {
        schema.keys().find(|c| !standard::is_standard(c)).or(schema.keys().next()).cloned()
    });
    let props = props_of(schema, coll.as_deref());
    let analysis = get(ps, "analysis").filter(|a| BUILDER_ANALYSES.iter().any(|x| x.0 == *a)).unwrap_or("count").to_string();
    let (_, tf, _) = range(ps);
    let mut m = Map::new();
    m.insert("event_collection".into(), json!(coll));
    m.insert("timeframe".into(), json!(tf));
    m.insert("filters".into(), Value::Array(filters_from(ps, &props, "fp", "fo", "fv")));
    if needs_target(&analysis) {
        let t = get(ps, "target").map(String::from).or_else(|| props.iter().find(|(_, k)| is_numeric(k)).or(props.iter().next()).map(|(p, _)| p.clone()));
        m.insert("target_property".into(), json!(t));
    }
    if analysis == "percentile" {
        m.insert("percentile".into(), json!(get(ps, "percentile").and_then(|p| p.parse::<f64>().ok()).unwrap_or(90.0)));
    }
    let mut group = None;
    let mut interval = String::new();
    if analysis == "extraction" {
        m.insert("latest".into(), json!(100));
    } else {
        if let Some(g) = get(ps, "group_by") {
            m.insert("group_by".into(), json!(g));
            group = Some(g.to_string());
        }
        if let Some(i) = get(ps, "interval").filter(|i| ["hourly", "daily", "weekly", "monthly"].contains(i)) {
            m.insert("interval".into(), json!(i));
            interval = i.to_string();
        }
    }
    (analysis, coll, m, group, interval)
}

pub async fn builder(State(st): State<St>, u: WebUser, Path(pid): Path<String>, uri: Uri, headers: HeaderMap) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let schema = st.engine.schema(&pid).await.unwrap_or_default();
    let ps = params(&uri);
    let tz = tz(&headers);
    let (analysis, coll, m, group, interval) = builder_query(&ps, &schema);
    let props = props_of(&schema, coll.as_deref());
    let (rk, _, _) = range(&ps);
    let result = match &coll {
        Some(_) => Some(run(&st, &pid, &analysis, Value::Object(m.clone()), &tz).await),
        None => None,
    };
    let saved = st.meta.saved_queries(&pid).unwrap_or_default();
    let public = st.cfg.server.public_url.trim_end_matches('/').to_string();
    let target = m.get("target_property").and_then(Value::as_str);
    let label = {
        let a = BUILDER_ANALYSES.iter().find(|x| x.0 == analysis).map(|x| x.1).unwrap_or("Count");
        match target {
            Some(t) => format!("{a} of {t}"),
            None => format!("{a} of {}", coll.as_deref().unwrap_or("events")),
        }
    };
    let label = match &group {
        Some(g) if analysis != "extraction" => format!("{label} by {g}"),
        _ => label,
    };
    let mut filter_rows: Vec<(&str, &str, &str)> = all(&ps, "fp").into_iter().enumerate().filter(|(_, p)| !p.is_empty())
        .map(|(i, p)| (p, all(&ps, "fo").get(i).copied().unwrap_or("eq"), all(&ps, "fv").get(i).copied().unwrap_or(""))).collect();
    if filter_rows.is_empty() {
        filter_rows.push(("", "eq", ""));
    }
    let qs = uri.query().unwrap_or("").to_string();
    let mut body_json = m.clone();
    body_json.insert("timezone".into(), json!(tz));
    let body_s = serde_json::to_string_pretty(&Value::Object(body_json)).unwrap_or_default();
    let body = html! {
        (page_head("Query builder", html! { "Ask any question of any collection. Every result comes with the API call that reproduces it." }, html! {}))
        @if schema.is_empty() { (no_events(&pid)) } @else {
        div.explore {
            form.card.qform method="get" {
                div.field { label.f { "Collection" } (coll_select("collection", &schema, coll.as_deref(), true)) }
                div.field {
                    label.f { "Analysis" }
                    select name="analysis" { @for (k, l) in BUILDER_ANALYSES { option value=(k) selected[k == analysis] { (l) } } }
                }
                div.field data-show="count_unique sum average minimum maximum median percentile select_unique" {
                    label.f { "Of property" } (prop_select("target", &props, target, None))
                }
                div.field data-show="percentile" {
                    label.f { "Percentile" } input type="text" name="percentile" value=(get(&ps, "percentile").unwrap_or("90")) inputmode="decimal";
                }
                div.field { label.f { "Timeframe" } (range_select(rk)) }
                div.field data-rows {
                    label.f { "Filters" }
                    @for (fp, fo, fv) in &filter_rows {
                        div.rep.frow {
                            (prop_select("fp", &props, Some(fp), Some("Property…")))
                            select name="fo" { @for (k, l) in OPS { option value=(k) selected[k == *fo] { (l) } } }
                            input type="text" name="fv" value=(fv) placeholder="value";
                            button.ghost.small type="button" data-remove title="Remove" { "×" }
                        }
                    }
                    button.ghost.small type="button" data-add { "+ Filter" }
                }
                div data-show="count count_unique sum average minimum maximum median percentile select_unique" {
                    div.field { label.f { "Group by" } (prop_select("group_by", &props, group.as_deref(), Some("No grouping"))) }
                    div.field {
                        label.f { "Interval" }
                        select name="interval" {
                            @for (k, l) in [("", "None (one total)"), ("hourly", "Hourly"), ("daily", "Daily"), ("weekly", "Weekly"), ("monthly", "Monthly")] {
                                option value=(k) selected[k == interval] { (l) }
                            }
                        }
                    }
                }
                button.block type="submit" { "Run query" }
            }
            div.stack {
                @match &result {
                    Some(Ok(r)) => (render_result(r, &analysis, &label, group.as_deref(), &interval)),
                    Some(Err(e)) => div.notice.err { (e) },
                    None => {}
                }
                @if result.is_some() {
                    div.card data-tabs {
                        div.card-hd {
                            h2 { "API call" }
                            div.tabs { button.on type="button" data-tab="body" { "JSON" } button type="button" data-tab="curl" { "curl" } }
                        }
                        div data-panel="body" { (snippet(&format!("POST /1.0/projects/{pid}/queries/{analysis}"), "q-body", &body_s)) }
                        div data-panel="curl" hidden {
                            (snippet("Use a key with the queries permission", "q-curl", &format!("curl -X POST {public}/1.0/projects/{pid}/queries/{analysis} \\\n  -H \"Authorization: $READ_KEY\" -H 'Content-Type: application/json' \\\n  -d '{}'", serde_json::to_string(&body_json_compact(&m, &tz)).unwrap_or_default())))
                        }
                        form.row method="post" action=(format!("/app/p/{pid}/explore/save")) style="margin-top:4px" {
                            input type="hidden" name="csrf" value=(u.csrf);
                            input type="hidden" name="qs" value=(qs);
                            input type="text" name="name" placeholder="Save as… e.g. weekly-signups" style="flex:1;max-width:320px" required;
                            button.ghost type="submit" { "Save query" }
                        }
                    }
                }
                @if !saved.is_empty() {
                    div.card {
                        h2 { "Saved queries" }
                        div.saved {
                            @for sq in &saved {
                                @let link = sq.body.pointer("/metadata/explorer").and_then(Value::as_str);
                                div.row style="justify-content:space-between;padding:6px 0;border-bottom:1px solid var(--border)" {
                                    @if let Some(l) = link { a href=(format!("?{l}")) { (sq.name) } } @else { span { (sq.name) } }
                                    span.muted.small { (sq.body.pointer("/query/analysis_type").and_then(Value::as_str).unwrap_or("")) " · " (sq.body.pointer("/query/event_collection").and_then(Value::as_str).unwrap_or("")) }
                                }
                            }
                        }
                    }
                }
            }
        }
        }
    };
    page(&st, &u, &p, Tab::Builder, "Query builder", body)
}

fn body_json_compact(m: &Map<String, Value>, tz: &str) -> Value {
    let mut b = m.clone();
    b.insert("timezone".into(), json!(tz));
    Value::Object(b)
}

pub async fn save_query(State(st): State<St>, u: WebUser, Path(pid): Path<String>, headers: HeaderMap, Form(form): Form<HashMap<String, String>>) -> Response {
    if let Err(r) = u.project(&st, &pid) {
        return r;
    }
    if let Err(r) = u.check_csrf(&form) {
        return r;
    }
    let qs = form.get("qs").cloned().unwrap_or_default();
    let name: String = form
        .get("name")
        .map(|n| n.trim().chars().map(|c| if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' { c } else { '-' }).collect())
        .unwrap_or_default();
    let back = format!("/app/p/{pid}/explore?{qs}");
    if name.is_empty() {
        return Redirect::to(&back).into_response();
    }
    let schema = st.engine.schema(&pid).await.unwrap_or_default();
    let ps: Vec<(String, String)> = url::form_urlencoded::parse(qs.as_bytes()).into_owned().collect();
    let (analysis, _, mut m, _, _) = builder_query(&ps, &schema);
    m.insert("timezone".into(), json!(tz(&headers)));
    let Ok(q) = crate::query::parse(&analysis, &m) else { return Redirect::to(&back).into_response() };
    let body = json!({"query": q.echo(), "metadata": {"explorer": qs}, "refresh_rate": 0});
    if let Err(e) = st.meta.put_saved_query(&pid, &name, &body) {
        return internal(e);
    }
    Redirect::to(&back).into_response()
}

// ---- 2. Event stream ----

pub async fn stream_page(State(st): State<St>, u: WebUser, Path(pid): Path<String>, uri: Uri, headers: HeaderMap) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let schema = st.engine.schema(&pid).await.unwrap_or_default();
    let ps = params(&uri);
    let tz = tz(&headers);
    let only = get(&ps, "collection").filter(|c| schema.contains_key(*c)).map(String::from);
    let live = get(&ps, "live") == Some("1");
    let colls: Vec<String> = match &only {
        Some(c) => vec![c.clone()],
        None => schema.keys().take(30).cloned().collect(),
    };
    let per = if only.is_some() { 200 } else { 25 };
    let results: Vec<(String, Result<Value, String>)> = stream::iter(colls.into_iter().map(|c| {
        let st = &st;
        let pid = &pid;
        let tz = &tz;
        async move {
            let r = run(st, pid, "extraction", json!({"event_collection": c, "timeframe": "this_30_days", "latest": per}), tz).await;
            (c, r)
        }
    }))
    .buffer_unordered(6)
    .collect()
    .await;
    let mut events: Vec<(String, String, Value)> = vec![];
    for (c, r) in results {
        if let Ok(Value::Array(items)) = r {
            for ev in items {
                let ts = ev.pointer("/keen/timestamp").and_then(Value::as_str).unwrap_or("").to_string();
                events.push((ts, c.clone(), ev));
            }
        }
    }
    events.sort_by(|a, b| b.0.cmp(&a.0));
    events.truncate(150);
    let link = |c: Option<&str>, live: bool| {
        let mut q = vec![];
        if let Some(c) = c {
            q.push(format!("collection={}", urlencoding(c)));
        }
        if live {
            q.push("live=1".into());
        }
        format!("?{}", q.join("&"))
    };
    let body = html! {
        (page_head("Event stream", html! { "The latest events as they arrive, newest first. Click one to see everything it carries." },
            html! { a.btn.ghost.on[live] href=(link(only.as_deref(), !live)) { @if live { span.live { i {} "Live · pause" } } @else { "▶ Go live" } } }))
        @if schema.is_empty() { (no_events(&pid)) } @else {
            div.seg style="margin-bottom:16px" {
                a.on[only.is_none()] href=(link(None, live)) { "All collections" }
                @for c in schema.keys() { a.on[only.as_deref() == Some(c.as_str())] href=(link(Some(c), live)) { (c) } }
            }
            div.card style="padding:8px 12px" {
                @if events.is_empty() { div.empty { "No events in the last 30 days." } }
                div.feed {
                    @for (i, (_, c, ev)) in events.iter().enumerate() {
                        (event_row(ev, only.is_none().then_some(c.as_str()), &format!("ev{i}")))
                    }
                }
            }
            p.footnote { "Showing up to " (events.len()) " recent events from the last 30 days" @if live { ", refreshing every 5 seconds" } "." }
            @if live {
                script { (PreEscaped("setTimeout(function(){ if(!document.querySelector('details.ev[open]')) location.reload(); else setTimeout(arguments.callee, 5000); }, 5000);")) }
            }
        }
    };
    page(&st, &u, &p, Tab::Stream, "Event stream", body)
}

fn urlencoding(s: &str) -> String {
    url::form_urlencoded::byte_serialize(s.as_bytes()).collect()
}

// ---- 3. Collections and property profiler ----

fn sparkline(vals: &[f64]) -> Markup {
    let max = vals.iter().cloned().fold(0.0, f64::max).max(1.0);
    let n = vals.len().max(2) - 1;
    let pts: Vec<String> = vals.iter().enumerate().map(|(i, v)| format!("{:.1},{:.1}", i as f64 / n as f64 * 100.0, 28.0 - v / max * 26.0)).collect();
    let line = pts.join(" ");
    html! {
        svg.spark viewBox="0 0 100 30" preserveAspectRatio="none" aria-hidden="true" {
            polygon points=(format!("0,30 {line} 100,30")) {}
            polyline points=(line) vector-effect="non-scaling-stroke" {}
        }
    }
}

pub async fn collections_page(State(st): State<St>, u: WebUser, Path(pid): Path<String>, uri: Uri, headers: HeaderMap) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let schema = st.engine.schema(&pid).await.unwrap_or_default();
    let ps = params(&uri);
    let tz = tz(&headers);
    if let Some(c) = get(&ps, "c").filter(|c| schema.contains_key(*c)) {
        return profile(&st, &u, &p, &schema, c, &tz).await;
    }
    let cards: Vec<(String, Vec<f64>)> = stream::iter(schema.keys().take(40).cloned().map(|c| {
        let st = &st;
        let pid = &pid;
        let tz = &tz;
        async move {
            let r = run(st, pid, "count", json!({"event_collection": c, "timeframe": "this_30_days", "interval": "daily"}), tz).await;
            let vals = match r {
                Ok(Value::Array(items)) => items.iter().map(|i| i.get("value").and_then(Value::as_f64).unwrap_or(0.0)).collect(),
                _ => vec![],
            };
            (c, vals)
        }
    }))
    .buffered(6)
    .collect()
    .await;
    let mut cards = cards;
    cards.sort_by(|a, b| b.1.iter().sum::<f64>().partial_cmp(&a.1.iter().sum::<f64>()).unwrap_or(std::cmp::Ordering::Equal));
    let body = html! {
        (page_head("Collections", html! { "Every collection in this project, busiest first. Open one to profile its properties." }, html! {}))
        @if schema.is_empty() { (no_events(&pid)) } @else {
            div.grid.g3 {
                @for (c, vals) in &cards {
                    @let total: f64 = vals.iter().sum();
                    @let today = vals.last().copied().unwrap_or(0.0);
                    a.card.coll href=(format!("?c={}", urlencoding(c))) {
                        div.row style="justify-content:space-between" {
                            code { (c) }
                            @if standard::is_standard(c) { span.pill { "standard" } }
                        }
                        div.v { (fmt_compact(total)) span.muted.small { " events · 30 days" } }
                        (sparkline(vals))
                        div.muted.small { (fmt_int(today as u64)) " today · " (schema.get(c).map(|p| p.len()).unwrap_or(0)) " properties" }
                    }
                }
            }
        }
    };
    page(&st, &u, &p, Tab::Collections, "Collections", body)
}

struct PropProfile {
    path: String,
    kind: String,
    coverage: Option<f64>,
    top: Vec<(String, f64)>,
    stats: Option<(f64, f64, f64)>,
}

async fn profile(st: &St, u: &WebUser, p: &Project, schema: &Schema, coll: &str, tz: &str) -> Response {
    let pid = &p.id;
    let props: Vec<(String, String)> = schema.get(coll).map(|m| m.iter().filter(|(k, _)| !k.starts_with("keen.")).map(|(a, b)| (a.clone(), b.clone())).take(60).collect()).unwrap_or_default();
    let tf = "this_30_days";
    let (series, total) = tokio::join!(
        run(st, pid, "count", json!({"event_collection": coll, "timeframe": tf, "interval": "daily"}), tz),
        run(st, pid, "count", json!({"event_collection": coll, "timeframe": tf}), tz),
    );
    let total = total.ok().and_then(|v| v.as_f64()).unwrap_or(0.0);
    let profiles: Vec<PropProfile> = stream::iter(props.into_iter().map(|(path, kind)| async move {
        let exists = json!([{"property_name": path, "operator": "exists", "property_value": true}]);
        let have = run(st, pid, "count", json!({"event_collection": coll, "timeframe": tf, "filters": exists}), tz).await.ok().and_then(|v| v.as_f64());
        let coverage = have.map(|h| if total > 0.0 { h / total } else { 0.0 });
        let mut top = vec![];
        let mut stats = None;
        if is_numeric(&kind) {
            let (a, b, c) = tokio::join!(
                run(st, pid, "minimum", json!({"event_collection": coll, "timeframe": tf, "target_property": path}), tz),
                run(st, pid, "average", json!({"event_collection": coll, "timeframe": tf, "target_property": path}), tz),
                run(st, pid, "maximum", json!({"event_collection": coll, "timeframe": tf, "target_property": path}), tz),
            );
            if let (Ok(a), Ok(b), Ok(c)) = (a, b, c) {
                stats = Some((a.as_f64().unwrap_or(0.0), b.as_f64().unwrap_or(0.0), c.as_f64().unwrap_or(0.0)));
            }
        } else if kind != "datetime" {
            if let Ok(Value::Array(rows)) = run(st, pid, "count", json!({"event_collection": coll, "timeframe": tf, "group_by": path}), tz).await {
                top = rows.iter().map(|r| (fmt_val(r.get(&path).unwrap_or(&Value::Null)), r.get("result").and_then(Value::as_f64).unwrap_or(0.0))).collect();
                top.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
            }
        }
        PropProfile { path, kind, coverage, top, stats }
    }))
    .buffered(8)
    .collect()
    .await;
    let chart = match series {
        Ok(Value::Array(items)) if !items.is_empty() => {
            let vals: Vec<Option<f64>> = items.iter().map(|i| i.get("value").and_then(Value::as_f64)).collect();
            let ticks: Vec<String> = items.iter().map(|i| tick_label(i, "daily")).collect();
            let tips: Vec<String> = items.iter().map(|i| format!("{}\u{1f}{}", tf_label(i, "daily"), fmt_val(i.get("value").unwrap_or(&Value::Null)))).collect();
            line_chart(&ticks, &tips, &vals, "Events", &|v| fmt_compact(v))
        }
        _ => html! {},
    };
    let builder = |extra: &str| format!("/app/p/{pid}/explore?collection={}&range=30d{extra}", urlencoding(coll));
    let body = html! {
        (page_head(coll, html! { a href="?" { "← All collections" } " · " (fmt_int(total as u64)) " events in the last 30 days" },
            html! { a.btn.ghost href=(builder("")) { "Open in query builder" } a.btn.ghost href=(format!("/app/p/{pid}/stream?collection={}", urlencoding(coll))) { "See events" } }))
        div.card style="margin-bottom:16px" { (chart) }
        div.card style="padding:8px 12px" {
            div.table-wrap {
                table.profile {
                    thead { tr { th { "Property" } th { "Type" } th { "Present in" } th { "Values" } } }
                    tbody {
                        @for pp in &profiles {
                            tr {
                                td { a.mono href=(builder(&format!("&group_by={}", urlencoding(&pp.path)))) title="Group by this in the query builder" { (pp.path) } }
                                td.muted { (pp.kind) }
                                td style="white-space:nowrap" {
                                    @if let Some(c) = pp.coverage {
                                        div.meter { div style=(format!("width:{:.0}%", c * 100.0)) {} }
                                        span.small.muted { (format!("{:.0}%", c * 100.0)) }
                                    }
                                }
                                td {
                                    @if let Some((lo, avg, hi)) = pp.stats {
                                        span.small { "min " b { (fmt_num(lo)) } " · avg " b { (fmt_num(avg)) } " · max " b { (fmt_num(hi)) } }
                                    } @else if !pp.top.is_empty() {
                                        @for (v, n) in pp.top.iter().take(5) { span.pill title=(format!("{} events", fmt_num(*n))) { (v) " " span.muted { (fmt_compact(*n)) } } }
                                        @if pp.top.len() > 5 { span.muted.small { "+" (pp.top.len() - 5) " more" } }
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    };
    page(st, u, p, Tab::Collections, coll, body)
}

// ---- 4. Pivot ----

const PIVOT_METRICS: [(&str, &str); 4] = [("count", "Count of events"), ("count_unique", "Unique values of…"), ("sum", "Sum of…"), ("average", "Average of…")];

pub async fn pivot_page(State(st): State<St>, u: WebUser, Path(pid): Path<String>, uri: Uri, headers: HeaderMap) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let schema = st.engine.schema(&pid).await.unwrap_or_default();
    let ps = params(&uri);
    let tz = tz(&headers);
    let coll = get(&ps, "collection").filter(|c| schema.contains_key(*c)).map(String::from).or_else(|| {
        schema.contains_key(standard::PAGEVIEW).then(|| standard::PAGEVIEW.to_string()).or(schema.keys().next().cloned())
    });
    let props = props_of(&schema, coll.as_deref());
    let pick = |want: &[&str], skip: Option<&str>| -> Option<String> {
        want.iter().find(|w| props.contains_key(**w) && Some(**w) != skip).map(|s| s.to_string()).or_else(|| {
            props.iter().find(|(p, k)| k.as_str() == "string" && Some(p.as_str()) != skip && !p.ends_with(".id")).map(|(p, _)| p.clone())
        })
    };
    let rows_p = get(&ps, "rows").filter(|r| props.contains_key(*r)).map(String::from).or_else(|| pick(&["page.path", "session.channel"], None));
    let cols_p = get(&ps, "cols").filter(|r| props.contains_key(*r)).map(String::from).or_else(|| pick(&["device.type", "browser.name"], rows_p.as_deref()));
    let metric = get(&ps, "metric").filter(|m| PIVOT_METRICS.iter().any(|x| x.0 == *m)).unwrap_or("count");
    let target = get(&ps, "target").filter(|t| props.contains_key(*t)).map(String::from).or_else(|| {
        if metric == "count_unique" { props.contains_key("visitor.id").then(|| "visitor.id".to_string()) } else { props.iter().find(|(_, k)| is_numeric(k)).map(|(p, _)| p.clone()) }
    });
    let (rk, tf, _) = range(&ps);
    let filters = filters_from(&ps, &props, "fp", "fo", "fv");
    let result = match (&coll, &rows_p, &cols_p) {
        (Some(c), Some(r), Some(k)) if r != k => {
            let mut m = json!({"event_collection": c, "timeframe": tf, "group_by": [r, k], "filters": filters});
            if metric != "count" {
                m["target_property"] = json!(target);
            }
            Some(run(&st, &pid, metric, m, &tz).await)
        }
        _ => None,
    };
    let body = html! {
        (page_head("Pivot", html! { "Cross any two properties and see where the activity concentrates." }, html! {}))
        @if schema.is_empty() { (no_events(&pid)) } @else {
            form.card.hform method="get" style="margin-bottom:16px" {
                div.field { label.f { "Collection" } (coll_select("collection", &schema, coll.as_deref(), true)) }
                div.field { label.f { "Rows" } (prop_select("rows", &props, rows_p.as_deref(), None)) }
                div.field { label.f { "Columns" } (prop_select("cols", &props, cols_p.as_deref(), None)) }
                div.field {
                    label.f { "Measure" }
                    select name="metric" onchange="this.form.submit()" { @for (k, l) in PIVOT_METRICS { option value=(k) selected[k == metric] { (l) } } }
                }
                @if metric != "count" { div.field { label.f { "Property" } (prop_select("target", &props, target.as_deref(), None)) } }
                div.field { label.f { "Timeframe" } (range_select(rk)) }
                div.field style="align-self:end" {
                    button type="submit" { "Pivot" }
                    " "
                    a.btn.ghost href=(format!("?collection={}&rows={}&cols={}&metric={metric}&range={rk}{}", urlencoding(coll.as_deref().unwrap_or("")), urlencoding(cols_p.as_deref().unwrap_or("")), urlencoding(rows_p.as_deref().unwrap_or("")), target.as_ref().map(|t| format!("&target={}", urlencoding(t))).unwrap_or_default())) title="Swap rows and columns" { "⇄" }
                }
            }
            @match &result {
                Some(Ok(Value::Array(items))) => (pivot_table(items, rows_p.as_deref().unwrap_or(""), cols_p.as_deref().unwrap_or(""), metric)),
                Some(Ok(_)) => div.card { div.empty { "No results." } },
                Some(Err(e)) => div.notice.err { (e) },
                None => div.notice { "Pick two different properties." },
            }
        }
    };
    page(&st, &u, &p, Tab::Pivot, "Pivot", body)
}

fn pivot_table(items: &[Value], rp: &str, cp: &str, metric: &str) -> Markup {
    if items.is_empty() {
        return html! { div.card { div.empty { "No matching events in this timeframe." } } };
    }
    let additive = metric == "count" || metric == "sum";
    let mut cells: HashMap<(String, String), f64> = HashMap::new();
    let mut rt: HashMap<String, f64> = HashMap::new();
    let mut ct: HashMap<String, f64> = HashMap::new();
    for i in items {
        let r = fmt_val(i.get(rp).unwrap_or(&Value::Null));
        let c = fmt_val(i.get(cp).unwrap_or(&Value::Null));
        let v = i.get("result").and_then(Value::as_f64).unwrap_or(0.0);
        *rt.entry(r.clone()).or_default() += v;
        *ct.entry(c.clone()).or_default() += v;
        cells.insert((r, c), v);
    }
    let top = |m: HashMap<String, f64>, n: usize| {
        let mut v: Vec<(String, f64)> = m.into_iter().collect();
        v.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        let more = v.len().saturating_sub(n);
        v.truncate(n);
        (v, more)
    };
    let (rows, more_r) = top(rt, 20);
    let (cols, more_c) = top(ct, 10);
    let max = cells.values().cloned().fold(0.0, f64::max).max(f64::MIN_POSITIVE);
    html! {
        div.card style="padding:8px 12px" {
            div.table-wrap {
                table.heat {
                    thead {
                        tr {
                            th { (rp) " ╲ " (cp) }
                            @for (c, _) in &cols { th.num { (c) } }
                            @if additive { th.num { "Total" } }
                        }
                    }
                    tbody {
                        @for (r, total) in &rows {
                            tr {
                                td.lbl title=(r) { (r) }
                                @for (c, _) in &cols {
                                    @let v = cells.get(&(r.clone(), c.clone())).copied();
                                    td.num style=(heat(v.unwrap_or(0.0), max)) title=(format!("{r} × {c}")) { @if let Some(v) = v { (fmt_num(v)) } @else { span.muted { "·" } } }
                                }
                                @if additive { td.num.tot { (fmt_num(*total)) } }
                            }
                        }
                    }
                    @if additive {
                        tfoot { tr { td { "Total" } @for (_, t) in &cols { td.num.tot { (fmt_num(*t)) } } td.num.tot { (fmt_num(cols.iter().map(|c| c.1).sum::<f64>())) } } }
                    }
                }
            }
            @if more_r > 0 || more_c > 0 { p.footnote { "Showing the largest " (rows.len()) " rows and " (cols.len()) " columns (" (more_r) " rows and " (more_c) " columns hidden)." } }
        }
    }
}

// ---- 5. Funnels ----

pub async fn funnels_page(State(st): State<St>, u: WebUser, Path(pid): Path<String>, uri: Uri, headers: HeaderMap) -> Response {
    let p = match u.project(&st, &pid) {
        Ok(p) => p,
        Err(r) => return r,
    };
    let schema = st.engine.schema(&pid).await.unwrap_or_default();
    let ps = params(&uri);
    let tz = tz(&headers);
    let (rk, tf, _) = range(&ps);
    let actor = get(&ps, "actor").unwrap_or("visitor.id").to_string();
    let mut steps: Vec<(String, String, String)> = {
        let (sc, sp, sv) = (all(&ps, "sc"), all(&ps, "sp"), all(&ps, "sv"));
        sc.iter().enumerate().filter(|(_, c)| schema.contains_key(**c)).map(|(i, c)| (c.to_string(), sp.get(i).unwrap_or(&"").to_string(), sv.get(i).unwrap_or(&"").to_string())).collect()
    };
    if steps.is_empty() {
        // A starting point from what this project has.
        let custom = schema.keys().find(|c| !standard::is_standard(c));
        for c in [Some(standard::SESSION), Some(standard::PAGEVIEW), custom.map(String::as_str)].into_iter().flatten() {
            if schema.contains_key(c) && !steps.iter().any(|s| s.0 == c) {
                steps.push((c.to_string(), String::new(), String::new()));
            }
        }
        if steps.is_empty() {
            steps.extend(schema.keys().take(2).map(|c| (c.clone(), String::new(), String::new())));
        }
    }
    let all_props: BTreeMap<String, String> = schema.values().flat_map(|m| m.iter().map(|(a, b)| (a.clone(), b.clone()))).filter(|(p, _)| !p.starts_with("keen.")).collect();
    let result = if steps.len() >= 2 {
        let body: Vec<Value> = steps
            .iter()
            .map(|(c, fp, fv)| {
                let mut s = json!({"event_collection": c, "actor_property": actor, "timeframe": tf, "timezone": tz});
                if !fp.is_empty() {
                    let kind = schema.get(c).and_then(|m| m.get(fp)).map(String::as_str);
                    s["filters"] = json!([{"property_name": fp, "operator": "eq", "property_value": typed(fv, kind, "eq")}]);
                }
                s
            })
            .collect();
        Some(run(&st, &pid, "funnel", json!({"steps": body}), &tz).await)
    } else {
        None
    };
    let counts: Vec<f64> = match &result {
        Some(Ok(Value::Array(v))) => v.iter().map(|x| x.as_f64().unwrap_or(0.0)).collect(),
        _ => vec![],
    };
    let first = counts.first().copied().unwrap_or(0.0).max(f64::MIN_POSITIVE);
    let body = html! {
        (page_head("Funnels", html! { "How many of the same people go on to each next step, in order." }, html! {}))
        @if schema.is_empty() { (no_events(&pid)) } @else {
            datalist #props { @for (p, _) in &all_props { option value=(p) {} } }
            div.explore {
                form.card.qform method="get" {
                    div.field data-rows {
                        label.f { "Steps" }
                        @for (c, fp, fv) in &steps {
                            div.rep.step-row {
                                (coll_select("sc", &schema, Some(c), false))
                                input type="text" name="sp" value=(fp) list="props" placeholder="Only where property… (optional)";
                                input type="text" name="sv" value=(fv) placeholder="equals value";
                                button.ghost.small type="button" data-remove title="Remove step" { "×" }
                            }
                        }
                        button.ghost.small type="button" data-add { "+ Step" }
                    }
                    div.field { label.f { "Same person means same" } input type="text" name="actor" value=(actor) list="props"; div.hint { "e.g. visitor.id, or visitor.user_id for signed-in users" } }
                    div.field { label.f { "Timeframe" } (range_select(rk)) }
                    button.block type="submit" { "Run funnel" }
                }
                div.stack {
                    @match &result {
                        Some(Err(e)) => div.notice.err { (e) },
                        None => div.notice { "Add at least two steps." },
                        Some(Ok(_)) => div.card {
                            div.card-hd {
                                h2 { "Conversion" }
                                @if counts.len() >= 2 { span.muted { "Overall " b { (format!("{:.1}%", counts.last().unwrap() / first * 100.0)) } } }
                            }
                            div.funnel {
                                @for (i, (n, (c, fp, fv))) in counts.iter().zip(&steps).enumerate() {
                                    @let prev = if i == 0 { *n } else { counts[i - 1] };
                                    div.fstep {
                                        div.fl {
                                            span.step { (i + 1) }
                                            code { (c) }
                                            @if !fp.is_empty() { span.muted.small { " where " (fp) " = " (fv) } }
                                        }
                                        div.fb { div style=(format!("width:{:.1}%", n / first * 100.0)) {} }
                                        div.fn {
                                            b { (fmt_num(*n)) }
                                            span.muted.small { " · " (format!("{:.1}%", n / first * 100.0)) }
                                        }
                                        @if i > 0 && prev > 0.0 {
                                            div.fd.small { "↓ " (format!("{:.1}%", n / prev * 100.0)) " continued · " (fmt_num(prev - n)) " dropped off" }
                                        }
                                    }
                                }
                            }
                        },
                    }
                }
            }
        }
    };
    page(&st, &u, &p, Tab::Funnels, "Funnels", body)
}
