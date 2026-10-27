//! Prometheus metrics.

use crate::store::Engine;
use prometheus::{Encoder, IntCounter, IntGauge, Registry, TextEncoder};
use std::sync::atomic::Ordering;

pub struct Metrics {
    registry: Registry,
    pub events_written: IntCounter,
    pub events_ignored: IntCounter,
    pub write_errors: IntCounter,
    pub queries: IntCounter,
    pub query_errors: IntCounter,
    wal_pending: IntGauge,
    wal_commits: IntGauge,
    wal_commit_failures: IntGauge,
    compactions: IntGauge,
    compaction_failures: IntGauge,
    cache_hits: IntGauge,
    cache_misses: IntGauge,
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

impl Metrics {
    pub fn new() -> Metrics {
        let registry = Registry::new_custom(Some("incisive".into()), None).unwrap();
        let counter = |n: &str, h: &str| {
            let c = IntCounter::new(n, h).unwrap();
            registry.register(Box::new(c.clone())).unwrap();
            c
        };
        let gauge = |n: &str, h: &str| {
            let g = IntGauge::new(n, h).unwrap();
            registry.register(Box::new(g.clone())).unwrap();
            g
        };
        Metrics {
            events_written: counter("events_written_total", "Events durably written."),
            events_ignored: counter("events_ignored_total", "Events dropped because a project member ignores their own address."),
            write_errors: counter("write_errors_total", "Write requests that failed to commit."),
            queries: counter("queries_total", "Queries run."),
            query_errors: counter("query_errors_total", "Queries that failed."),
            wal_pending: gauge("wal_pending_objects", "WAL objects not yet compacted."),
            wal_commits: gauge("wal_commits_total", "WAL objects committed."),
            wal_commit_failures: gauge("wal_commit_failures_total", "WAL commits that failed."),
            compactions: gauge("compactions_total", "Compactions completed."),
            compaction_failures: gauge("compaction_failures_total", "Compactions that failed."),
            cache_hits: gauge("disk_cache_hits_total", "Disk cache hits."),
            cache_misses: gauge("disk_cache_misses_total", "Disk cache misses."),
            registry,
        }
    }

    pub fn render(&self, engine: &Engine) -> String {
        let s = &engine.stats;
        self.wal_pending.set(engine.pending_wal() as i64);
        self.wal_commits.set(s.wal_commits.load(Ordering::Relaxed) as i64);
        self.wal_commit_failures.set(s.wal_commit_failures.load(Ordering::Relaxed) as i64);
        self.compactions.set(s.compactions.load(Ordering::Relaxed) as i64);
        self.compaction_failures.set(s.compaction_failures.load(Ordering::Relaxed) as i64);
        self.cache_hits.set(engine.disk.hits.load(Ordering::Relaxed) as i64);
        self.cache_misses.set(engine.disk.misses.load(Ordering::Relaxed) as i64);
        let mut buf = vec![];
        TextEncoder::new().encode(&self.registry.gather(), &mut buf).unwrap();
        String::from_utf8(buf).unwrap_or_default()
    }
}
