//! Object storage: R2/S3 (via the standard AWS credential sources) or a local directory.

use crate::config::Storage;
use anyhow::{bail, Context, Result};
use object_store::aws::{AmazonS3Builder, S3ConditionalPut};
use object_store::local::LocalFileSystem;
use object_store::ObjectStore;
use std::sync::Arc;

/// Builds the configured object store.
pub fn build(cfg: &Storage) -> Result<Arc<dyn ObjectStore>> {
    match cfg.backend.as_str() {
        "fs" => {
            std::fs::create_dir_all(&cfg.fs_path)?;
            Ok(Arc::new(LocalFileSystem::new_with_prefix(&cfg.fs_path)?))
        }
        "s3" => {
            if cfg.bucket.is_empty() {
                bail!("storage.bucket is required for the s3 backend");
            }
            let (key, secret) = credentials(&cfg.profile)?;
            let mut b = AmazonS3Builder::new()
                .with_bucket_name(&cfg.bucket)
                .with_region(&cfg.region)
                .with_access_key_id(key)
                .with_secret_access_key(secret)
                .with_conditional_put(S3ConditionalPut::ETagMatch);
            if !cfg.endpoint.is_empty() {
                b = b.with_endpoint(&cfg.endpoint).with_allow_http(cfg.endpoint.starts_with("http://"));
            }
            Ok(Arc::new(b.build()?))
        }
        other => bail!("unknown storage backend {other:?}"),
    }
}

/// Reads credentials from AWS_ACCESS_KEY_ID/AWS_SECRET_ACCESS_KEY, else from the shared
/// credentials file (AWS_SHARED_CREDENTIALS_FILE or ~/.aws/credentials) for the profile
/// (AWS_PROFILE overrides the configured one). Values are never logged.
fn credentials(profile: &str) -> Result<(String, String)> {
    if let (Ok(k), Ok(s)) = (std::env::var("AWS_ACCESS_KEY_ID"), std::env::var("AWS_SECRET_ACCESS_KEY")) {
        return Ok((k, s));
    }
    let profile = std::env::var("AWS_PROFILE").unwrap_or_else(|_| if profile.is_empty() { "default".into() } else { profile.into() });
    let path = std::env::var("AWS_SHARED_CREDENTIALS_FILE").map(Into::into).unwrap_or_else(|_| {
        std::path::PathBuf::from(std::env::var("HOME").unwrap_or_default()).join(".aws/credentials")
    });
    let text = std::fs::read_to_string(&path).with_context(|| format!("reading {}", path.display()))?;
    let (mut key, mut secret, mut in_profile) = (None, None, false);
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_profile = line.trim_matches(|c| c == '[' || c == ']').trim() == profile;
            continue;
        }
        if !in_profile {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            match k.trim() {
                "aws_access_key_id" => key = Some(v.trim().to_string()),
                "aws_secret_access_key" => secret = Some(v.trim().to_string()),
                _ => {}
            }
        }
    }
    match (key, secret) {
        (Some(k), Some(s)) => Ok((k, s)),
        _ => bail!("no credentials for profile {profile:?} in {}", path.display()),
    }
}
