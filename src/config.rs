//! Runtime configuration: defaults < `incisive.toml` < environment (`INCISIVE_SECTION__KEY`).

use figment::providers::{Env, Format, Serialized, Toml};
use figment::Figment;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct Config {
    /// Comma-separated exact email domains allowed for new accounts; empty allows all.
    pub signup_allowed_email_domains: String,
    pub server: Server,
    pub storage: Storage,
    pub cache: Cache,
    pub wal: Wal,
    pub compaction: Compaction,
    pub meta: Meta,
    pub email: Email,
    pub limits: Limits,
    pub geo: Geo,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Server {
    pub addr: String,
    /// Public base URL of the web app and API (used in emails and snippets).
    pub public_url: String,
    /// Take client IPs from X-Forwarded-For (enable only behind a trusted proxy).
    pub trust_forwarded_for: bool,
    /// Hosts that GET-tracking `redirect=` may send browsers to. Empty disables redirects.
    pub redirect_allow_hosts: Vec<String>,
    pub shutdown_timeout_secs: u64,
    /// Whether session cookies carry the Secure attribute.
    pub secure_cookies: bool,
    /// Where install snippets load the web tracker from, e.g. a public CDN bucket. Empty means
    /// this server's own `/js/incisive.js`.
    pub tracker_url: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Storage {
    /// "s3" (R2, S3, MinIO) or "fs" (local directory, for development and tests).
    pub backend: String,
    pub bucket: String,
    pub endpoint: String,
    pub region: String,
    /// AWS shared-credentials profile to read when no credentials are in the environment.
    pub profile: String,
    /// Key prefix inside the bucket, e.g. "dev/".
    pub prefix: String,
    pub fs_path: PathBuf,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Cache {
    /// Local (NVMe) directory holding copies of storage objects.
    pub dir: PathBuf,
    pub max_disk_bytes: u64,
    /// Budget for decoded columns held in memory.
    pub max_memory_bytes: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Wal {
    /// Longest a write waits for others to share its commit.
    pub group_commit_ms: u64,
    pub max_batch_events: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Compaction {
    pub enabled: bool,
    pub interval_secs: u64,
    /// Compact a collection's WAL once it has this many objects...
    pub min_wal_objects: usize,
    /// ...or its oldest object is this old.
    pub max_wal_age_secs: u64,
    /// Merge small segments of a collection once there are this many.
    pub merge_segments_over: usize,
    pub target_segment_rows: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Meta {
    pub path: PathBuf,
    /// Snapshot the metadata database to storage this often (0 disables).
    pub backup_interval_secs: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Email {
    /// Resend-compatible API base URL.
    pub resend_url: String,
    pub api_key: String,
    pub from: String,
    /// Log magic links instead of sending them (development).
    pub log_only: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Limits {
    pub max_event_bytes: usize,
    pub max_request_bytes: usize,
    pub max_batch_events: usize,
    pub write_events_per_second: f64,
    pub write_burst: f64,
    pub queries_per_second: f64,
    pub query_burst: f64,
    pub max_concurrent_queries: usize,
    pub query_timeout_secs: u64,
    pub max_extraction_events: usize,
    pub max_funnel_actors: usize,
    pub max_groups: usize,
    pub max_intervals: usize,
    pub max_scan_events: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct Geo {
    /// MaxMind-format city database (.mmdb). Without it, geo enrichment is skipped.
    pub db_path: PathBuf,
}

impl Geo {
    /// Whether a location database is configured.
    pub fn enabled(&self) -> bool {
        !self.db_path.as_os_str().is_empty()
    }
}


impl Default for Server {
    fn default() -> Self {
        Server {
            addr: "127.0.0.1:8080".into(),
            public_url: "http://localhost:8080".into(),
            trust_forwarded_for: false,
            redirect_allow_hosts: vec![],
            shutdown_timeout_secs: 30,
            secure_cookies: false,
            tracker_url: String::new(),
        }
    }
}

impl Default for Storage {
    fn default() -> Self {
        Storage {
            backend: "fs".into(),
            bucket: String::new(),
            endpoint: String::new(),
            region: "auto".into(),
            profile: String::new(),
            prefix: String::new(),
            fs_path: "./data/objects".into(),
        }
    }
}

impl Default for Cache {
    fn default() -> Self {
        Cache { dir: "./data/cache".into(), max_disk_bytes: 50 << 30, max_memory_bytes: 2 << 30 }
    }
}

impl Default for Wal {
    fn default() -> Self {
        Wal { group_commit_ms: 25, max_batch_events: 50_000 }
    }
}

impl Default for Compaction {
    fn default() -> Self {
        Compaction {
            enabled: true,
            interval_secs: 15,
            min_wal_objects: 32,
            max_wal_age_secs: 120,
            merge_segments_over: 8,
            target_segment_rows: 2_000_000,
        }
    }
}

impl Default for Meta {
    fn default() -> Self {
        Meta { path: "./data/meta.sqlite".into(), backup_interval_secs: 300 }
    }
}

impl Default for Email {
    fn default() -> Self {
        Email {
            resend_url: "https://api.resend.com".into(),
            api_key: String::new(),
            from: "Incisive <login@example.com>".into(),
            log_only: true,
        }
    }
}

impl Default for Limits {
    fn default() -> Self {
        Limits {
            max_event_bytes: 1 << 20,
            max_request_bytes: 10 << 20,
            max_batch_events: 5000,
            write_events_per_second: 2000.0,
            write_burst: 10000.0,
            queries_per_second: 20.0,
            query_burst: 100.0,
            max_concurrent_queries: 16,
            query_timeout_secs: 30,
            max_extraction_events: 100_000,
            max_funnel_actors: 1_000_000,
            max_groups: 10_000,
            max_intervals: 5_000,
            max_scan_events: 200_000_000,
        }
    }
}

impl Default for Geo {
    fn default() -> Self {
        Geo { db_path: PathBuf::new() }
    }
}

impl Config {
    /// Loads configuration from defaults, an optional TOML file, and the environment.
    pub fn load(file: Option<&std::path::Path>) -> anyhow::Result<Config> {
        let mut fig = Figment::from(Serialized::defaults(Config::default()));
        match file {
            Some(f) => fig = fig.merge(Toml::file_exact(f)),
            None => fig = fig.merge(Toml::file("incisive.toml")),
        }
        fig = fig.merge(Env::prefixed("INCISIVE_").split("__"));
        Ok(fig.extract()?)
    }
}
