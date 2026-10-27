//! In-memory columnar batches of events.
//!
//! Every property path is a column of [`Cell`]s, which keep the distinction between a missing
//! property, an explicit null, and each JSON type. Queries operate on these columns.

use serde_json::Value;
use std::collections::HashMap;
use std::sync::Arc;

/// One property value in one event.
#[derive(Debug, Clone, PartialEq)]
pub enum Cell {
    Missing,
    Null,
    Bool(bool),
    Num(f64),
    Str(Arc<str>),
    /// A list or object, kept as parsed JSON.
    Json(Arc<Value>),
}

impl Cell {
    pub fn from_value(v: &Value) -> Cell {
        match v {
            Value::Null => Cell::Null,
            Value::Bool(b) => Cell::Bool(*b),
            Value::Number(n) => Cell::Num(n.as_f64().unwrap_or(0.0)),
            Value::String(s) => Cell::Str(Arc::from(s.as_str())),
            other => Cell::Json(Arc::new(other.clone())),
        }
    }

    /// Present and not null.
    pub fn present(&self) -> bool {
        !matches!(self, Cell::Missing | Cell::Null)
    }

    pub fn num(&self) -> Option<f64> {
        match self {
            Cell::Num(n) => Some(*n),
            _ => None,
        }
    }

    pub fn to_value(&self) -> Value {
        match self {
            Cell::Missing | Cell::Null => Value::Null,
            Cell::Bool(b) => Value::Bool(*b),
            Cell::Num(n) => num_value(*n),
            Cell::Str(s) => Value::String(s.to_string()),
            Cell::Json(v) => (**v).clone(),
        }
    }

    /// A canonical string key: equal values (by type and value) have equal keys.
    pub fn key(&self) -> String {
        match self {
            Cell::Missing | Cell::Null => "null".into(),
            Cell::Bool(b) => b.to_string(),
            Cell::Num(n) => format!("n{}", fmt_num(*n)),
            Cell::Str(s) => format!("s{s}"),
            Cell::Json(v) => format!("j{v}"),
        }
    }
}

/// Renders a number as JSON, using an integer when the value is integral.
pub fn num_value(n: f64) -> Value {
    if n.is_finite() && n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
        Value::from(n as i64)
    } else {
        serde_json::Number::from_f64(n).map(Value::Number).unwrap_or(Value::Null)
    }
}

fn fmt_num(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 9.007_199_254_740_992e15 {
        format!("{}", n as i64)
    } else {
        format!("{n}")
    }
}

/// Looks up a dotted path in an event.
pub fn lookup<'a>(ev: &'a Value, path: &str) -> Option<&'a Value> {
    let mut cur = ev;
    for part in path.split('.') {
        cur = cur.as_object()?.get(part)?;
    }
    Some(cur)
}

pub fn cell_at(ev: &Value, path: &str) -> Cell {
    lookup(ev, path).map(Cell::from_value).unwrap_or(Cell::Missing)
}

/// A batch of events from one collection.
#[derive(Debug, Default)]
pub struct Batch {
    /// Event timestamps in milliseconds since the epoch.
    pub ts: Vec<i64>,
    pub ids: Vec<Arc<str>>,
    /// Columns by dotted path. Only the paths a query asked for are guaranteed present.
    pub cols: HashMap<String, Arc<Vec<Cell>>>,
    /// Full event JSON, when requested.
    pub raw: Option<Arc<Vec<Arc<str>>>>,
}

impl Batch {
    pub fn len(&self) -> usize {
        self.ts.len()
    }

    pub fn is_empty(&self) -> bool {
        self.ts.is_empty()
    }

    pub fn col(&self, path: &str) -> Option<&Arc<Vec<Cell>>> {
        self.cols.get(path)
    }

    /// Builds a batch from parsed events, extracting the given paths.
    pub fn from_events(events: &[Value], ts: Vec<i64>, ids: Vec<Arc<str>>, paths: &[String], raw: Option<Vec<Arc<str>>>) -> Batch {
        let mut cols = HashMap::with_capacity(paths.len());
        for p in paths {
            let col: Vec<Cell> = events.iter().map(|e| cell_at(e, p)).collect();
            cols.insert(p.clone(), Arc::new(col));
        }
        Batch { ts, ids, cols, raw: raw.map(Arc::new) }
    }
}

/// Leaf paths and their JSON type names, for schema discovery and column shredding.
pub fn leaves(v: &Value, prefix: &str, out: &mut Vec<(String, &'static str)>) {
    if let Value::Object(m) = v {
        for (k, child) in m {
            let path = if prefix.is_empty() { k.clone() } else { format!("{prefix}.{k}") };
            match child {
                Value::Object(sub) if !sub.is_empty() => leaves(child, &path, out),
                _ => out.push((path, type_name(child))),
            }
        }
    }
}

pub fn type_name(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "num",
        Value::String(s) => {
            if s.len() >= 20 && chrono::DateTime::parse_from_rfc3339(s).is_ok() {
                "datetime"
            } else {
                "string"
            }
        }
        Value::Array(_) => "list",
        Value::Object(_) => "object",
    }
}
