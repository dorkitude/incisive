//! API keys, permissions, and legacy scoped keys.

use crate::query::Filter;
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

/// Every API key starts with this prefix.
pub const KEY_PREFIX: &str = "sk_incisive_";
/// Random characters after the prefix. The total length is 32 so that a master key can also
/// derive AES keys for legacy scoped keys, which client libraries require to be 32 characters.
const KEY_RANDOM_LEN: usize = 20;
const ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// Every account master key starts with this prefix.
pub const ACCOUNT_KEY_PREFIX: &str = "ak_incisive_";
/// Random characters after the account key prefix.
const ACCOUNT_KEY_RANDOM_LEN: usize = 32;

/// The permissions a custom key can be granted.
pub const PERMISSIONS: [&str; 6] = ["writes", "queries", "saved_queries", "cached_queries", "datasets", "schema"];

/// Generates a new random API key.
pub fn generate_key() -> String {
    random_key(KEY_PREFIX, KEY_RANDOM_LEN)
}

/// Generates a new random account master key.
pub fn generate_account_key() -> String {
    random_key(ACCOUNT_KEY_PREFIX, ACCOUNT_KEY_RANDOM_LEN)
}

fn random_key(prefix: &str, len: usize) -> String {
    let mut s = String::from(prefix);
    for _ in 0..len {
        // 248 = 4 * 62, so rejection keeps the distribution uniform.
        let mut b: u8 = rand::random();
        while b >= 248 {
            b = rand::random();
        }
        s.push(ALPHABET[(b % 62) as usize] as char);
    }
    s
}

/// A random URL-safe token for login links and sessions.
pub fn random_token() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

pub fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

/// What a request wants to do.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Op {
    Write,
    Query,
    Schema,
    /// Read saved and cached query definitions and results.
    SavedRead,
    /// Create, update or delete saved queries.
    SavedManage,
    Datasets,
    Delete,
    Keys,
    /// Rename the project.
    Settings,
}

/// Options of a custom key, in the wire format of the keys API.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct KeyOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub writes: Option<WritesOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub queries: Option<QueriesOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub saved_queries: Option<NamedOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cached_queries: Option<NamedOptions>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub datasets: Option<Value>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct WritesOptions {
    /// Properties merged into every written event, overriding the client's.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub autofill: Option<Map<String, Value>>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct QueriesOptions {
    /// Filters added to every query.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub filters: Vec<Filter>,
}

#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct NamedOptions {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub allowed: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub blocked: Option<Vec<String>>,
}

impl NamedOptions {
    fn permits(&self, name: &str) -> bool {
        if self.blocked.as_ref().is_some_and(|b| b.iter().any(|n| n == name)) {
            return false;
        }
        self.allowed.as_ref().is_none_or(|a| a.iter().any(|n| n == name))
    }
}

/// The resolved access of an authenticated request.
#[derive(Clone, Debug)]
pub struct Access {
    pub project_id: String,
    pub key_id: Option<String>,
    pub master: bool,
    pub permitted: Vec<String>,
    pub options: KeyOptions,
    /// For legacy scoped keys: which of read/write are allowed.
    pub scoped_ops: Option<Vec<String>>,
    pub scoped_filters: Vec<Filter>,
}

impl Access {
    pub fn master(project_id: &str, key_id: Option<String>) -> Access {
        Access { project_id: project_id.into(), key_id, master: true, permitted: vec![], options: KeyOptions::default(), scoped_ops: None, scoped_filters: vec![] }
    }

    pub fn can(&self, op: Op) -> bool {
        if self.master {
            return true;
        }
        if let Some(ops) = &self.scoped_ops {
            let has = |s: &str| ops.iter().any(|o| o == s);
            return match op {
                Op::Write => has("write"),
                Op::Query | Op::Schema | Op::SavedRead => has("read"),
                _ => false,
            };
        }
        let has = |s: &str| self.permitted.iter().any(|p| p == s);
        match op {
            Op::Write => has("writes"),
            Op::Query => has("queries"),
            // Read keys commonly drive schema discovery in dashboards, so queries imply schema.
            Op::Schema => has("schema") || has("queries"),
            Op::SavedRead => has("saved_queries") || has("cached_queries"),
            Op::Datasets => has("datasets"),
            Op::SavedManage | Op::Delete | Op::Keys | Op::Settings => false,
        }
    }

    /// Whether this key may read the named saved query.
    pub fn can_read_saved(&self, name: &str) -> bool {
        if self.master || self.scoped_ops.is_some() {
            return self.can(Op::SavedRead);
        }
        let ok = |o: &Option<NamedOptions>, perm: &str| self.permitted.iter().any(|p| p == perm) && o.as_ref().is_none_or(|o| o.permits(name));
        ok(&self.options.saved_queries, "saved_queries") || ok(&self.options.cached_queries, "cached_queries")
    }

    /// Filters added to every query made with this key.
    pub fn query_filters(&self) -> Vec<Filter> {
        let mut f = self.scoped_filters.clone();
        if let Some(q) = &self.options.queries {
            f.extend(q.filters.iter().cloned());
        }
        f
    }

    pub fn autofill(&self) -> Option<&Map<String, Value>> {
        self.options.writes.as_ref().and_then(|w| w.autofill.as_ref())
    }
}

/// Validates the permissions and options of a custom key from the keys API.
pub fn validate_key_spec(permitted: &[String], options: &KeyOptions) -> Result<(), String> {
    for p in permitted {
        if !PERMISSIONS.contains(&p.as_str()) {
            return Err(format!("unknown permission {p:?}; expected one of {}", PERMISSIONS.join(", ")));
        }
    }
    if let Some(q) = &options.queries {
        crate::query::validate_filters(&q.filters).map_err(|e| e.message)?;
    }
    if let Some(af) = options.writes.as_ref().and_then(|w| w.autofill.as_ref())
        && (af.contains_key(crate::event::RESERVED) || af.contains_key(crate::event::LEGACY_RESERVED)) {
            return Err("autofill cannot set the reserved metadata object".into());
        }
    Ok(())
}

// ---- legacy scoped keys ----

type Aes256CbcDec = cbc::Decryptor<aes::Aes256>;
type Aes256CbcEnc = cbc::Encryptor<aes::Aes256>;

fn aes_key(master: &str) -> Option<[u8; 32]> {
    if master.len() == 64 && master.bytes().all(|b| b.is_ascii_hexdigit()) {
        return hex::decode(master).ok()?.try_into().ok();
    }
    // Shorter keys are padded PKCS7-style to 32 bytes, as the client libraries do.
    let b = master.as_bytes();
    if b.is_empty() || b.len() > 32 {
        return None;
    }
    let mut key = [(32 - b.len()) as u8; 32];
    key[..b.len()].copy_from_slice(b);
    Some(key)
}

/// Decrypts a legacy scoped key (`hex(iv) || hex(ciphertext)`) with a project master key,
/// tolerating the padding differences between client libraries.
pub fn decrypt_scoped(master: &str, scoped: &str) -> Option<Map<String, Value>> {
    use cbc::cipher::{BlockModeDecrypt, KeyIvInit};
    let key = aes_key(master)?;
    if scoped.len() < 64 || !scoped.bytes().all(|b| b.is_ascii_hexdigit()) {
        return None;
    }
    let raw = hex::decode(scoped).ok()?;
    let (iv, ct) = raw.split_at(16);
    if ct.is_empty() || ct.len() % 16 != 0 {
        return None;
    }
    let mut buf = ct.to_vec();
    let dec = Aes256CbcDec::new_from_slices(&key, iv).ok()?;
    let pt = dec.decrypt_padded::<cbc::cipher::block_padding::NoPadding>(&mut buf).ok()?;
    let parse = |b: &[u8]| serde_json::from_slice::<Value>(b).ok().and_then(|v| v.as_object().cloned());
    if let Some(&n) = pt.last() {
        let n = n as usize;
        if (1..=32).contains(&n) && n <= pt.len() && pt[pt.len() - n..].iter().all(|&b| b as usize == n)
            && let Some(m) = parse(&pt[..pt.len() - n]) {
                return Some(m);
            }
    }
    parse(pt)
}

/// Encrypts scoped key options (PKCS7, block 16). Used by the web UI and tests.
pub fn encrypt_scoped(master: &str, options: &Value) -> Option<String> {
    use cbc::cipher::{BlockModeEncrypt, KeyIvInit};
    let key = aes_key(master)?;
    let iv: [u8; 16] = rand::random();
    let pt = serde_json::to_vec(options).ok()?;
    let ct = Aes256CbcEnc::new_from_slices(&key, &iv).ok()?.encrypt_padded_vec::<cbc::cipher::block_padding::Pkcs7>(&pt);
    Some(format!("{}{}", hex::encode(iv), hex::encode(ct)))
}

/// Turns decrypted scoped key options into an access for the project.
pub fn scoped_access(project_id: &str, opts: &Map<String, Value>) -> Result<Access, String> {
    let ops: Vec<String> = match opts.get("allowed_operations") {
        None | Some(Value::Null) => vec!["read".into(), "write".into()],
        Some(Value::Array(a)) => a.iter().filter_map(|v| v.as_str().map(String::from)).collect(),
        Some(_) => return Err("allowed_operations must be a list".into()),
    };
    let filters: Vec<Filter> = match opts.get("filters") {
        None | Some(Value::Null) => vec![],
        Some(v) => serde_json::from_value(v.clone()).map_err(|e| format!("invalid scoped key filters: {e}"))?,
    };
    crate::query::validate_filters(&filters).map_err(|e| e.message)?;
    Ok(Access {
        project_id: project_id.into(),
        key_id: None,
        master: false,
        permitted: vec![],
        options: KeyOptions::default(),
        scoped_ops: Some(ops),
        scoped_filters: filters,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_have_prefix_and_length() {
        let k = generate_key();
        assert!(k.starts_with(KEY_PREFIX));
        assert_eq!(k.len(), 32);
        assert_ne!(k, generate_key());
        let a = generate_account_key();
        assert!(a.starts_with(ACCOUNT_KEY_PREFIX) && !a.starts_with(KEY_PREFIX));
        assert_eq!(a.len(), ACCOUNT_KEY_PREFIX.len() + 32);
    }

    #[test]
    fn upstream_scoped_vectors() {
        let mut n = 0;
        for (file, key, sk, want) in [
            (include_str!("../tests/data/scoped_python.json"), "key", "scoped_key", "options"),
            (include_str!("../tests/data/scoped_php.json"), "masterKey", "scopedKey", "decrypted"),
        ] {
            let vs: Vec<Value> = serde_json::from_str(file).unwrap();
            for v in vs {
                let (k, s) = (v[key].as_str().unwrap(), v[sk].as_str().unwrap());
                let got = decrypt_scoped(k, s).unwrap_or_else(|| panic!("decrypt failed: {v}"));
                assert_eq!(Value::Object(got), v[want], "{v}");
                assert!(decrypt_scoped(k, &s.to_uppercase()).is_some());
                n += 1;
            }
        }
        assert!(n >= 20, "only {n} vectors");
    }

    #[test]
    fn round_trip_with_our_keys() {
        let k = generate_key();
        let opts = serde_json::json!({"allowed_operations": ["read"], "filters": [{"property_name": "a", "operator": "eq", "property_value": 1}]});
        let s = encrypt_scoped(&k, &opts).unwrap();
        assert_eq!(Value::Object(decrypt_scoped(&k, &s).unwrap()), opts);
        assert!(decrypt_scoped(&generate_key(), &s).is_none());
    }

    #[test]
    fn permissions() {
        let a = Access { permitted: vec!["queries".into()], master: false, ..Access::master("p", None) };
        assert!(a.can(Op::Query) && a.can(Op::Schema) && !a.can(Op::Write) && !a.can(Op::Delete));
        let w = Access { permitted: vec!["writes".into()], master: false, ..Access::master("p", None) };
        assert!(w.can(Op::Write) && !w.can(Op::Query) && !w.can(Op::Schema));
    }
}
