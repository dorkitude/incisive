//! Query execution over columnar batches.

use super::timeframe::{buckets, parse_timeframe, parse_timezone, Range, Zone};
use super::{bad, Filter, Query, QueryError};
use crate::store::table::{num_value, Batch, Cell};
use crate::store::Engine;
use chrono::{DateTime, Utc};
use serde_json::{json, Map, Value};
use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

/// Limits on a single query.
#[derive(Clone, Debug)]
pub struct Limits {
    pub max_extraction: usize,
    pub max_funnel: usize,
    pub max_groups: usize,
    pub max_intervals: usize,
    pub max_scan: usize,
}

fn internal(e: anyhow::Error) -> QueryError {
    QueryError { code: "InternalServerError", message: format!("{e:#}"), status: 500 }
}

// ---- filters ----

enum Pred {
    Eq(Value),
    Ne(Value),
    Cmp(Ordering, bool, Value), // (direction, or-equal, value)
    Exists(bool),
    In(Vec<Value>),
    Contains(Value, bool), // (value, negate)
    Within(f64, f64, f64), // lon, lat, meters
    TimeCmp(&'static str, i64), // operator, ms (on the event timestamp)
}

struct Compiled {
    path: String,
    pred: Pred,
}

fn compile(f: &Filter) -> Result<Compiled, QueryError> {
    let v = &f.property_value;
    let op = f.operator.as_str();
    if f.property_name == "keen.timestamp"
        && let Some(t) = v.as_str().and_then(crate::event::parse_time) {
            let ms = t.timestamp_millis();
            let op = match op {
                "eq" => "eq",
                "ne" => "ne",
                "lt" => "lt",
                "lte" => "lte",
                "gt" => "gt",
                "gte" => "gte",
                _ => "",
            };
            if !op.is_empty() {
                return Ok(Compiled { path: String::new(), pred: Pred::TimeCmp(op, ms) });
            }
        }
    let pred = match op {
        "eq" => {
            scalar(v, op)?;
            Pred::Eq(v.clone())
        }
        "ne" => {
            scalar(v, op)?;
            Pred::Ne(v.clone())
        }
        "lt" | "lte" | "gt" | "gte" => {
            if !(v.is_number() || v.is_string()) {
                return Err(bad("InvalidFilterError", format!("operator {op} requires a number or string property_value")));
            }
            let dir = if op.starts_with('l') { Ordering::Less } else { Ordering::Greater };
            Pred::Cmp(dir, op.ends_with('e'), v.clone())
        }
        "exists" => Pred::Exists(v.as_bool().ok_or_else(|| bad("InvalidFilterError", "operator exists requires a boolean property_value"))?),
        "in" => {
            let list = v.as_array().ok_or_else(|| bad("InvalidFilterError", "operator in requires a list property_value"))?;
            for x in list {
                scalar(x, op)?;
            }
            Pred::In(list.clone())
        }
        "contains" | "not_contains" => {
            if !(v.is_string() || v.is_number()) {
                return Err(bad("InvalidFilterError", format!("operator {op} requires a string or number property_value")));
            }
            Pred::Contains(v.clone(), op == "not_contains")
        }
        "within" => {
            let msg = "operator within requires {coordinates: [lon, lat], max_distance_miles: n}";
            let c = v.get("coordinates").and_then(Value::as_array).ok_or_else(|| bad("InvalidFilterError", msg))?;
            let miles = v.get("max_distance_miles").and_then(Value::as_f64).ok_or_else(|| bad("InvalidFilterError", msg))?;
            let (Some(lon), Some(lat)) = (c.first().and_then(Value::as_f64), c.get(1).and_then(Value::as_f64)) else {
                return Err(bad("InvalidFilterError", msg));
            };
            Pred::Within(lon, lat, miles * 1609.344)
        }
        other => return Err(bad("InvalidFilterError", format!("unsupported filter operator {other:?}"))),
    };
    Ok(Compiled { path: f.property_name.clone(), pred })
}

fn scalar(v: &Value, op: &str) -> Result<(), QueryError> {
    if v.is_array() || v.is_object() {
        return Err(bad("InvalidFilterError", format!("operator {op} does not support list or object property_value")));
    }
    Ok(())
}

fn scalar_eq(c: &Cell, v: &Value) -> bool {
    match (c, v) {
        (Cell::Num(n), Value::Number(m)) => m.as_f64() == Some(*n),
        (Cell::Str(s), Value::String(t)) => **s == **t,
        (Cell::Bool(b), Value::Bool(w)) => b == w,
        _ => false,
    }
}

fn json_scalar_eq(x: &Value, v: &Value) -> bool {
    match (x, v) {
        (Value::Number(a), Value::Number(b)) => a.as_f64() == b.as_f64(),
        (Value::String(a), Value::String(b)) => a == b,
        (Value::Bool(a), Value::Bool(b)) => a == b,
        _ => false,
    }
}

/// eq: a scalar equal to v, or a list containing v. eq null matches null and missing.
fn eq(c: &Cell, v: &Value) -> bool {
    if v.is_null() {
        return !c.present();
    }
    if scalar_eq(c, v) {
        return true;
    }
    match c {
        Cell::Json(j) => j.as_array().is_some_and(|a| a.iter().any(|x| json_scalar_eq(x, v))),
        _ => false,
    }
}

fn cmp_cell(c: &Cell, v: &Value) -> Option<Ordering> {
    match (c, v) {
        (Cell::Num(n), Value::Number(m)) => n.partial_cmp(&m.as_f64()?),
        (Cell::Str(s), Value::String(t)) => Some((**s).cmp(t.as_str())),
        _ => None,
    }
}

fn haversine(lon1: f64, lat1: f64, lon2: f64, lat2: f64) -> f64 {
    let r = 6_371_008.8;
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = (lat2 - lat1).to_radians();
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * r * a.sqrt().asin()
}

impl Compiled {
    fn test(&self, c: &Cell, ts: i64) -> bool {
        match &self.pred {
            Pred::Eq(v) => eq(c, v),
            Pred::Ne(v) => !eq(c, v),
            Pred::Cmp(dir, or_eq, v) => cmp_cell(c, v).is_some_and(|o| o == *dir || (*or_eq && o == Ordering::Equal)),
            Pred::Exists(want) => c.present() == *want,
            Pred::In(list) => list.iter().any(|v| eq(c, v)),
            Pred::Contains(v, neg) => {
                let hit = match (c, v) {
                    (Cell::Str(s), Value::String(t)) => s.contains(t.as_str()),
                    (Cell::Json(j), _) => j.as_array().is_some_and(|a| a.iter().any(|x| json_scalar_eq(x, v))),
                    _ => false,
                };
                hit != *neg
            }
            Pred::Within(lon, lat, meters) => match c {
                Cell::Json(j) => match j.as_array().map(|a| a.as_slice()) {
                    Some([x, y]) => match (x.as_f64(), y.as_f64()) {
                        (Some(x), Some(y)) => haversine(x, y, *lon, *lat) <= *meters,
                        _ => false,
                    },
                    _ => false,
                },
                _ => false,
            },
            Pred::TimeCmp(op, ms) => match *op {
                "eq" => ts == *ms,
                "ne" => ts != *ms,
                "lt" => ts < *ms,
                "lte" => ts <= *ms,
                "gt" => ts > *ms,
                _ => ts >= *ms,
            },
        }
    }
}

/// Rows of a batch that pass the timeframe and filters.
fn matching(b: &Batch, r: Option<Range>, fs: &[Compiled]) -> Vec<usize> {
    let (s, e) = r.map(|r| (r.start.timestamp_millis(), r.end.timestamp_millis())).unwrap_or((i64::MIN, i64::MAX));
    let missing = Cell::Missing;
    (0..b.len())
        .filter(|&i| {
            let ts = b.ts[i];
            ts >= s
                && ts < e
                && fs.iter().all(|f| {
                    let c = if f.path.is_empty() { &missing } else { b.col(&f.path).map(|c| &c[i]).unwrap_or(&missing) };
                    f.test(c, ts)
                })
        })
        .collect()
}

// ---- aggregation ----

#[derive(Clone)]
enum Acc {
    Count(u64),
    Unique(HashSet<String>),
    Sum(f64),
    Avg(f64, u64),
    Min(Option<f64>),
    Max(Option<f64>),
    Values(Vec<f64>, f64),
    Select(HashMap<String, Value>),
}

impl Acc {
    fn new(analysis: &str, pct: Option<f64>) -> Acc {
        match analysis {
            "count" => Acc::Count(0),
            "count_unique" => Acc::Unique(HashSet::new()),
            "sum" => Acc::Sum(0.0),
            "average" => Acc::Avg(0.0, 0),
            "minimum" => Acc::Min(None),
            "maximum" => Acc::Max(None),
            "median" => Acc::Values(vec![], 50.0),
            "percentile" => Acc::Values(vec![], pct.unwrap_or(50.0)),
            _ => Acc::Select(HashMap::new()),
        }
    }

    fn add(&mut self, c: &Cell) {
        match self {
            Acc::Count(n) => *n += 1,
            Acc::Unique(s) => {
                if c.present() {
                    s.insert(c.key());
                }
            }
            Acc::Sum(t) => *t += c.num().unwrap_or(0.0),
            Acc::Avg(t, n) => {
                if let Some(x) = c.num() {
                    *t += x;
                    *n += 1;
                }
            }
            Acc::Min(m) => {
                if let Some(x) = c.num() {
                    *m = Some(m.map_or(x, |y: f64| y.min(x)));
                }
            }
            Acc::Max(m) => {
                if let Some(x) = c.num() {
                    *m = Some(m.map_or(x, |y: f64| y.max(x)));
                }
            }
            Acc::Values(v, _) => {
                if let Some(x) = c.num() {
                    v.push(x);
                }
            }
            Acc::Select(s) => {
                if c.present() {
                    s.entry(c.key()).or_insert_with(|| c.to_value());
                }
            }
        }
    }

    fn result(self) -> Value {
        match self {
            Acc::Count(n) => Value::from(n),
            Acc::Unique(s) => Value::from(s.len()),
            Acc::Sum(t) => num_value(t),
            Acc::Avg(t, n) => {
                if n == 0 {
                    Value::Null
                } else {
                    num_value(t / n as f64)
                }
            }
            Acc::Min(m) | Acc::Max(m) => m.map(num_value).unwrap_or(Value::Null),
            Acc::Values(mut v, p) => {
                if v.is_empty() {
                    return Value::Null;
                }
                v.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
                let pos = (v.len() - 1) as f64 * p / 100.0;
                let (lo, hi) = (pos.floor() as usize, pos.ceil() as usize);
                let x = v[lo] + (v[hi] - v[lo]) * (pos - lo as f64);
                num_value((x * 1e9).round() / 1e9)
            }
            Acc::Select(s) => {
                let mut vals: Vec<Value> = s.into_values().collect();
                vals.sort_by(compare_values);
                Value::Array(vals)
            }
        }
    }
}

/// An aggregation to compute: (label, analysis, target, percentile).
struct Agg {
    label: String,
    analysis: String,
    target: String,
    pct: Option<f64>,
}

fn rank(v: &Value) -> u8 {
    match v {
        Value::Null => 0,
        Value::Bool(_) => 1,
        Value::Number(_) => 2,
        Value::String(_) => 3,
        _ => 4,
    }
}

/// Orders null < bool < number < string < other, then by value.
pub fn compare_values(a: &Value, b: &Value) -> Ordering {
    match rank(a).cmp(&rank(b)) {
        Ordering::Equal => {}
        o => return o,
    }
    match (a, b) {
        (Value::Bool(x), Value::Bool(y)) => x.cmp(y),
        (Value::Number(x), Value::Number(y)) => x.as_f64().partial_cmp(&y.as_f64()).unwrap_or(Ordering::Equal),
        (Value::String(x), Value::String(y)) => x.cmp(y),
        _ => a.to_string().cmp(&b.to_string()),
    }
}

/// A group's accumulated values.
struct Group {
    values: Vec<Value>,
    accs: Vec<Acc>,
}

pub struct Executor<'a> {
    pub engine: &'a Engine,
    pub limits: Limits,
    pub now: DateTime<Utc>,
    /// Events read by queries run so far, for execution metadata.
    pub scanned: std::sync::atomic::AtomicUsize,
}

/// A predicate over stored events for deletes: the timeframe (on the event timestamp) and filters.
pub fn event_matcher(filters: &[Filter], range: Option<Range>) -> Result<impl Fn(&Value) -> bool + Sync, QueryError> {
    let compiled: Vec<Compiled> = filters.iter().map(compile).collect::<Result<_, _>>()?;
    let (s, e) = range.map(|r| (r.start.timestamp_millis(), r.end.timestamp_millis())).unwrap_or((i64::MIN, i64::MAX));
    Ok(move |ev: &Value| {
        let ts = crate::event::event_ts(ev);
        ts >= s
            && ts < e
            && compiled.iter().all(|f| {
                let c = if f.path.is_empty() { Cell::Missing } else { crate::store::table::cell_at(ev, &f.path) };
                f.test(&c, ts)
            })
    })
}

impl<'a> Executor<'a> {
    pub fn new(engine: &'a Engine, limits: Limits, now: DateTime<Utc>) -> Executor<'a> {
        Executor { engine, limits, now, scanned: Default::default() }
    }

    /// Runs q for a project; `scope` filters (from restricted keys) are added to every query.
    pub async fn run(&self, project: &str, q: &Query, scope: &[Filter]) -> Result<Map<String, Value>, QueryError> {
        let zone = parse_timezone(q.timezone.as_ref()).map_err(|e| bad("InvalidTimezoneError", e))?;
        let range = parse_timeframe(q.timeframe.as_ref(), zone, self.now).map_err(|e| bad("TimeframeDefinitionError", e))?;
        let mut out = Map::new();
        out.insert("query".into(), q.echo());
        match q.analysis_type.as_str() {
            "funnel" => {
                let (res, actors) = self.funnel(project, q, range, zone, scope).await?;
                out.insert("result".into(), json!(res));
                out.insert("steps".into(), serde_json::to_value(&q.steps).unwrap());
                if let Some(a) = actors {
                    out.insert("actors".into(), Value::Array(a));
                }
                return Ok(out);
            }
            "extraction" => {
                out.insert("result".into(), Value::Array(self.extraction(project, q, range, scope).await?));
                return Ok(out);
            }
            _ => {}
        }
        let bks = if q.interval.is_empty() {
            None
        } else {
            let r = range.ok_or_else(|| bad("InvalidIntervalError", "interval requires a timeframe"))?;
            Some(buckets(r, &q.interval, zone, self.limits.max_intervals).map_err(|e| bad("InvalidIntervalError", e))?)
        };
        let multi = q.analysis_type == "multi_analysis";
        let aggs: Vec<Agg> = if multi {
            q.analyses
                .iter()
                .map(|(l, a)| Agg { label: l.clone(), analysis: a.analysis_type.clone(), target: a.target_property.clone().unwrap_or_default(), pct: a.percentile })
                .collect()
        } else {
            vec![Agg { label: String::new(), analysis: q.analysis_type.clone(), target: q.target_property.clone(), pct: q.percentile }]
        };
        let mut filters: Vec<Filter> = q.filters.clone();
        filters.extend(scope.iter().cloned());
        let compiled: Vec<Compiled> = filters.iter().map(compile).collect::<Result<_, _>>()?;
        let mut paths: Vec<String> = compiled.iter().filter(|c| !c.path.is_empty()).map(|c| c.path.clone()).collect();
        paths.extend(q.group_by.iter().cloned());
        paths.extend(aggs.iter().filter(|a| !a.target.is_empty()).map(|a| a.target.clone()));
        paths.sort();
        paths.dedup();
        let batches = self.scan(project, &q.event_collection, range, &paths, false).await?;
        let nb = bks.as_ref().map_or(1, |b| b.len());
        let starts: Vec<i64> = bks.as_ref().map(|b| b.iter().map(|r| r.start.timestamp_millis()).collect()).unwrap_or_default();
        let mut groups: Vec<HashMap<String, Group>> = (0..nb).map(|_| HashMap::new()).collect();
        let missing = Cell::Missing;
        let mut ngroups = 0usize;
        for b in &batches {
            let gcols: Vec<&[Cell]> = q.group_by.iter().map(|g| b.col(g).map(|c| c.as_slice()).unwrap_or(&[])).collect();
            let tcols: Vec<Option<&[Cell]>> = aggs.iter().map(|a| b.col(&a.target).map(|c| c.as_slice())).collect();
            for i in matching(b, range, &compiled) {
                let bi = if bks.is_some() { starts.partition_point(|s| *s <= b.ts[i]).saturating_sub(1) } else { 0 };
                let key: String = gcols.iter().map(|c| c.get(i).unwrap_or(&missing).key()).collect::<Vec<_>>().join("\u{1}");
                let g = groups[bi].entry(key).or_insert_with(|| {
                    ngroups += 1;
                    Group {
                        values: gcols.iter().map(|c| c.get(i).unwrap_or(&missing).to_value()).collect(),
                        accs: aggs.iter().map(|a| Acc::new(&a.analysis, a.pct)).collect(),
                    }
                });
                for (k, acc) in g.accs.iter_mut().enumerate() {
                    let c = tcols[k].map(|c| &c[i]).unwrap_or(&missing);
                    acc.add(c);
                }
            }
            if !q.group_by.is_empty() && ngroups > self.limits.max_groups * nb {
                return Err(bad("TooManyGroupsError", format!("group_by produces more than {} groups", self.limits.max_groups)));
            }
        }
        let value_of = |accs: Vec<Acc>| -> Value {
            if multi {
                let mut m = Map::new();
                for (a, acc) in aggs.iter().zip(accs) {
                    m.insert(a.label.clone(), acc.result());
                }
                Value::Object(m)
            } else {
                accs.into_iter().next().unwrap().result()
            }
        };
        let empty = || value_of(aggs.iter().map(|a| Acc::new(&a.analysis, a.pct)).collect());
        let shape_groups = |gs: HashMap<String, Group>| -> Value {
            let mut rows: Vec<(Vec<Value>, Value)> = gs.into_values().map(|g| (g.values, value_of(g.accs))).collect();
            rows.sort_by(|a, b| {
                a.0.iter().zip(&b.0).map(|(x, y)| compare_values(x, y)).find(|o| *o != Ordering::Equal).unwrap_or(Ordering::Equal)
            });
            let mut out: Vec<Map<String, Value>> = rows
                .into_iter()
                .map(|(vals, v)| {
                    let mut m = Map::new();
                    for (g, val) in q.group_by.iter().zip(vals) {
                        m.insert(g.clone(), val);
                    }
                    match v {
                        Value::Object(o) if multi => m.extend(o),
                        v => {
                            m.insert("result".into(), v);
                        }
                    }
                    m
                })
                .collect();
            if !q.order_by.is_empty() {
                out.sort_by(|a, b| {
                    for o in &q.order_by {
                        let c = compare_values(a.get(&o.property_name).unwrap_or(&Value::Null), b.get(&o.property_name).unwrap_or(&Value::Null));
                        if c != Ordering::Equal {
                            return if o.direction.eq_ignore_ascii_case("desc") { c.reverse() } else { c };
                        }
                    }
                    Ordering::Equal
                });
            }
            if let Some(l) = q.limit {
                out.truncate(l);
            }
            Value::Array(out.into_iter().map(Value::Object).collect())
        };
        let result = match bks {
            None => {
                let gs = groups.pop().unwrap();
                if q.group_by.is_empty() {
                    gs.into_values().next().map(|g| value_of(g.accs)).unwrap_or_else(empty)
                } else {
                    shape_groups(gs)
                }
            }
            Some(bks) => Value::Array(
                bks.iter()
                    .zip(groups)
                    .map(|(r, gs)| {
                        let v = if q.group_by.is_empty() {
                            gs.into_values().next().map(|g| value_of(g.accs)).unwrap_or_else(empty)
                        } else {
                            shape_groups(gs)
                        };
                        json!({"timeframe": {"start": zone.format(r.start), "end": zone.format(r.end)}, "value": v})
                    })
                    .collect(),
            ),
        };
        out.insert("result".into(), result);
        Ok(out)
    }

    async fn scan(&self, project: &str, coll: &str, range: Option<Range>, paths: &[String], raw: bool) -> Result<Vec<Batch>, QueryError> {
        let r = range.map(|r| (r.start.timestamp_millis(), r.end.timestamp_millis()));
        let batches = self.engine.scan(project, coll, r, paths, raw).await.map_err(internal)?;
        let n: usize = batches.iter().map(Batch::len).sum();
        self.scanned.fetch_add(n, std::sync::atomic::Ordering::Relaxed);
        if n > self.limits.max_scan {
            return Err(bad("QueryLimitExceededError", format!("the query would scan more than {} events; narrow the timeframe", self.limits.max_scan)));
        }
        Ok(batches)
    }

    async fn extraction(&self, project: &str, q: &Query, range: Option<Range>, scope: &[Filter]) -> Result<Vec<Value>, QueryError> {
        let mut filters = q.filters.clone();
        filters.extend(scope.iter().cloned());
        let compiled: Vec<Compiled> = filters.iter().map(compile).collect::<Result<_, _>>()?;
        let paths: Vec<String> = compiled.iter().filter(|c| !c.path.is_empty()).map(|c| c.path.clone()).collect();
        let batches = self.scan(project, &q.event_collection, range, &paths, true).await?;
        let mut rows: Vec<(i64, std::sync::Arc<str>, std::sync::Arc<str>)> = vec![];
        for b in &batches {
            let raw = b.raw.as_ref().unwrap();
            for i in matching(b, range, &compiled) {
                rows.push((b.ts[i], b.ids[i].clone(), raw[i].clone()));
            }
        }
        rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        if let Some(n) = q.latest {
            let skip = rows.len().saturating_sub(n);
            rows.drain(..skip);
        } else if rows.len() > self.limits.max_extraction {
            return Err(bad("TooManyEventsError", format!("extraction matches more than {} events; narrow the timeframe or use latest", self.limits.max_extraction)));
        }
        Ok(rows
            .into_iter()
            .map(|(_, _, raw)| {
                let ev: Value = serde_json::from_str(&raw).unwrap_or(Value::Null);
                if q.property_names.is_empty() {
                    ev
                } else {
                    project_paths(&ev, &q.property_names)
                }
            })
            .collect())
    }

    async fn funnel(&self, project: &str, q: &Query, range: Option<Range>, zone: Zone, scope: &[Filter]) -> Result<(Vec<usize>, Option<Vec<Value>>), QueryError> {
        let mut current: HashSet<String> = HashSet::new();
        let mut results = vec![];
        let mut actors = vec![];
        let mut any_actors = false;
        for (i, s) in q.steps.iter().enumerate() {
            let mut sr = range;
            if s.timeframe.is_some() || s.timezone.is_some() {
                let z = match &s.timezone {
                    Some(tz) => parse_timezone(Some(tz)).map_err(|e| bad("InvalidTimezoneError", format!("step {i}: {e}")))?,
                    None => zone,
                };
                let tf = s.timeframe.as_ref().or(q.timeframe.as_ref());
                sr = parse_timeframe(tf, z, self.now).map_err(|e| bad("TimeframeDefinitionError", format!("step {i}: {e}")))?;
            }
            let mut filters = s.filters.clone();
            filters.extend(q.filters.iter().cloned());
            filters.extend(scope.iter().cloned());
            let compiled: Vec<Compiled> = filters.iter().map(compile).collect::<Result<_, _>>()?;
            let mut paths: Vec<String> = compiled.iter().filter(|c| !c.path.is_empty()).map(|c| c.path.clone()).collect();
            paths.push(s.actor_property.clone());
            let batches = self.scan(project, &s.event_collection, sr, &paths, false).await?;
            let mut did: HashMap<String, Value> = HashMap::new();
            for b in &batches {
                let Some(col) = b.col(&s.actor_property) else { continue };
                for r in matching(b, sr, &compiled) {
                    if col[r].present() {
                        did.entry(col[r].key()).or_insert_with(|| col[r].to_value());
                    }
                }
                if did.len() > self.limits.max_funnel {
                    return Err(bad("TooManyActorsError", format!("funnel step {i} has more than {} actors", self.limits.max_funnel)));
                }
            }
            let step: HashSet<String> = if i == 0 {
                did.keys().cloned().collect()
            } else {
                current.iter().filter(|a| did.contains_key(*a) != s.inverted).cloned().collect()
            };
            results.push(step.len());
            if s.with_actors {
                any_actors = true;
                // Actors of an inverted step never appear in `did`; they came from earlier steps.
                let mut list: Vec<Value> = step.iter().map(|k| did.get(k).cloned().unwrap_or_else(|| key_value(k))).collect();
                list.sort_by(compare_values);
                actors.push(Value::Array(list));
            } else {
                actors.push(Value::Null);
            }
            if i == 0 || !s.optional {
                current = step;
            }
        }
        Ok((results, if any_actors { Some(actors) } else { None }))
    }
}

/// Inverts Cell::key for scalar keys.
fn key_value(k: &str) -> Value {
    match k.split_at(1) {
        ("s", rest) => Value::String(rest.to_string()),
        ("n", rest) => rest.parse::<f64>().map(num_value).unwrap_or(Value::Null),
        ("j", rest) => serde_json::from_str(rest).unwrap_or(Value::Null),
        _ => match k {
            "true" => Value::Bool(true),
            "false" => Value::Bool(false),
            _ => Value::Null,
        },
    }
}

/// Keeps only the named dotted paths of an event, preserving nesting.
pub fn project_paths(ev: &Value, paths: &[String]) -> Value {
    let mut dst = json!({});
    for p in paths {
        if let Some(v) = crate::store::table::lookup(ev, p) {
            crate::event::set_path(&mut dst, p, v.clone());
        }
    }
    dst
}
