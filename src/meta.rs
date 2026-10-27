//! Metadata in SQLite: users, login links, sessions, projects, keys, and saved queries.
//! The database is backed up to object storage periodically and restored from there when the
//! local file is missing.

use crate::auth::{self, Access, KeyOptions};
use anyhow::{anyhow, Context, Result};
use chrono::Utc;
use parking_lot::Mutex;
use rusqlite::{params, Connection, OptionalExtension};
use serde::Serialize;
use serde_json::Value;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub const LOGIN_TOKEN_TTL_SECS: i64 = 15 * 60;
pub const MAX_CODE_ATTEMPTS: i64 = 5;
pub const SESSION_TTL_SECS: i64 = 400 * 24 * 3600;

const SCHEMA: &str = r#"
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    email TEXT NOT NULL UNIQUE,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS login_tokens (
    token_hash TEXT PRIMARY KEY,
    email TEXT NOT NULL,
    expires_at INTEGER NOT NULL,
    used_at INTEGER
);
CREATE TABLE IF NOT EXISTS sessions (
    token_hash TEXT PRIMARY KEY,
    user_id TEXT NOT NULL REFERENCES users(id),
    created_at INTEGER NOT NULL,
    expires_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS projects (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE IF NOT EXISTS memberships (
    project_id TEXT NOT NULL REFERENCES projects(id),
    user_id TEXT NOT NULL REFERENCES users(id),
    role TEXT NOT NULL,
    PRIMARY KEY (project_id, user_id)
);
CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL REFERENCES projects(id),
    name TEXT NOT NULL,
    key_hash TEXT NOT NULL UNIQUE,
    hint TEXT NOT NULL,
    master INTEGER NOT NULL,
    secret TEXT,
    permitted TEXT NOT NULL,
    options TEXT NOT NULL,
    is_active INTEGER NOT NULL,
    created_at INTEGER NOT NULL,
    last_used_at INTEGER
);
CREATE INDEX IF NOT EXISTS api_keys_project ON api_keys(project_id);
CREATE TABLE IF NOT EXISTS saved_queries (
    project_id TEXT NOT NULL REFERENCES projects(id),
    name TEXT NOT NULL,
    body TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (project_id, name)
);
CREATE TABLE IF NOT EXISTS ignored_ips (
    user_id TEXT NOT NULL REFERENCES users(id),
    ip TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    PRIMARY KEY (user_id, ip)
);
CREATE INDEX IF NOT EXISTS ignored_ips_ip ON ignored_ips(ip);
"#;

/// How long an address stays ignored after its user last loaded the app from it.
pub const IGNORE_IP_TTL_SECS: i64 = 30 * 24 * 3600;

#[derive(Clone, Debug, Serialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub created_at: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct Project {
    pub id: String,
    pub name: String,
    pub created_at: i64,
}

#[derive(Clone, Debug)]
pub struct ApiKey {
    pub id: String,
    pub project_id: String,
    pub name: String,
    /// The start of the key, for display.
    pub hint: String,
    pub master: bool,
    /// The full key, kept for master keys (they decrypt legacy scoped keys) and for write-only
    /// keys (they are public by nature: they are embedded in web pages and pixels).
    pub secret: Option<String>,
    pub permitted: Vec<String>,
    pub options: KeyOptions,
    pub is_active: bool,
    pub created_at: i64,
    pub last_used_at: Option<i64>,
}

impl ApiKey {
    fn access(&self) -> Access {
        if self.master {
            return Access::master(&self.project_id, Some(self.id.clone()));
        }
        Access {
            project_id: self.project_id.clone(),
            key_id: Some(self.id.clone()),
            master: false,
            permitted: self.permitted.clone(),
            options: self.options.clone(),
            scoped_ops: None,
            scoped_filters: vec![],
        }
    }
}

#[derive(Clone, Debug, Serialize)]
pub struct SavedQuery {
    pub name: String,
    pub body: Value,
    pub created_at: i64,
    pub updated_at: i64,
}

pub struct Meta {
    conn: Mutex<Connection>,
    path: PathBuf,
    /// Key hash -> key, for fast authentication of tracking requests.
    keys: Mutex<HashMap<String, Option<ApiKey>>>,
    /// Project id -> master secrets, for scoped key decryption.
    masters: Mutex<HashMap<String, Vec<String>>>,
}

pub fn is_write_only(permitted: &[String]) -> bool {
    permitted.len() == 1 && permitted[0] == "writes"
}

fn now() -> i64 {
    Utc::now().timestamp()
}

fn new_id() -> String {
    hex::encode(rand::random::<[u8; 12]>())
}

fn key_from_row(r: &rusqlite::Row) -> rusqlite::Result<ApiKey> {
    let permitted: String = r.get("permitted")?;
    let options: String = r.get("options")?;
    Ok(ApiKey {
        id: r.get("id")?,
        project_id: r.get("project_id")?,
        name: r.get("name")?,
        hint: r.get("hint")?,
        master: r.get::<_, i64>("master")? != 0,
        secret: r.get("secret")?,
        permitted: serde_json::from_str(&permitted).unwrap_or_default(),
        options: serde_json::from_str(&options).unwrap_or_default(),
        is_active: r.get::<_, i64>("is_active")? != 0,
        created_at: r.get("created_at")?,
        last_used_at: r.get("last_used_at")?,
    })
}

impl Meta {
    pub fn open(path: &Path) -> Result<Arc<Meta>> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let conn = Connection::open(path).with_context(|| format!("opening {}", path.display()))?;
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON; PRAGMA busy_timeout=5000;")?;
        conn.execute_batch(SCHEMA)?;
        // Columns added after the first release.
        for col in ["code_hash TEXT", "attempts INTEGER NOT NULL DEFAULT 0"] {
            let _ = conn.execute(&format!("ALTER TABLE login_tokens ADD COLUMN {col}"), []);
        }
        Ok(Arc::new(Meta { conn: Mutex::new(conn), path: path.to_path_buf(), keys: Mutex::default(), masters: Mutex::default() }))
    }

    fn invalidate(&self) {
        self.keys.lock().clear();
        self.masters.lock().clear();
    }

    // ---- users and login ----

    pub fn user_by_email(&self, email: &str) -> Result<Option<User>> {
        let c = self.conn.lock();
        Ok(c.query_row("SELECT id, email, created_at FROM users WHERE email = ?1", [email], |r| Ok(User { id: r.get(0)?, email: r.get(1)?, created_at: r.get(2)? }))
            .optional()?)
    }

    pub fn user(&self, id: &str) -> Result<Option<User>> {
        let c = self.conn.lock();
        Ok(c.query_row("SELECT id, email, created_at FROM users WHERE id = ?1", [id], |r| Ok(User { id: r.get(0)?, email: r.get(1)?, created_at: r.get(2)? }))
            .optional()?)
    }

    /// Returns the user with this email, creating it if needed. The bool is true when created.
    pub fn ensure_user(&self, email: &str) -> Result<(User, bool)> {
        if let Some(u) = self.user_by_email(email)? {
            return Ok((u, false));
        }
        let u = User { id: new_id(), email: email.to_string(), created_at: now() };
        self.conn.lock().execute("INSERT INTO users (id, email, created_at) VALUES (?1, ?2, ?3)", params![u.id, u.email, u.created_at])?;
        Ok((u, true))
    }

    /// Creates a 6-digit sign-in code for an email address, replacing any earlier unused code.
    pub fn create_login_code(&self, email: &str) -> Result<String> {
        let code = format!("{:06}", rand::random_range(0..1_000_000u32));
        let c = self.conn.lock();
        c.execute("DELETE FROM login_tokens WHERE expires_at < ?1 OR (email = ?2 AND used_at IS NULL)", params![now() - 86400, email])?;
        // The row key is random; the code is checked against `code_hash`.
        c.execute(
            "INSERT INTO login_tokens (token_hash, email, expires_at, code_hash) VALUES (?1, ?2, ?3, ?4)",
            params![auth::random_token(), email, now() + LOGIN_TOKEN_TTL_SECS, auth::sha256_hex(&format!("{email}:{code}"))],
        )?;
        Ok(code)
    }

    /// Checks a sign-in code. A code works once, within 15 minutes, and is burned after
    /// `MAX_CODE_ATTEMPTS` wrong guesses.
    pub fn consume_login_code(&self, email: &str, code: &str) -> Result<bool> {
        let c = self.conn.lock();
        let row: Option<(String, String, i64)> = c
            .query_row(
                "SELECT token_hash, code_hash, attempts FROM login_tokens WHERE email = ?1 AND used_at IS NULL AND expires_at >= ?2 AND code_hash IS NOT NULL",
                params![email, now()],
                |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
            )
            .optional()?;
        let Some((key, want, attempts)) = row else { return Ok(false) };
        if attempts >= MAX_CODE_ATTEMPTS {
            return Ok(false);
        }
        if auth::sha256_hex(&format!("{email}:{}", code.trim())) == want {
            c.execute("UPDATE login_tokens SET used_at = ?1 WHERE token_hash = ?2", params![now(), key])?;
            return Ok(true);
        }
        c.execute("UPDATE login_tokens SET attempts = attempts + 1 WHERE token_hash = ?1", [key])?;
        Ok(false)
    }

    pub fn create_session(&self, user_id: &str) -> Result<String> {
        let token = auth::random_token();
        let c = self.conn.lock();
        c.execute("DELETE FROM sessions WHERE expires_at < ?1", [now()])?;
        c.execute(
            "INSERT INTO sessions (token_hash, user_id, created_at, expires_at) VALUES (?1, ?2, ?3, ?4)",
            params![auth::sha256_hex(&token), user_id, now(), now() + SESSION_TTL_SECS],
        )?;
        Ok(token)
    }

    pub fn session_user(&self, token: &str) -> Result<Option<User>> {
        let c = self.conn.lock();
        Ok(c.query_row(
            "SELECT u.id, u.email, u.created_at FROM sessions s JOIN users u ON u.id = s.user_id WHERE s.token_hash = ?1 AND s.expires_at > ?2",
            params![auth::sha256_hex(token), now()],
            |r| Ok(User { id: r.get(0)?, email: r.get(1)?, created_at: r.get(2)? }),
        )
        .optional()?)
    }

    pub fn delete_session(&self, token: &str) -> Result<()> {
        self.conn.lock().execute("DELETE FROM sessions WHERE token_hash = ?1", [auth::sha256_hex(token)])?;
        Ok(())
    }

    // ---- projects ----

    /// Creates a project owned by the user, with a master, read and write key. Returns the
    /// project and the new keys (the only time their secrets are available).
    pub fn create_project(&self, user_id: Option<&str>, name: &str) -> Result<(Project, Vec<(ApiKey, String)>)> {
        let p = Project { id: new_id(), name: name.to_string(), created_at: now() };
        {
            let c = self.conn.lock();
            c.execute("INSERT INTO projects (id, name, created_at) VALUES (?1, ?2, ?3)", params![p.id, p.name, p.created_at])?;
            if let Some(u) = user_id {
                c.execute("INSERT INTO memberships (project_id, user_id, role) VALUES (?1, ?2, 'owner')", params![p.id, u])?;
            }
        }
        let keys = vec![
            self.create_key(&p.id, "Master key", true, vec![], KeyOptions::default())?,
            self.create_key(&p.id, "Read key", false, vec!["queries".into(), "saved_queries".into(), "cached_queries".into(), "schema".into()], KeyOptions::default())?,
            self.create_key(&p.id, "Write key", false, vec!["writes".into()], KeyOptions::default())?,
        ];
        Ok((p, keys))
    }

    pub fn project(&self, id: &str) -> Result<Option<Project>> {
        let c = self.conn.lock();
        Ok(c.query_row("SELECT id, name, created_at FROM projects WHERE id = ?1", [id], |r| Ok(Project { id: r.get(0)?, name: r.get(1)?, created_at: r.get(2)? }))
            .optional()?)
    }

    pub fn projects_for(&self, user_id: &str) -> Result<Vec<Project>> {
        let c = self.conn.lock();
        let mut st = c.prepare("SELECT p.id, p.name, p.created_at FROM projects p JOIN memberships m ON m.project_id = p.id WHERE m.user_id = ?1 ORDER BY p.created_at")?;
        let rows = st.query_map([user_id], |r| Ok(Project { id: r.get(0)?, name: r.get(1)?, created_at: r.get(2)? }))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Ignores events from `ip` in the user's projects, or extends an existing ignore.
    pub fn ignore_ip(&self, user_id: &str, ip: &str) -> Result<()> {
        let c = self.conn.lock();
        c.execute(
            "INSERT INTO ignored_ips (user_id, ip, updated_at) VALUES (?1, ?2, ?3) ON CONFLICT (user_id, ip) DO UPDATE SET updated_at = ?3",
            params![user_id, ip, now()],
        )?;
        Ok(())
    }

    pub fn unignore_ip(&self, user_id: &str, ip: &str) -> Result<()> {
        let c = self.conn.lock();
        c.execute("DELETE FROM ignored_ips WHERE user_id = ?1 AND ip = ?2", params![user_id, ip])?;
        Ok(())
    }

    /// The user's ignored addresses, most recently seen first, with when each was last seen.
    pub fn ignored_ips(&self, user_id: &str) -> Result<Vec<(String, i64)>> {
        let c = self.conn.lock();
        let mut st = c.prepare("SELECT ip, updated_at FROM ignored_ips WHERE user_id = ?1 AND updated_at > ?2 ORDER BY updated_at DESC")?;
        let rows = st.query_map(params![user_id, now() - IGNORE_IP_TTL_SECS], |r| Ok((r.get(0)?, r.get(1)?)))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Whether a member of the project has asked to ignore events from `ip`.
    pub fn is_ignored(&self, project_id: &str, ip: &str) -> Result<bool> {
        let c = self.conn.lock();
        Ok(c
            .query_row(
                "SELECT 1 FROM ignored_ips i JOIN memberships m ON m.user_id = i.user_id WHERE i.ip = ?1 AND m.project_id = ?2 AND i.updated_at > ?3 LIMIT 1",
                params![ip, project_id, now() - IGNORE_IP_TTL_SECS],
                |_| Ok(()),
            )
            .optional()?
            .is_some())
    }

    pub fn is_member(&self, project_id: &str, user_id: &str) -> Result<bool> {
        let c = self.conn.lock();
        Ok(c.query_row("SELECT 1 FROM memberships WHERE project_id = ?1 AND user_id = ?2", params![project_id, user_id], |_| Ok(())).optional()?.is_some())
    }

    pub fn rename_project(&self, id: &str, name: &str) -> Result<()> {
        self.conn.lock().execute("UPDATE projects SET name = ?1 WHERE id = ?2", params![name, id])?;
        Ok(())
    }

    // ---- keys ----

    /// Creates a key and returns it with its secret.
    pub fn create_key(&self, project_id: &str, name: &str, master: bool, permitted: Vec<String>, options: KeyOptions) -> Result<(ApiKey, String)> {
        let secret = auth::generate_key();
        let k = ApiKey {
            id: new_id(),
            project_id: project_id.to_string(),
            name: name.to_string(),
            hint: format!("{}…", &secret[..auth::KEY_PREFIX.len() + 4]),
            master,
            secret: (master || is_write_only(&permitted)).then(|| secret.clone()),
            permitted,
            options,
            is_active: true,
            created_at: now(),
            last_used_at: None,
        };
        self.conn.lock().execute(
            "INSERT INTO api_keys (id, project_id, name, key_hash, hint, master, secret, permitted, options, is_active, created_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, 1, ?10)",
            params![
                k.id,
                k.project_id,
                k.name,
                auth::sha256_hex(&secret),
                k.hint,
                master as i64,
                k.secret,
                serde_json::to_string(&k.permitted)?,
                serde_json::to_string(&k.options)?,
                k.created_at
            ],
        )?;
        self.invalidate();
        Ok((k, secret))
    }

    pub fn keys(&self, project_id: &str) -> Result<Vec<ApiKey>> {
        let c = self.conn.lock();
        let mut st = c.prepare("SELECT * FROM api_keys WHERE project_id = ?1 ORDER BY created_at, rowid")?;
        let rows = st.query_map([project_id], key_from_row)?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn key(&self, project_id: &str, id: &str) -> Result<Option<ApiKey>> {
        let c = self.conn.lock();
        Ok(c.query_row("SELECT * FROM api_keys WHERE project_id = ?1 AND id = ?2", params![project_id, id], key_from_row).optional()?)
    }

    /// Finds a key of the project by its secret.
    pub fn key_by_secret(&self, project_id: &str, secret: &str) -> Result<Option<ApiKey>> {
        Ok(self.lookup_hash(&auth::sha256_hex(secret))?.filter(|k| k.project_id == project_id))
    }

    fn lookup_hash(&self, hash: &str) -> Result<Option<ApiKey>> {
        if let Some(k) = self.keys.lock().get(hash) {
            return Ok(k.clone());
        }
        let k = {
            let c = self.conn.lock();
            c.query_row("SELECT * FROM api_keys WHERE key_hash = ?1", [hash], key_from_row).optional()?
        };
        let mut cache = self.keys.lock();
        if cache.len() > 10_000 {
            cache.clear();
        }
        cache.insert(hash.to_string(), k.clone());
        Ok(k)
    }

    pub fn update_key(&self, k: &ApiKey) -> Result<()> {
        // A key that gains read permissions stops keeping its secret.
        let keep = k.master || is_write_only(&k.permitted);
        self.conn.lock().execute(
            "UPDATE api_keys SET name = ?1, permitted = ?2, options = ?3, is_active = ?4, secret = CASE WHEN ?7 THEN secret ELSE NULL END WHERE id = ?5 AND project_id = ?6",
            params![k.name, serde_json::to_string(&k.permitted)?, serde_json::to_string(&k.options)?, k.is_active as i64, k.id, k.project_id, keep],
        )?;
        self.invalidate();
        Ok(())
    }

    pub fn delete_key(&self, project_id: &str, id: &str) -> Result<bool> {
        let n = self.conn.lock().execute("DELETE FROM api_keys WHERE project_id = ?1 AND id = ?2", params![project_id, id])?;
        self.invalidate();
        Ok(n > 0)
    }

    /// Resolves the credential of an API request against the project in its URL: a project key,
    /// or a legacy scoped key encrypted with one of the project's master keys.
    pub fn authenticate(&self, project_id: &str, credential: &str) -> Result<Option<Access>> {
        if credential.starts_with(auth::KEY_PREFIX) {
            return Ok(self.key_by_secret(project_id, credential)?.filter(|k| k.is_active).map(|k| k.access()));
        }
        if credential.len() >= 64 && credential.bytes().all(|b| b.is_ascii_hexdigit()) {
            for m in self.master_secrets(project_id)? {
                if let Some(opts) = auth::decrypt_scoped(&m, credential) {
                    return Ok(auth::scoped_access(project_id, &opts).ok());
                }
            }
        }
        Ok(None)
    }

    fn master_secrets(&self, project_id: &str) -> Result<Vec<String>> {
        if let Some(m) = self.masters.lock().get(project_id) {
            return Ok(m.clone());
        }
        let v: Vec<String> = {
            let c = self.conn.lock();
            let mut st = c.prepare("SELECT secret FROM api_keys WHERE project_id = ?1 AND master = 1 AND is_active = 1 AND secret IS NOT NULL")?;
            let rows = st.query_map([project_id], |r| r.get(0))?;
            rows.collect::<rusqlite::Result<_>>()?
        };
        self.masters.lock().insert(project_id.to_string(), v.clone());
        Ok(v)
    }

    pub fn touch_key(&self, id: &str) {
        let _ = self.conn.lock().execute("UPDATE api_keys SET last_used_at = ?1 WHERE id = ?2 AND (last_used_at IS NULL OR last_used_at < ?1 - 60)", params![now(), id]);
    }

    // ---- saved queries ----

    pub fn saved_queries(&self, project_id: &str) -> Result<Vec<SavedQuery>> {
        let c = self.conn.lock();
        let mut st = c.prepare("SELECT name, body, created_at, updated_at FROM saved_queries WHERE project_id = ?1 ORDER BY name")?;
        let rows = st.query_map([project_id], |r| {
            let body: String = r.get(1)?;
            Ok(SavedQuery { name: r.get(0)?, body: serde_json::from_str(&body).unwrap_or(Value::Null), created_at: r.get(2)?, updated_at: r.get(3)? })
        })?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    pub fn saved_query(&self, project_id: &str, name: &str) -> Result<Option<SavedQuery>> {
        let c = self.conn.lock();
        Ok(c.query_row("SELECT name, body, created_at, updated_at FROM saved_queries WHERE project_id = ?1 AND name = ?2", params![project_id, name], |r| {
            let body: String = r.get(1)?;
            Ok(SavedQuery { name: r.get(0)?, body: serde_json::from_str(&body).unwrap_or(Value::Null), created_at: r.get(2)?, updated_at: r.get(3)? })
        })
        .optional()?)
    }

    /// Creates or replaces a saved query. Returns true when it was created.
    pub fn put_saved_query(&self, project_id: &str, name: &str, body: &Value) -> Result<bool> {
        let existed = self.saved_query(project_id, name)?.is_some();
        self.conn.lock().execute(
            "INSERT INTO saved_queries (project_id, name, body, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?4)
             ON CONFLICT (project_id, name) DO UPDATE SET body = excluded.body, updated_at = excluded.updated_at",
            params![project_id, name, serde_json::to_string(body)?, now()],
        )?;
        Ok(!existed)
    }

    pub fn delete_saved_query(&self, project_id: &str, name: &str) -> Result<bool> {
        Ok(self.conn.lock().execute("DELETE FROM saved_queries WHERE project_id = ?1 AND name = ?2", params![project_id, name])? > 0)
    }

    // ---- backup ----

    /// Writes a consistent, compressed snapshot of the database.
    pub fn snapshot(&self) -> Result<Vec<u8>> {
        let tmp = self.path.with_extension(format!("snapshot-{}", rand::random::<u32>()));
        self.conn.lock().execute("VACUUM INTO ?1", [tmp.to_string_lossy()])?;
        let data = std::fs::read(&tmp);
        let _ = std::fs::remove_file(&tmp);
        Ok(zstd::encode_all(&data?[..], 3)?)
    }

    /// Restores a snapshot to `path` if no database exists there yet.
    pub fn restore_if_missing(path: &Path, snapshot: &[u8]) -> Result<bool> {
        if path.exists() {
            return Ok(false);
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let data = zstd::decode_all(snapshot).map_err(|e| anyhow!("decompressing backup: {e}"))?;
        std::fs::write(path, data)?;
        Ok(true)
    }
}

pub const BACKUP_LATEST: &str = "backups/meta-latest.sqlite.zst";

/// Periodically backs the database up to object storage.
pub async fn backup_loop(meta: Arc<Meta>, engine: Arc<crate::store::Engine>, every_secs: u64) {
    if every_secs == 0 {
        return;
    }
    let mut last: Option<Vec<u8>> = None;
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(every_secs));
    loop {
        tick.tick().await;
        if let Err(e) = backup_once(&meta, &engine, &mut last).await {
            tracing::warn!(error = %e, "metadata backup failed");
        }
    }
}

pub async fn backup_once(meta: &Arc<Meta>, engine: &crate::store::Engine, last: &mut Option<Vec<u8>>) -> Result<()> {
    let m = meta.clone();
    let snap = tokio::task::spawn_blocking(move || m.snapshot()).await??;
    // Compare uncompressed content cheaply via a hash of the snapshot bytes.
    if last.as_ref().is_some_and(|l| *l == snap) {
        return Ok(());
    }
    let day = Utc::now().format("%Y-%m-%d");
    engine.put_object(BACKUP_LATEST, snap.clone().into()).await?;
    engine.put_object(&format!("backups/meta-{day}.sqlite.zst"), snap.clone().into()).await?;
    *last = Some(snap);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn login_flow_and_keys() {
        let dir = tempfile::tempdir().unwrap();
        let m = Meta::open(&dir.path().join("m.sqlite")).unwrap();
        let t = m.create_login_code("a@example.com").unwrap();
        assert_eq!(t.len(), 6);
        assert!(!m.consume_login_code("b@example.com", &t).unwrap(), "codes belong to one address");
        assert!(m.consume_login_code("a@example.com", &t).unwrap());
        assert!(!m.consume_login_code("a@example.com", &t).unwrap(), "single use");
        let (u, created) = m.ensure_user("a@example.com").unwrap();
        assert!(created);
        let s = m.create_session(&u.id).unwrap();
        assert_eq!(m.session_user(&s).unwrap().unwrap().id, u.id);
        let (p, keys) = m.create_project(Some(&u.id), "Site").unwrap();
        assert_eq!(keys.len(), 3);
        let master = &keys[0].1;
        assert!(m.authenticate(&p.id, master).unwrap().unwrap().master);
        let read = m.authenticate(&p.id, &keys[1].1).unwrap().unwrap();
        assert!(read.can(auth::Op::Query) && !read.can(auth::Op::Write));
        assert!(m.authenticate("other", master).unwrap().is_none(), "keys are scoped to their project");
        let scoped = auth::encrypt_scoped(master, &serde_json::json!({"allowed_operations": ["write"]})).unwrap();
        let a = m.authenticate(&p.id, &scoped).unwrap().unwrap();
        assert!(a.can(auth::Op::Write) && !a.can(auth::Op::Query));
        let mut k = keys[2].0.clone();
        k.is_active = false;
        m.update_key(&k).unwrap();
        assert!(m.authenticate(&p.id, &keys[2].1).unwrap().is_none(), "revoked");
        let snap = m.snapshot().unwrap();
        let p2 = dir.path().join("restored.sqlite");
        assert!(Meta::restore_if_missing(&p2, &snap).unwrap());
        let m2 = Meta::open(&p2).unwrap();
        assert_eq!(m2.projects_for(&u.id).unwrap().len(), 1);
    }

    #[test]
    fn expired_login_code() {
        let dir = tempfile::tempdir().unwrap();
        let m = Meta::open(&dir.path().join("m.sqlite")).unwrap();
        let t = m.create_login_code("a@example.com").unwrap();
        m.conn.lock().execute("UPDATE login_tokens SET expires_at = ?1", [now() - 1]).unwrap();
        assert!(!m.consume_login_code("a@example.com", &t).unwrap());
    }

    #[test]
    fn code_attempts_are_limited_and_new_codes_replace_old() {
        let dir = tempfile::tempdir().unwrap();
        let m = Meta::open(&dir.path().join("m.sqlite")).unwrap();
        let old = m.create_login_code("a@example.com").unwrap();
        let t = m.create_login_code("a@example.com").unwrap();
        if old != t {
            assert!(!m.consume_login_code("a@example.com", &old).unwrap(), "an earlier code stops working");
        }
        let wrong = if t == "000000" { "111111" } else { "000000" };
        for _ in 0..MAX_CODE_ATTEMPTS {
            assert!(!m.consume_login_code("a@example.com", wrong).unwrap());
        }
        assert!(!m.consume_login_code("a@example.com", &t).unwrap(), "burned after too many wrong guesses");
    }
}
