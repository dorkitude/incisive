//! The storage engine. Object storage (R2) is the source of truth:
//!
//! - `wal/{project}/{collection}/{ulid}_{min_ts}_{max_ts}.ndjson.zst`: committed write batches.
//!   A write is acknowledged only after its WAL object is stored.
//! - `segments/{project}/{collection}/{ulid}.parquet`: compacted, column-split events.
//! - `manifests/{project}.json`: which segments are live and which WAL objects they absorbed,
//!   replaced with conditional writes.
//!
//! This node keeps an index of WAL objects, a local disk cache of objects, and an in-memory
//! cache of decoded columns. Queries read live segments plus WAL objects not yet absorbed.

pub mod cache;
pub mod objstore;
pub mod segment;
pub mod table;

use crate::config::Config;
use anyhow::{anyhow, Context, Result};
use bytes::Bytes;
use cache::DiskCache;
use futures::{StreamExt, TryStreamExt};
use object_store::{path::Path, ObjectStore, ObjectStoreExt, PutMode, PutOptions, PutPayload, UpdateVersion};
use parking_lot::{Mutex, RwLock};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{BTreeMap, BTreeSet, HashMap};
use std::sync::Arc;
use table::{Batch, Cell};
use tokio::sync::{mpsc, oneshot};

/// An event ready for storage: validated JSON including the reserved metadata object.
#[derive(Debug, Clone)]
pub struct StoredEvent {
    pub collection: String,
    pub ts: i64,
    pub id: String,
    pub event: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Manifest {
    pub version: u64,
    pub collections: BTreeMap<String, CollectionState>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CollectionState {
    pub segments: Vec<SegmentMeta>,
    /// WAL keys whose events are in segments (hidden from queries until deleted).
    pub absorbed_wal: BTreeSet<String>,
    /// Observed property types (path -> type name) from compacted events.
    pub properties: BTreeMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SegmentMeta {
    pub key: String,
    pub rows: usize,
    pub min_ts: i64,
    pub max_ts: i64,
    pub bytes: usize,
}

#[derive(Debug, Clone)]
struct WalInfo {
    min_ts: i64,
    max_ts: i64,
    created_ms: u64,
}

struct ManifestState {
    m: Manifest,
    etag: Option<String>,
}

/// Decoded contents of one object, filled in lazily per column.
struct Loaded {
    ts: Arc<Vec<i64>>,
    ids: Arc<Vec<Arc<str>>>,
    cols: Mutex<HashMap<String, Arc<Vec<Cell>>>>,
    raw: Mutex<Option<Arc<Vec<Arc<str>>>>>,
    src: Source,
}

enum Source {
    Wal(Arc<Vec<Value>>),
    Segment { data: Bytes, paths: Arc<BTreeSet<String>> },
}

struct WriteReq {
    project: String,
    events: Vec<StoredEvent>,
    done: oneshot::Sender<Result<(), String>>,
}

/// Counters exported as metrics.
#[derive(Default)]
pub struct Stats {
    pub wal_commits: std::sync::atomic::AtomicU64,
    pub wal_commit_failures: std::sync::atomic::AtomicU64,
    pub compactions: std::sync::atomic::AtomicU64,
    pub compaction_failures: std::sync::atomic::AtomicU64,
}

pub struct Engine {
    store: Arc<dyn ObjectStore>,
    prefix: String,
    pub disk: DiskCache,
    cfg: Config,
    wal: RwLock<HashMap<(String, String), BTreeMap<String, WalInfo>>>,
    manifests: RwLock<HashMap<String, Arc<ManifestState>>>,
    mem: Mutex<lru::LruCache<String, Arc<Loaded>>>,
    writer: mpsc::Sender<WriteReq>,
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub stats: Stats,
}

/// Encodes a collection name as a single safe key segment.
pub fn encode_name(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || b == b'_' || b == b'-' {
            out.push(b as char);
        } else {
            out.push_str(&format!("~{b:02x}"));
        }
    }
    out
}

pub fn decode_name(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'~' && i + 2 < bytes.len() {
            if let Ok(b) = u8::from_str_radix(&s[i + 1..i + 3], 16) {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn now_ms() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_millis() as u64
}

impl Engine {
    /// Opens the engine: indexes existing WAL objects and starts the group-commit writer.
    pub async fn open(cfg: &Config, store: Arc<dyn ObjectStore>) -> Result<Arc<Engine>> {
        let prefix = cfg.storage.prefix.clone();
        let disk = DiskCache::new(cfg.cache.dir.clone(), cfg.cache.max_disk_bytes, store.clone())?;
        let (tx, rx) = mpsc::channel(4096);
        let engine = Arc::new(Engine {
            store,
            prefix,
            disk,
            cfg: cfg.clone(),
            wal: RwLock::new(HashMap::new()),
            manifests: RwLock::new(HashMap::new()),
            mem: Mutex::new(lru::LruCache::new(std::num::NonZeroUsize::new(4096).unwrap())),
            writer: tx,
            locks: Mutex::new(HashMap::new()),
            stats: Stats::default(),
        });
        engine.index_wal().await?;
        tokio::spawn(writer_loop(engine.clone(), rx));
        Ok(engine)
    }

    fn key(&self, rest: &str) -> String {
        format!("{}{}", self.prefix, rest)
    }

    async fn index_wal(&self) -> Result<()> {
        let prefix = Path::from(self.key("wal"));
        let metas: Vec<_> = self.store.list(Some(&prefix)).try_collect().await?;
        let mut idx = self.wal.write();
        for m in metas {
            let key = m.location.to_string();
            if let Some((project, coll, info)) = self.parse_wal_key(&key) {
                idx.entry((project, coll)).or_default().insert(key, info);
            }
        }
        Ok(())
    }

    fn parse_wal_key(&self, key: &str) -> Option<(String, String, WalInfo)> {
        let rest = key.strip_prefix(&self.prefix)?.strip_prefix("wal/")?;
        let mut parts = rest.split('/');
        let project = parts.next()?.to_string();
        let coll = decode_name(parts.next()?);
        let file = parts.next()?.strip_suffix(".ndjson.zst")?;
        let mut f = file.split('_');
        let id = ulid::Ulid::from_string(f.next()?).ok()?;
        let min_ts = f.next()?.parse().ok()?;
        let max_ts = f.next()?.parse().ok()?;
        Some((project, coll, WalInfo { min_ts, max_ts, created_ms: id.timestamp_ms() }))
    }

    /// Durably appends events for one project. Returns once they are committed to object storage.
    pub async fn append(&self, project: &str, events: Vec<StoredEvent>) -> Result<()> {
        if events.is_empty() {
            return Ok(());
        }
        let (done, rx) = oneshot::channel();
        self.writer
            .send(WriteReq { project: project.to_string(), events, done })
            .await
            .map_err(|_| anyhow!("writer stopped"))?;
        rx.await.map_err(|_| anyhow!("writer dropped request"))?.map_err(|e| anyhow!(e))
    }

    async fn commit(&self, project: &str, coll: &str, events: &[StoredEvent]) -> Result<()> {
        let min_ts = events.iter().map(|e| e.ts).min().unwrap_or(0);
        let max_ts = events.iter().map(|e| e.ts).max().unwrap_or(0);
        let key = self.key(&format!("wal/{project}/{}/{}_{min_ts}_{max_ts}.ndjson.zst", encode_name(coll), ulid::Ulid::generate()));
        let mut plain = Vec::with_capacity(events.len() * 256);
        for e in events {
            serde_json::to_writer(&mut plain, &e.event)?;
            plain.push(b'\n');
        }
        let data = Bytes::from(zstd::encode_all(&plain[..], 3)?);
        self.store
            .put_opts(&Path::from(key.as_str()), PutPayload::from(data.clone()), PutOptions { mode: PutMode::Create, ..Default::default() })
            .await
            .with_context(|| format!("writing {key}"))?;
        // Visible to queries on this node immediately after the commit.
        let _ = self.disk.put_local(&key, &data);
        let parsed: Vec<Value> = events.iter().map(|e| e.event.clone()).collect();
        self.mem.lock().put(key.clone(), Arc::new(loaded_wal(parsed)));
        self.wal
            .write()
            .entry((project.to_string(), coll.to_string()))
            .or_default()
            .insert(key, WalInfo { min_ts, max_ts, created_ms: now_ms() });
        Ok(())
    }

    fn project_lock(&self, project: &str) -> Arc<tokio::sync::Mutex<()>> {
        self.locks.lock().entry(project.to_string()).or_default().clone()
    }

    // ---- manifests ----

    fn manifest_key(&self, project: &str) -> String {
        self.key(&format!("manifests/{project}.json"))
    }

    async fn manifest(&self, project: &str) -> Result<Arc<ManifestState>> {
        if let Some(m) = self.manifests.read().get(project) {
            return Ok(m.clone());
        }
        let key = self.manifest_key(project);
        let st = match self.store.get(&Path::from(key.as_str())).await {
            Ok(r) => {
                let etag = r.meta.e_tag.clone();
                let data = r.bytes().await?;
                ManifestState { m: serde_json::from_slice(&data)?, etag }
            }
            Err(object_store::Error::NotFound { .. }) => ManifestState { m: Manifest::default(), etag: None },
            Err(e) => return Err(e.into()),
        };
        let st = Arc::new(st);
        self.manifests.write().insert(project.to_string(), st.clone());
        Ok(st)
    }

    /// Replaces the manifest, failing if someone else changed it since we read it.
    async fn save_manifest(&self, project: &str, prev: &ManifestState, mut m: Manifest) -> Result<()> {
        m.version = prev.m.version + 1;
        let key = self.manifest_key(project);
        let data = Bytes::from(serde_json::to_vec(&m)?);
        let mode = match &prev.etag {
            None => PutMode::Create,
            Some(e) => PutMode::Update(UpdateVersion { e_tag: Some(e.clone()), version: None }),
        };
        let path = Path::from(key.as_str());
        let res = match self.store.put_opts(&path, PutPayload::from(data.clone()), PutOptions { mode, ..Default::default() }).await {
            Err(object_store::Error::NotImplemented { .. }) => {
                // Local filesystem: no conditional writes; this node is the only writer.
                self.store.put(&path, PutPayload::from(data)).await?
            }
            Err(e) => {
                self.manifests.write().remove(project);
                return Err(e).context("manifest changed concurrently");
            }
            Ok(r) => r,
        };
        self.manifests.write().insert(project.to_string(), Arc::new(ManifestState { m, etag: res.e_tag }));
        Ok(())
    }

    // ---- reads ----

    async fn load(&self, key: &str, is_wal: bool) -> Result<Arc<Loaded>> {
        if let Some(l) = self.mem.lock().get(key) {
            return Ok(l.clone());
        }
        let data = self.disk.get(key).await?;
        let loaded = if is_wal {
            let plain = zstd::decode_all(&data[..])?;
            let mut evs = Vec::new();
            for line in plain.split(|b| *b == b'\n') {
                if !line.is_empty() {
                    evs.push(serde_json::from_slice(line)?);
                }
            }
            loaded_wal(evs)
        } else {
            let paths: BTreeSet<String> = segment::stored_paths(&data)?.into_iter().collect();
            let d = segment::read(&data, &[], false)?;
            Loaded {
                ts: Arc::new(d.ts),
                ids: Arc::new(d.ids),
                cols: Mutex::new(HashMap::new()),
                raw: Mutex::new(None),
                src: Source::Segment { data, paths: Arc::new(paths) },
            }
        };
        let loaded = Arc::new(loaded);
        self.mem.lock().put(key.to_string(), loaded.clone());
        Ok(loaded)
    }

    fn batch_from(&self, l: &Loaded, paths: &[String], want_raw: bool) -> Result<Batch> {
        let mut missing: Vec<String> = {
            let cols = l.cols.lock();
            paths.iter().filter(|p| !cols.contains_key(*p)).cloned().collect()
        };
        missing.dedup();
        let need_raw = want_raw && l.raw.lock().is_none();
        if !missing.is_empty() || need_raw {
            match &l.src {
                Source::Wal(evs) => {
                    let mut cols = l.cols.lock();
                    for p in &missing {
                        cols.insert(p.clone(), Arc::new(evs.iter().map(|e| table::cell_at(e, p)).collect()));
                    }
                    if need_raw {
                        *l.raw.lock() = Some(Arc::new(evs.iter().map(|e| Arc::from(e.to_string().as_str())).collect()));
                    }
                }
                Source::Segment { data, paths: stored } => {
                    // Leaf columns come from the split columns; other paths (objects, unknown) from raw JSON.
                    let (leaf, derived): (Vec<String>, Vec<String>) = missing.iter().cloned().partition(|p| stored.contains(p));
                    let derived: Vec<String> = derived
                        .into_iter()
                        .filter(|p| {
                            let pre = format!("{p}.");
                            stored.range(pre.clone()..).next().is_some_and(|s| s.starts_with(&pre))
                        })
                        .collect();
                    let unknown: Vec<String> =
                        missing.iter().filter(|p| !stored.contains(*p) && !derived.contains(p)).cloned().collect();
                    let need_raw_now = need_raw || !derived.is_empty();
                    let d = segment::read(data, &leaf, need_raw_now)?;
                    let mut cols = l.cols.lock();
                    for (p, c) in d.cols {
                        cols.insert(p, Arc::new(c));
                    }
                    for p in unknown {
                        cols.insert(p, Arc::new(vec![Cell::Missing; l.ts.len()]));
                    }
                    if let Some(raw) = d.raw {
                        if !derived.is_empty() {
                            let parsed: Vec<Value> = raw.iter().map(|r| serde_json::from_str(r).unwrap_or(Value::Null)).collect();
                            for p in &derived {
                                cols.insert(p.clone(), Arc::new(parsed.iter().map(|e| table::cell_at(e, p)).collect()));
                            }
                        }
                        if want_raw {
                            *l.raw.lock() = Some(Arc::new(raw));
                        }
                    }
                }
            }
        }
        let cols = l.cols.lock();
        Ok(Batch {
            ts: (*l.ts).clone(),
            ids: (*l.ids).clone(),
            cols: paths.iter().filter_map(|p| cols.get(p).map(|c| (p.clone(), c.clone()))).collect(),
            raw: if want_raw { l.raw.lock().clone() } else { None },
        })
    }

    /// Returns batches for a collection overlapping [start, end) (ms), with the given columns.
    pub async fn scan(&self, project: &str, coll: &str, range: Option<(i64, i64)>, paths: &[String], want_raw: bool) -> Result<Vec<Batch>> {
        let ms = self.manifest(project).await?;
        let state = ms.m.collections.get(coll);
        let overlaps = |min: i64, max: i64| range.is_none_or(|(s, e)| max >= s && min < e);
        let mut objects: Vec<(String, bool)> = vec![];
        if let Some(st) = state {
            for s in &st.segments {
                if overlaps(s.min_ts, s.max_ts) {
                    objects.push((s.key.clone(), false));
                }
            }
        }
        if let Some(w) = self.wal.read().get(&(project.to_string(), coll.to_string())) {
            for (k, info) in w {
                let absorbed = state.is_some_and(|s| s.absorbed_wal.contains(k));
                if !absorbed && overlaps(info.min_ts, info.max_ts) {
                    objects.push((k.clone(), true));
                }
            }
        }
        let loaded: Vec<Arc<Loaded>> = futures::stream::iter(objects.iter().map(|(k, w)| self.load(k, *w)))
            .buffered(16)
            .try_collect()
            .await?;
        loaded.iter().map(|l| self.batch_from(l, paths, want_raw)).collect()
    }

    /// Collections with data, and their observed property types.
    pub async fn schema(&self, project: &str) -> Result<BTreeMap<String, BTreeMap<String, String>>> {
        let ms = self.manifest(project).await?;
        let mut out: BTreeMap<String, BTreeMap<String, String>> = BTreeMap::new();
        for (c, st) in &ms.m.collections {
            if !st.segments.is_empty() {
                out.insert(c.clone(), st.properties.clone());
            }
        }
        let wal: Vec<(String, Vec<String>)> = self
            .wal
            .read()
            .iter()
            .filter(|((p, _), _)| p == project)
            .map(|((_, c), m)| {
                let absorbed = ms.m.collections.get(c).map(|s| &s.absorbed_wal);
                (c.clone(), m.keys().filter(|k| absorbed.is_none_or(|a| !a.contains(*k))).cloned().collect())
            })
            .collect();
        for (c, keys) in wal {
            for k in keys {
                let l = self.load(&k, true).await?;
                if let Source::Wal(evs) = &l.src {
                    let props = out.entry(c.clone()).or_default();
                    let mut lv = vec![];
                    for e in evs.iter() {
                        lv.clear();
                        table::leaves(e, "", &mut lv);
                        for (p, t) in lv.drain(..) {
                            props.insert(p, t.to_string());
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    // ---- compaction and deletes ----

    /// Compacts every collection whose WAL is due (or all, when `force`).
    pub async fn compact_all(&self, force: bool) -> Result<usize> {
        let due: Vec<(String, String)> = {
            let idx = self.wal.read();
            let now = now_ms();
            idx.iter()
                .filter(|(_, m)| {
                    !m.is_empty()
                        && (force
                            || m.len() >= self.cfg.compaction.min_wal_objects
                            || m.values().any(|i| now.saturating_sub(i.created_ms) > self.cfg.compaction.max_wal_age_secs * 1000))
                })
                .map(|(k, _)| k.clone())
                .collect()
        };
        let mut n = 0;
        for (p, c) in due {
            let lock = self.project_lock(&p);
            let _g = lock.lock().await;
            match self.compact(&p, &c).await {
                Ok(true) => n += 1,
                Ok(false) => {}
                Err(e) => {
                    self.stats.compaction_failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                    tracing::error!(project = %p, collection = %c, error = %e, "compaction failed");
                }
            }
            if let Err(e) = self.merge_segments(&p, &c).await {
                tracing::error!(project = %p, collection = %c, error = %e, "segment merge failed");
            }
        }
        Ok(n)
    }

    /// Moves a collection's pending WAL objects into a new segment. Caller holds the project lock.
    async fn compact(&self, project: &str, coll: &str) -> Result<bool> {
        let ms = self.manifest(project).await?;
        let absorbed = ms.m.collections.get(coll).map(|s| s.absorbed_wal.clone()).unwrap_or_default();
        let keys: Vec<String> = self
            .wal
            .read()
            .get(&(project.to_string(), coll.to_string()))
            .map(|m| m.keys().filter(|k| !absorbed.contains(*k)).cloned().collect())
            .unwrap_or_default();
        if keys.is_empty() {
            self.gc_wal(project, coll).await?;
            return Ok(false);
        }
        let mut events: Vec<Value> = vec![];
        for k in &keys {
            if let Source::Wal(evs) = &self.load(k, true).await?.src {
                events.extend(evs.iter().cloned());
            }
        }
        let mut m = ms.m.clone();
        let st = m.collections.entry(coll.to_string()).or_default();
        if let Some(seg) = self.write_segment(project, coll, &events, &mut st.properties).await? {
            st.segments.push(seg);
        }
        st.absorbed_wal.extend(keys.iter().cloned());
        self.save_manifest(project, &ms, m).await?;
        self.stats.compactions.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        self.gc_wal(project, coll).await?;
        Ok(true)
    }

    /// Writes events (any order) as a segment; updates the property schema.
    async fn write_segment(&self, project: &str, coll: &str, events: &[Value], props: &mut BTreeMap<String, String>) -> Result<Option<SegmentMeta>> {
        if events.is_empty() {
            return Ok(None);
        }
        let mut rows: Vec<(i64, String, String, &Value)> = events
            .iter()
            .map(|e| {
                let ts = crate::event::event_ts(e);
                let id = table::lookup(e, "keen.id").and_then(Value::as_str).unwrap_or("").to_string();
                (ts, id, e.to_string(), e)
            })
            .collect();
        rows.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
        let mut lv = vec![];
        for (_, _, _, e) in &rows {
            lv.clear();
            table::leaves(e, "", &mut lv);
            for (p, t) in lv.drain(..) {
                props.insert(p, t.to_string());
            }
        }
        let seg_rows: Vec<segment::Row> = rows.iter().map(|(ts, id, raw, e)| segment::Row { ts: *ts, id, raw, event: e }).collect();
        let data = tokio::task::block_in_place(|| segment::write(&seg_rows))?;
        let key = self.key(&format!("segments/{project}/{}/{}.parquet", encode_name(coll), ulid::Ulid::generate()));
        self.store.put(&Path::from(key.as_str()), PutPayload::from(data.clone())).await?;
        self.disk.put_local(&key, &data)?;
        Ok(Some(SegmentMeta { key, rows: rows.len(), min_ts: rows[0].0, max_ts: rows[rows.len() - 1].0, bytes: data.len() }))
    }

    /// Deletes absorbed WAL objects, then forgets them in the manifest.
    async fn gc_wal(&self, project: &str, coll: &str) -> Result<()> {
        let ms = self.manifest(project).await?;
        let Some(st) = ms.m.collections.get(coll) else { return Ok(()) };
        if st.absorbed_wal.is_empty() {
            return Ok(());
        }
        let keys: Vec<String> = st.absorbed_wal.iter().cloned().collect();
        for k in &keys {
            match self.store.delete(&Path::from(k.as_str())).await {
                Ok(()) | Err(object_store::Error::NotFound { .. }) => {}
                Err(e) => return Err(e.into()),
            }
            self.disk.forget(k);
            self.mem.lock().pop(k);
        }
        if let Some(m) = self.wal.write().get_mut(&(project.to_string(), coll.to_string())) {
            for k in &keys {
                m.remove(k);
            }
        }
        let mut m = ms.m.clone();
        if let Some(st) = m.collections.get_mut(coll) {
            for k in &keys {
                st.absorbed_wal.remove(k);
            }
        }
        self.save_manifest(project, &ms, m).await
    }

    /// Merges small segments of a collection into one. Caller holds the project lock.
    async fn merge_segments(&self, project: &str, coll: &str) -> Result<()> {
        let ms = self.manifest(project).await?;
        let Some(st) = ms.m.collections.get(coll) else { return Ok(()) };
        let small: Vec<SegmentMeta> =
            st.segments.iter().filter(|s| s.rows < self.cfg.compaction.target_segment_rows / 4).cloned().collect();
        if small.len() <= self.cfg.compaction.merge_segments_over {
            return Ok(());
        }
        let events = self.segment_events(&small).await?;
        let mut m = ms.m.clone();
        let st = m.collections.get_mut(coll).unwrap();
        let old: BTreeSet<String> = small.iter().map(|s| s.key.clone()).collect();
        st.segments.retain(|s| !old.contains(&s.key));
        if let Some(seg) = self.write_segment(project, coll, &events, &mut st.properties).await? {
            st.segments.push(seg);
        }
        self.save_manifest(project, &ms, m).await?;
        self.delete_objects(old.into_iter().collect()).await;
        Ok(())
    }

    async fn segment_events(&self, segs: &[SegmentMeta]) -> Result<Vec<Value>> {
        let mut out = vec![];
        for s in segs {
            let l = self.load(&s.key, false).await?;
            let b = self.batch_from(&l, &[], true)?;
            for r in b.raw.as_ref().map(|r| r.iter()).into_iter().flatten() {
                out.push(serde_json::from_str(r)?);
            }
        }
        Ok(out)
    }

    async fn delete_objects(&self, keys: Vec<String>) {
        for k in keys {
            self.mem.lock().pop(&k);
            self.disk.forget(&k);
            if let Err(e) = self.store.delete(&Path::from(k.as_str())).await {
                tracing::warn!(key = %k, error = %e, "could not delete replaced object");
            }
        }
    }

    /// Deletes events of a collection matching `pred` (all events when `pred` is None).
    /// Returns the number of events deleted.
    pub async fn delete_events(&self, project: &str, coll: &str, pred: Option<&(dyn Fn(&Value) -> bool + Sync)>) -> Result<usize> {
        let lock = self.project_lock(project);
        let _g = lock.lock().await;
        self.compact(project, coll).await?;
        let ms = self.manifest(project).await?;
        let Some(st) = ms.m.collections.get(coll) else { return Ok(0) };
        let mut m = ms.m.clone();
        let old: Vec<String> = st.segments.iter().map(|s| s.key.clone()).collect();
        let deleted;
        match pred {
            None => {
                deleted = st.segments.iter().map(|s| s.rows).sum();
                m.collections.remove(coll);
            }
            Some(pred) => {
                let events = self.segment_events(&st.segments).await?;
                let before = events.len();
                let kept: Vec<Value> = events.into_iter().filter(|e| !pred(e)).collect();
                deleted = before - kept.len();
                if deleted == 0 {
                    return Ok(0);
                }
                let st = m.collections.get_mut(coll).unwrap();
                st.segments.clear();
                st.properties.clear();
                if let Some(seg) = self.write_segment(project, coll, &kept, &mut st.properties).await? {
                    st.segments.push(seg);
                }
            }
        }
        self.save_manifest(project, &ms, m).await?;
        self.delete_objects(old).await;
        Ok(deleted)
    }

    /// Total WAL objects not yet compacted, for metrics.
    pub fn pending_wal(&self) -> usize {
        self.wal.read().values().map(|m| m.len()).sum()
    }

    /// Checks that object storage answers.
    pub async fn health(&self) -> Result<()> {
        let key = self.key("health/probe");
        self.store.put(&Path::from(key.as_str()), PutPayload::from(Bytes::from_static(b"ok"))).await?;
        Ok(())
    }

    /// Writes an arbitrary object (metadata backups).
    pub async fn put_object(&self, rest: &str, data: Bytes) -> Result<()> {
        self.store.put(&Path::from(self.key(rest).as_str()), PutPayload::from(data)).await?;
        Ok(())
    }

    pub async fn get_object(&self, rest: &str) -> Result<Option<Bytes>> {
        match self.store.get(&Path::from(self.key(rest).as_str())).await {
            Ok(r) => Ok(Some(r.bytes().await?)),
            Err(object_store::Error::NotFound { .. }) => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    /// Lists object keys under a prefix (relative to the storage prefix).
    pub async fn list_objects(&self, rest: &str) -> Result<Vec<String>> {
        let prefix = Path::from(self.key(rest).as_str());
        let metas: Vec<_> = self.store.list(Some(&prefix)).try_collect().await?;
        Ok(metas.into_iter().map(|m| m.location.to_string()).collect())
    }
}

fn loaded_wal(evs: Vec<Value>) -> Loaded {
    let ts = evs.iter().map(crate::event::event_ts).collect();
    let ids = evs.iter().map(|e| Arc::from(table::lookup(e, "keen.id").and_then(Value::as_str).unwrap_or(""))).collect();
    Loaded { ts: Arc::new(ts), ids: Arc::new(ids), cols: Mutex::new(HashMap::new()), raw: Mutex::new(None), src: Source::Wal(Arc::new(evs)) }
}

/// Group commit: gathers concurrent writes for up to `group_commit_ms`, writes one WAL object
/// per (project, collection), and acknowledges each request when all of its objects are stored.
async fn writer_loop(engine: Arc<Engine>, mut rx: mpsc::Receiver<WriteReq>) {
    let window = std::time::Duration::from_millis(engine.cfg.wal.group_commit_ms);
    while let Some(first) = rx.recv().await {
        let mut reqs = vec![first];
        let deadline = tokio::time::Instant::now() + window;
        let mut n: usize = reqs[0].events.len();
        while n < engine.cfg.wal.max_batch_events {
            match tokio::time::timeout_at(deadline, rx.recv()).await {
                Ok(Some(r)) => {
                    n += r.events.len();
                    reqs.push(r);
                }
                _ => break,
            }
        }
        let mut groups: BTreeMap<(String, String), Vec<StoredEvent>> = BTreeMap::new();
        let mut req_groups: Vec<BTreeSet<(String, String)>> = vec![];
        for r in &reqs {
            let mut gs = BTreeSet::new();
            for e in &r.events {
                let k = (r.project.clone(), e.collection.clone());
                groups.entry(k.clone()).or_default().push(e.clone());
                gs.insert(k);
            }
            req_groups.push(gs);
        }
        let results: Vec<((String, String), Result<()>)> = futures::stream::iter(groups.into_iter().map(|(k, evs)| {
            let e = engine.clone();
            async move {
                let r = e.commit(&k.0, &k.1, &evs).await;
                (k, r)
            }
        }))
        .buffer_unordered(32)
        .collect()
        .await;
        let failed: HashMap<(String, String), String> =
            results.into_iter().filter_map(|(k, r)| r.err().map(|e| (k, format!("{e:#}")))).collect();
        if failed.is_empty() {
            engine.stats.wal_commits.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        } else {
            engine.stats.wal_commit_failures.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        for (r, gs) in reqs.into_iter().zip(req_groups) {
            let err = gs.iter().find_map(|g| failed.get(g).cloned());
            let _ = r.done.send(match err {
                None => Ok(()),
                Some(e) => Err(e),
            });
        }
    }
}

/// Runs compaction periodically until the process exits.
pub async fn compaction_loop(engine: Arc<Engine>) {
    if !engine.cfg.compaction.enabled {
        return;
    }
    let mut t = tokio::time::interval(std::time::Duration::from_secs(engine.cfg.compaction.interval_secs.max(1)));
    loop {
        t.tick().await;
        if let Err(e) = engine.compact_all(false).await {
            tracing::error!(error = %e, "compaction pass failed");
        }
    }
}

