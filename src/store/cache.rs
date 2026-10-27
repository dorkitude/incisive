//! Local disk (NVMe) cache of immutable storage objects, with least-recently-used eviction.

use anyhow::Result;
use bytes::Bytes;
use object_store::{path::Path, ObjectStore, ObjectStoreExt};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

pub struct DiskCache {
    dir: PathBuf,
    max_bytes: u64,
    store: Arc<dyn ObjectStore>,
    state: Mutex<State>,
    pub hits: AtomicU64,
    pub misses: AtomicU64,
}

#[derive(Default)]
struct State {
    entries: HashMap<String, (u64, u64)>, // key -> (bytes, last use tick)
    total: u64,
    tick: u64,
}

impl DiskCache {
    pub fn new(dir: PathBuf, max_bytes: u64, store: Arc<dyn ObjectStore>) -> Result<DiskCache> {
        std::fs::create_dir_all(&dir)?;
        let c = DiskCache { dir, max_bytes, store, state: Mutex::new(State::default()), hits: 0.into(), misses: 0.into() };
        c.scan()?;
        Ok(c)
    }

    fn scan(&self) -> Result<()> {
        let mut st = self.state.lock();
        let mut stack = vec![self.dir.clone()];
        while let Some(d) = stack.pop() {
            for e in std::fs::read_dir(&d)? {
                let e = e?;
                let md = e.metadata()?;
                if md.is_dir() {
                    stack.push(e.path());
                } else if let Ok(rel) = e.path().strip_prefix(&self.dir) {
                    let key = rel.to_string_lossy().to_string();
                    if key.ends_with(".tmp") {
                        let _ = std::fs::remove_file(e.path());
                        continue;
                    }
                    st.total += md.len();
                    st.entries.insert(key, (md.len(), 0));
                }
            }
        }
        Ok(())
    }

    fn local(&self, key: &str) -> PathBuf {
        self.dir.join(key)
    }

    /// Stores bytes we just wrote to object storage.
    pub fn put_local(&self, key: &str, data: &Bytes) -> Result<()> {
        let p = self.local(key);
        if let Some(parent) = p.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp = p.with_extension("tmp");
        std::fs::write(&tmp, data)?;
        std::fs::rename(&tmp, &p)?;
        self.record(key, data.len() as u64);
        Ok(())
    }

    fn record(&self, key: &str, size: u64) {
        let evict: Vec<String> = {
            let mut st = self.state.lock();
            st.tick += 1;
            let tick = st.tick;
            if let Some(old) = st.entries.insert(key.to_string(), (size, tick)) {
                st.total -= old.0;
            }
            st.total += size;
            let mut victims = vec![];
            if st.total > self.max_bytes {
                let mut all: Vec<(String, u64, u64)> = st.entries.iter().map(|(k, (s, t))| (k.clone(), *s, *t)).collect();
                all.sort_by_key(|x| x.2);
                let mut total = st.total;
                for (k, s, _) in all {
                    if total <= self.max_bytes * 9 / 10 || k == key {
                        continue;
                    }
                    total -= s;
                    victims.push(k);
                }
                for k in &victims {
                    if let Some((s, _)) = st.entries.remove(k) {
                        st.total -= s;
                    }
                }
            }
            victims
        };
        for k in evict {
            let _ = std::fs::remove_file(self.local(&k));
        }
    }

    /// Returns an object, from local disk when cached, else from object storage (then cached).
    pub async fn get(&self, key: &str) -> Result<Bytes> {
        let p = self.local(key);
        if let Ok(data) = tokio::fs::read(&p).await {
            self.hits.fetch_add(1, Ordering::Relaxed);
            let mut st = self.state.lock();
            st.tick += 1;
            let tick = st.tick;
            if let Some(e) = st.entries.get_mut(key) {
                e.1 = tick;
            }
            return Ok(Bytes::from(data));
        }
        self.misses.fetch_add(1, Ordering::Relaxed);
        let data = self.store.get(&Path::from(key)).await?.bytes().await?;
        self.put_local(key, &data)?;
        Ok(data)
    }

    pub fn forget(&self, key: &str) {
        let removed = self.state.lock().entries.remove(key);
        if let Some((s, _)) = removed {
            self.state.lock().total -= s;
        }
        let _ = std::fs::remove_file(self.local(key));
    }

    pub fn bytes_used(&self) -> u64 {
        self.state.lock().total
    }
}
