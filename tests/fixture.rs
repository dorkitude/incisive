//! Hand-computed query fixture, run against events in the WAL and again after compaction.

use chrono::{TimeZone, Utc};
use incisive::config::Config;
use incisive::event::Preparer;
use incisive::query::exec::{Executor, Limits};
use incisive::store::{objstore, Engine};
use serde_json::Value;

fn approx_eq(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => (x.as_f64().unwrap() - y.as_f64().unwrap()).abs() < 1e-9,
        (Value::Array(x), Value::Array(y)) => x.len() == y.len() && x.iter().zip(y).all(|(a, b)| approx_eq(a, b)),
        (Value::Object(x), Value::Object(y)) => x.len() == y.len() && x.iter().all(|(k, v)| y.get(k).is_some_and(|w| approx_eq(v, w))),
        _ => a == b,
    }
}

pub fn test_config(dir: &std::path::Path) -> Config {
    let mut cfg = Config::default();
    cfg.storage.backend = "fs".into();
    cfg.storage.fs_path = dir.join("bucket");
    cfg.storage.prefix = "t/".into();
    cfg.cache.dir = dir.join("cache");
    cfg.compaction.enabled = false;
    cfg.wal.group_commit_ms = 1;
    cfg
}

async fn run_cases(engine: &Engine, fx: &Value, stage: &str) -> Vec<String> {
    let now = Utc.with_ymd_and_hms(2026, 3, 12, 0, 0, 0).unwrap();
    let limits = Limits { max_extraction: 1000, max_funnel: 1000, max_groups: 1000, max_intervals: 1000, max_scan: 1_000_000 };
    let ex = Executor::new(engine, limits, now);
    let mut failures = vec![];
    for c in fx["cases"].as_array().unwrap() {
        let name = c["name"].as_str().unwrap();
        let mut params = c["params"].as_object().unwrap().clone();
        params.entry("timeframe").or_insert_with(|| fx["all_time"].clone());
        let q = match incisive::query::parse(c["analysis"].as_str().unwrap(), &params) {
            Ok(q) => q,
            Err(e) => {
                failures.push(format!("[{stage}] {name}: parse error {e}"));
                continue;
            }
        };
        match ex.run("p1", &q, &[]).await {
            Ok(out) => {
                if !approx_eq(&out["result"], &c["result"]) {
                    failures.push(format!("[{stage}] {name}:\n   got  {}\n   want {}", out["result"], c["result"]));
                }
                if let Some(want) = c.get("actors") {
                    let got = out.get("actors").cloned().unwrap_or(Value::Null);
                    if !approx_eq(&got, want) {
                        failures.push(format!("[{stage}] {name} actors:\n   got  {got}\n   want {want}"));
                    }
                }
            }
            Err(e) => failures.push(format!("[{stage}] {name}: {} {}", e.code, e.message)),
        }
    }
    failures
}

#[tokio::test(flavor = "multi_thread")]
async fn fixture_cases() {
    let fx: Value = serde_json::from_str(include_str!("data/fixture.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let cfg = test_config(dir.path());
    let engine = Engine::open(&cfg, objstore::build(&cfg.storage).unwrap()).await.unwrap();
    let prep = Preparer { max_event_bytes: 0, geo: None };
    let now = Utc.with_ymd_and_hms(2026, 3, 11, 0, 0, 0).unwrap();
    for (coll, evs) in fx["events"].as_object().unwrap() {
        let stored = evs.as_array().unwrap().iter().map(|e| prep.prepare(coll, e.clone(), None, &Default::default(), now).unwrap()).collect();
        engine.append("p1", stored).await.unwrap();
    }
    let mut failures = run_cases(&engine, &fx, "wal").await;
    engine.compact_all(true).await.unwrap();
    assert_eq!(engine.pending_wal(), 0, "compaction should absorb all WAL");
    failures.extend(run_cases(&engine, &fx, "segments").await);
    // A fresh engine over the same storage sees the same data.
    drop(engine);
    let cfg2 = Config { cache: incisive::config::Cache { dir: dir.path().join("cache2"), ..cfg.cache.clone() }, ..cfg.clone() };
    let engine = Engine::open(&cfg2, objstore::build(&cfg2.storage).unwrap()).await.unwrap();
    failures.extend(run_cases(&engine, &fx, "reopened").await);
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}

/// The fixture against real object storage. Run with
/// `INCISIVE_SMOKE_CONFIG=deploy/incisive.dev.toml cargo test --test fixture -- --ignored`.
/// Uses a unique prefix under `smoke/` and deletes what it wrote.
#[tokio::test(flavor = "multi_thread")]
#[ignore]
async fn fixture_cases_object_storage() {
    let Ok(path) = std::env::var("INCISIVE_SMOKE_CONFIG") else { return };
    let fx: Value = serde_json::from_str(include_str!("data/fixture.json")).unwrap();
    let dir = tempfile::tempdir().unwrap();
    let mut cfg = Config::load(Some(std::path::Path::new(&path))).unwrap();
    cfg.storage.prefix = format!("smoke/{}/", ulid::Ulid::generate());
    cfg.cache.dir = dir.path().join("cache");
    cfg.compaction.enabled = false;
    let store = objstore::build(&cfg.storage).unwrap();
    let engine = Engine::open(&cfg, store.clone()).await.unwrap();
    let prep = Preparer { max_event_bytes: 0, geo: None };
    let now = Utc.with_ymd_and_hms(2026, 3, 11, 0, 0, 0).unwrap();
    for (coll, evs) in fx["events"].as_object().unwrap() {
        let stored = evs.as_array().unwrap().iter().map(|e| prep.prepare(coll, e.clone(), None, &Default::default(), now).unwrap()).collect();
        engine.append("p1", stored).await.unwrap();
    }
    let mut failures = run_cases(&engine, &fx, "s3-wal").await;
    engine.compact_all(true).await.unwrap();
    failures.extend(run_cases(&engine, &fx, "s3-segments").await);
    let keys = engine.list_objects("").await.unwrap();
    assert!(keys.iter().any(|k| k.contains("segments/")) && keys.iter().any(|k| k.contains("manifests/")), "{keys:?}");
    for k in keys {
        object_store::ObjectStoreExt::delete(&*store, &object_store::path::Path::from(k.as_str())).await.ok();
    }
    assert!(failures.is_empty(), "{} failures:\n{}", failures.len(), failures.join("\n"));
}
