//! Web analytics for the dashboard, computed in one pass over the standard pageview collection.

use crate::query::timeframe::{buckets, Range, Zone};
use crate::standard::PAGEVIEW;
use crate::store::table::Cell;
use crate::store::Engine;
use anyhow::Result;
use std::collections::{HashMap, HashSet};

const PATHS: [&str; 16] = [
    "visitor.id",
    "session.id",
    "session.elapsed_seconds",
    "page.path",
    "page.hostname",
    "session.landing_page.path",
    "session.channel",
    "session.referrer.source",
    "session.referrer.hostname",
    "session.utm.source",
    "session.utm.campaign",
    "geo.country",
    "browser.name",
    "os.name",
    "device.type",
    "user_agent.is_bot",
];

#[derive(Default, Clone, Copy, Debug)]
pub struct Totals {
    pub pageviews: u64,
    pub visitors: u64,
    pub sessions: u64,
    pub bounces: u64,
    pub duration_secs: f64,
}

impl Totals {
    pub fn bounce_rate(&self) -> Option<f64> {
        (self.sessions > 0).then(|| self.bounces as f64 / self.sessions as f64)
    }
    pub fn pages_per_session(&self) -> Option<f64> {
        (self.sessions > 0).then(|| self.pageviews as f64 / self.sessions as f64)
    }
    pub fn avg_duration(&self) -> Option<f64> {
        (self.sessions > 0).then(|| self.duration_secs / self.sessions as f64)
    }
}

/// One row of a breakdown: label, primary count, unique visitors.
#[derive(Clone, Debug)]
pub struct Row {
    pub label: String,
    pub count: u64,
    pub visitors: u64,
}

#[derive(Debug)]
pub struct Bucket {
    pub range: Range,
    pub totals: Totals,
}

#[derive(Debug)]
pub struct Report {
    pub totals: Totals,
    pub previous: Totals,
    pub series: Vec<Bucket>,
    pub pages: Vec<Row>,
    pub landing: Vec<Row>,
    pub channels: Vec<Row>,
    pub sources: Vec<Row>,
    pub campaigns: Vec<Row>,
    pub countries: Vec<Row>,
    pub browsers: Vec<Row>,
    pub oses: Vec<Row>,
    pub devices: Vec<Row>,
    pub bots_excluded: u64,
}

#[derive(Default)]
struct Session {
    pageviews: u64,
    first_ts: i64,
    last_ts: i64,
    max_elapsed: f64,
    first_bucket: usize,
}

fn text(c: Option<&Cell>) -> Option<&str> {
    match c {
        Some(Cell::Str(s)) if !s.is_empty() => Some(s),
        _ => None,
    }
}

#[derive(Default)]
struct Counter(HashMap<String, (u64, HashSet<String>)>);

impl Counter {
    fn add(&mut self, label: &str, visitor: &str) {
        let e = self.0.entry(label.to_string()).or_default();
        e.0 += 1;
        if !visitor.is_empty() {
            e.1.insert(visitor.to_string());
        }
    }
    fn top(self, n: usize) -> Vec<Row> {
        let mut rows: Vec<Row> = self.0.into_iter().map(|(label, (count, v))| Row { label, count, visitors: v.len() as u64 }).collect();
        rows.sort_by(|a, b| b.count.cmp(&a.count).then_with(|| a.label.cmp(&b.label)));
        rows.truncate(n);
        rows
    }
}

/// Aggregates one range: totals plus, when `detail`, per-bucket series and breakdowns.
async fn aggregate(engine: &Engine, project: &str, range: Range, bks: &[Range], detail: bool) -> Result<(Totals, Vec<Totals>, Option<Breakdowns>, u64)> {
    let paths: Vec<String> = PATHS.iter().map(|s| s.to_string()).collect();
    let batches = engine.scan(project, PAGEVIEW, Some((range.start.timestamp_millis(), range.end.timestamp_millis())), &paths, false).await?;
    let (s, e) = (range.start.timestamp_millis(), range.end.timestamp_millis());
    let starts: Vec<i64> = bks.iter().map(|r| r.start.timestamp_millis()).collect();
    let mut sessions: HashMap<String, Session> = HashMap::new();
    let mut visitors: HashSet<String> = HashSet::new();
    let mut per_bucket: Vec<(u64, HashSet<String>)> = (0..bks.len()).map(|_| Default::default()).collect();
    let mut b = Breakdowns::default();
    let mut pageviews = 0u64;
    let mut bots = 0u64;
    for batch in &batches {
        let col = |p: &str| batch.col(p);
        let cols: Vec<_> = PATHS.iter().map(|p| col(p)).collect();
        let at = |k: usize, i: usize| cols[k].map(|c| &c[i]);
        for i in 0..batch.len() {
            let ts = batch.ts[i];
            if ts < s || ts >= e {
                continue;
            }
            if matches!(at(15, i), Some(Cell::Bool(true))) {
                bots += 1;
                continue;
            }
            pageviews += 1;
            let visitor = text(at(0, i)).unwrap_or("");
            if !visitor.is_empty() {
                visitors.insert(visitor.to_string());
            }
            let bi = if starts.is_empty() { 0 } else { starts.partition_point(|x| *x <= ts).saturating_sub(1) };
            if let Some(pb) = per_bucket.get_mut(bi) {
                pb.0 += 1;
                if !visitor.is_empty() {
                    pb.1.insert(visitor.to_string());
                }
            }
            let sid = text(at(1, i)).map(String::from).unwrap_or_else(|| format!("~{visitor}~{ts}"));
            let new_session = !sessions.contains_key(&sid);
            let ss = sessions.entry(sid).or_insert_with(|| Session { first_ts: ts, last_ts: ts, first_bucket: bi, ..Default::default() });
            ss.pageviews += 1;
            ss.first_ts = ss.first_ts.min(ts);
            ss.last_ts = ss.last_ts.max(ts);
            if let Some(Cell::Num(x)) = at(2, i) {
                ss.max_elapsed = ss.max_elapsed.max(*x);
            }
            if detail {
                b.pages.add(text(at(3, i)).unwrap_or("(unknown)"), visitor);
                if new_session {
                    let channel = text(at(6, i)).unwrap_or("Direct");
                    b.channels.add(channel, visitor);
                    let source = text(at(9, i)).or(text(at(7, i))).or(text(at(8, i))).unwrap_or("(direct)");
                    b.sources.add(source, visitor);
                    if let Some(c) = text(at(10, i)) {
                        b.campaigns.add(c, visitor);
                    }
                    b.landing.add(text(at(5, i)).or(text(at(3, i))).unwrap_or("(unknown)"), visitor);
                    b.countries.add(text(at(11, i)).unwrap_or("Unknown"), visitor);
                    b.browsers.add(text(at(12, i)).unwrap_or("Unknown"), visitor);
                    b.oses.add(text(at(13, i)).unwrap_or("Unknown"), visitor);
                    b.devices.add(text(at(14, i)).unwrap_or("unknown"), visitor);
                }
            }
        }
    }
    let mut totals = Totals { pageviews, visitors: visitors.len() as u64, sessions: sessions.len() as u64, ..Default::default() };
    let mut series: Vec<Totals> = per_bucket.into_iter().map(|(pv, v)| Totals { pageviews: pv, visitors: v.len() as u64, ..Default::default() }).collect();
    for ss in sessions.values() {
        let dur = ss.max_elapsed.max((ss.last_ts - ss.first_ts) as f64 / 1000.0);
        totals.duration_secs += dur;
        let bounce = ss.pageviews == 1;
        totals.bounces += bounce as u64;
        if let Some(t) = series.get_mut(ss.first_bucket) {
            t.sessions += 1;
            t.bounces += bounce as u64;
            t.duration_secs += dur;
        }
    }
    Ok((totals, series, detail.then_some(b), bots))
}

#[derive(Default)]
struct Breakdowns {
    pages: Counter,
    landing: Counter,
    channels: Counter,
    sources: Counter,
    campaigns: Counter,
    countries: Counter,
    browsers: Counter,
    oses: Counter,
    devices: Counter,
}

/// Builds the dashboard report for a range, compared with the preceding range of equal length.
pub async fn report(engine: &Engine, project: &str, range: Range, interval: &str, zone: Zone) -> Result<Report> {
    let bks = buckets(range, interval, zone, 1000).map_err(|e| anyhow::anyhow!(e))?;
    let len = range.end - range.start;
    let prev = Range { start: range.start - len, end: range.start };
    let ((totals, series, b, bots), (previous, _, _, _)) =
        tokio::try_join!(aggregate(engine, project, range, &bks, true), aggregate(engine, project, prev, &[], false))?;
    let b = b.unwrap_or_default();
    Ok(Report {
        totals,
        previous,
        series: bks.into_iter().zip(series).map(|(range, totals)| Bucket { range, totals }).collect(),
        pages: b.pages.top(10),
        landing: b.landing.top(10),
        channels: b.channels.top(10),
        sources: b.sources.top(10),
        campaigns: b.campaigns.top(10),
        countries: b.countries.top(10),
        browsers: b.browsers.top(8),
        oses: b.oses.top(8),
        devices: b.devices.top(5),
        bots_excluded: bots,
    })
}

/// Visitors with a pageview in the last 30 minutes.
pub async fn live_visitors(engine: &Engine, project: &str) -> Result<u64> {
    let now = chrono::Utc::now();
    let r = Range { start: now - chrono::Duration::minutes(30), end: now + chrono::Duration::minutes(1) };
    let (t, _, _, _) = aggregate(engine, project, r, &[], false).await?;
    Ok(t.visitors)
}
