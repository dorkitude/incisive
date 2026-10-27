//! Analysis requests: parsing and validation. Execution is in [`exec`].

pub mod exec;
pub mod timeframe;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use std::collections::BTreeMap;

/// A client-visible query error.
#[derive(Debug, Clone)]
pub struct QueryError {
    pub code: &'static str,
    pub message: String,
    pub status: u16,
}

impl std::fmt::Display for QueryError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for QueryError {}

pub fn bad(code: &'static str, message: impl Into<String>) -> QueryError {
    QueryError { code, message: message.into(), status: 400 }
}

pub const ANALYSES: [&str; 12] = [
    "count", "count_unique", "sum", "average", "minimum", "maximum", "median", "percentile", "select_unique", "extraction", "funnel",
    "multi_analysis",
];

fn needs_target(a: &str) -> bool {
    matches!(a, "count_unique" | "sum" | "average" | "minimum" | "maximum" | "median" | "percentile" | "select_unique")
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Filter {
    pub property_name: String,
    pub operator: String,
    #[serde(default)]
    pub property_value: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderBy {
    pub property_name: String,
    #[serde(default)]
    pub direction: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Step {
    pub event_collection: String,
    pub actor_property: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeframe: Option<Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timezone: Option<Value>,
    #[serde(default)]
    pub optional: bool,
    #[serde(default)]
    pub inverted: bool,
    #[serde(default)]
    pub with_actors: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubAnalysis {
    pub analysis_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target_property: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percentile: Option<f64>,
}

#[derive(Debug, Clone, Default)]
pub struct Query {
    pub analysis_type: String,
    pub event_collection: String,
    pub target_property: String,
    pub percentile: Option<f64>,
    pub filters: Vec<Filter>,
    pub timeframe: Option<Value>,
    pub timezone: Option<Value>,
    pub interval: String,
    pub group_by: Vec<String>,
    pub group_by_raw: Option<Value>,
    pub order_by: Vec<OrderBy>,
    pub limit: Option<usize>,
    pub property_names: Vec<String>,
    pub latest: Option<usize>,
    pub steps: Vec<Step>,
    pub analyses: BTreeMap<String, SubAnalysis>,
    pub include_metadata: bool,
}

const STRUCTURED: [&str; 9] = ["filters", "group_by", "order_by", "steps", "analyses", "property_names", "timeframe", "timezone", "percentile"];

/// Turns GET query parameters into the map a POST JSON body would decode to: structured
/// parameters arrive JSON-encoded, scalars as plain strings. `api_key` is dropped.
pub fn from_query_pairs<'a>(pairs: impl Iterator<Item = (String, String)> + 'a) -> Map<String, Value> {
    let mut m = Map::new();
    for (k, v) in pairs {
        if k == "api_key" || m.contains_key(&k) {
            continue;
        }
        if STRUCTURED.contains(&k.as_str())
            && let Ok(x) = serde_json::from_str::<Value>(&v) {
                m.insert(k, x);
                continue;
            }
        m.insert(k, Value::String(v));
    }
    m
}

const KNOWN: [&str; 21] = [
    "analysis_type", "event_collection", "target_property", "percentile", "filters", "timeframe", "timezone", "interval", "group_by",
    "order_by", "limit", "property_names", "latest", "steps", "analyses", "include_metadata", "api_key", "force_refresh", "max_age",
    "zero_fill", "modified",
];

fn as_f64(v: &Value) -> Option<f64> {
    match v {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.parse().ok(),
        _ => None,
    }
}

fn as_count(v: &Value, name: &str) -> Result<usize, QueryError> {
    match as_f64(v) {
        Some(f) if f >= 1.0 && f.fract() == 0.0 => Ok(f as usize),
        _ => Err(bad("InvalidParameterError", format!("{name} must be a positive integer"))),
    }
}

fn from<T: serde::de::DeserializeOwned>(v: &Value, code: &'static str, msg: &str) -> Result<T, QueryError> {
    serde_json::from_value(v.clone()).map_err(|_| bad(code, msg))
}

/// Validates a request for the given analysis type.
pub fn parse(analysis: &str, m: &Map<String, Value>) -> Result<Query, QueryError> {
    if !ANALYSES.contains(&analysis) {
        return Err(QueryError { code: "ResourceNotFoundError", message: format!("unknown analysis type {analysis:?}"), status: 404 });
    }
    if m.contains_key("email") {
        return Err(bad("NotImplementedError", "asynchronous (email) extractions are not supported"));
    }
    let mut unknown: Vec<&str> = m.keys().map(String::as_str).filter(|k| !KNOWN.contains(k)).collect();
    if !unknown.is_empty() {
        unknown.sort();
        return Err(bad("InvalidParameterError", format!("unsupported parameter(s): {}", unknown.join(", "))));
    }
    let s = |k: &str| m.get(k).and_then(Value::as_str).unwrap_or("").to_string();
    let mut q = Query {
        analysis_type: analysis.to_string(),
        event_collection: s("event_collection"),
        target_property: s("target_property"),
        interval: s("interval"),
        timeframe: m.get("timeframe").filter(|v| !v.is_null()).cloned(),
        timezone: m.get("timezone").filter(|v| !v.is_null()).cloned(),
        include_metadata: matches!(m.get("include_metadata"), Some(Value::Bool(true))) || s("include_metadata") == "true",
        ..Default::default()
    };
    if let Some(v) = m.get("percentile") {
        match as_f64(v) {
            Some(p) if (0.0..=100.0).contains(&p) => q.percentile = Some(p),
            _ => return Err(bad("InvalidParameterError", "percentile must be a number between 0 and 100")),
        }
    } else if analysis == "percentile" {
        return Err(bad("MissingRequiredPropertyError", "percentile is required for percentile analyses"));
    }
    if let Some(v) = m.get("filters").filter(|v| !v.is_null()) {
        q.filters = from(v, "InvalidFilterError", "filters must be a list of filter objects")?;
    }
    if let Some(v) = m.get("group_by").filter(|v| !v.is_null()) {
        q.group_by_raw = Some(v.clone());
        match v {
            Value::String(g) if !g.is_empty() => q.group_by = vec![g.clone()],
            Value::String(_) => {}
            Value::Array(a) => {
                for g in a {
                    match g.as_str() {
                        Some(g) if !g.is_empty() => q.group_by.push(g.to_string()),
                        _ => return Err(bad("InvalidParameterError", "group_by must be a property name or a list of property names")),
                    }
                }
            }
            _ => return Err(bad("InvalidParameterError", "group_by must be a property name or a list of property names")),
        }
    }
    if let Some(v) = m.get("order_by").filter(|v| !v.is_null()) {
        q.order_by = match v {
            Value::String(p) => vec![OrderBy { property_name: p.clone(), direction: String::new() }],
            Value::Object(_) => vec![from(v, "InvalidParameterError", "invalid order_by")?],
            _ => from(v, "InvalidParameterError", "invalid order_by")?,
        };
    }
    if let Some(v) = m.get("limit").filter(|v| !v.is_null()) {
        q.limit = Some(as_count(v, "limit")?);
    }
    if let Some(v) = m.get("latest").filter(|v| !v.is_null()) {
        q.latest = Some(as_count(v, "latest")?);
    }
    if let Some(v) = m.get("property_names").filter(|v| !v.is_null()) {
        q.property_names = from(v, "InvalidParameterError", "property_names must be a list of property names")?;
    }
    if let Some(v) = m.get("steps").filter(|v| !v.is_null()) {
        q.steps = from(v, "InvalidParameterError", "steps must be a list of step objects")?;
    }
    if let Some(v) = m.get("analyses").filter(|v| !v.is_null()) {
        q.analyses = from(v, "InvalidParameterError", "analyses must be an object of named analyses")?;
    }
    q.validate()?;
    Ok(q)
}

pub const OPERATORS: [&str; 11] = ["eq", "ne", "lt", "lte", "gt", "gte", "exists", "in", "contains", "not_contains", "within"];

pub fn validate_filters(fs: &[Filter]) -> Result<(), QueryError> {
    for f in fs {
        if f.property_name.is_empty() {
            return Err(bad("InvalidFilterError", "filter requires property_name"));
        }
        if !OPERATORS.contains(&f.operator.as_str()) {
            return Err(bad("InvalidFilterError", format!("unsupported filter operator {:?}", f.operator)));
        }
    }
    Ok(())
}

impl Query {
    fn validate(&self) -> Result<(), QueryError> {
        match self.analysis_type.as_str() {
            "funnel" => {
                if self.steps.is_empty() {
                    return Err(bad("MissingRequiredPropertyError", "funnel requires steps"));
                }
                for (i, s) in self.steps.iter().enumerate() {
                    if s.event_collection.is_empty() || s.actor_property.is_empty() {
                        return Err(bad("MissingRequiredPropertyError", format!("funnel step {i} requires event_collection and actor_property")));
                    }
                    if i == 0 && (s.optional || s.inverted) {
                        return Err(bad("InvalidParameterError", "the first funnel step cannot be optional or inverted"));
                    }
                    validate_filters(&s.filters)?;
                }
                return validate_filters(&self.filters);
            }
            "multi_analysis" => {
                if self.analyses.is_empty() {
                    return Err(bad("MissingRequiredPropertyError", "multi_analysis requires analyses"));
                }
                for (label, a) in &self.analyses {
                    let t = a.analysis_type.as_str();
                    if matches!(t, "extraction" | "funnel" | "multi_analysis" | "") || !ANALYSES.contains(&t) {
                        return Err(bad("InvalidParameterError", format!("analysis {label:?}: analysis_type {t:?} is not allowed in multi_analysis")));
                    }
                    if needs_target(t) && a.target_property.as_deref().unwrap_or("").is_empty() {
                        return Err(bad("MissingRequiredPropertyError", format!("analysis {label:?} requires target_property")));
                    }
                    if t == "percentile" && a.percentile.is_none() {
                        return Err(bad("MissingRequiredPropertyError", format!("analysis {label:?} requires percentile")));
                    }
                }
            }
            _ => {}
        }
        if self.event_collection.is_empty() {
            return Err(bad("MissingRequiredPropertyError", "event_collection is required"));
        }
        if needs_target(&self.analysis_type) && self.target_property.is_empty() {
            return Err(bad("MissingRequiredPropertyError", format!("target_property is required for {}", self.analysis_type)));
        }
        if self.analysis_type == "extraction" && (!self.interval.is_empty() || !self.group_by.is_empty()) {
            return Err(bad("InvalidParameterError", "extraction does not support interval or group_by"));
        }
        if (!self.order_by.is_empty() || self.limit.is_some()) && self.group_by.is_empty() {
            return Err(bad("InvalidParameterError", "order_by and limit require group_by"));
        }
        validate_filters(&self.filters)
    }

    /// The normalized query echoed in responses.
    pub fn echo(&self) -> Value {
        let mut m = Map::new();
        m.insert("analysis_type".into(), Value::String(self.analysis_type.clone()));
        let mut put = |k: &str, v: Value, ok: bool| {
            if ok {
                m.insert(k.into(), v);
            }
        };
        put("event_collection", Value::String(self.event_collection.clone()), !self.event_collection.is_empty());
        put("target_property", Value::String(self.target_property.clone()), !self.target_property.is_empty());
        put("percentile", self.percentile.map(crate::store::table::num_value).unwrap_or(Value::Null), self.percentile.is_some());
        put("filters", serde_json::to_value(&self.filters).unwrap(), true);
        put("timeframe", self.timeframe.clone().unwrap_or(Value::Null), self.timeframe.is_some());
        put("timezone", self.timezone.clone().unwrap_or(Value::Null), self.timezone.is_some());
        put("interval", Value::String(self.interval.clone()), !self.interval.is_empty());
        put("group_by", self.group_by_raw.clone().unwrap_or(Value::Null), self.group_by_raw.is_some());
        put("order_by", serde_json::to_value(&self.order_by).unwrap(), !self.order_by.is_empty());
        put("limit", self.limit.map(Value::from).unwrap_or(Value::Null), self.limit.is_some());
        put("property_names", serde_json::to_value(&self.property_names).unwrap(), !self.property_names.is_empty());
        put("latest", self.latest.map(Value::from).unwrap_or(Value::Null), self.latest.is_some());
        put("steps", serde_json::to_value(&self.steps).unwrap(), !self.steps.is_empty());
        put("analyses", serde_json::to_value(&self.analyses).unwrap(), !self.analyses.is_empty());
        Value::Object(m)
    }
}
