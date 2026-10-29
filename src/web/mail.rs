//! Outgoing email through a Resend-compatible HTTP API.

use crate::config::Email;
use anyhow::{bail, Result};
use serde_json::json;

pub struct Mailer {
    url: String,
    api_key: String,
    from: String,
    log_only: bool,
    http: reqwest::Client,
    /// The last message sent, for tests and log-only development.
    pub last: parking_lot::Mutex<Option<(String, String, String)>>,
}

impl Mailer {
    pub fn new(cfg: &Email) -> Mailer {
        Mailer {
            url: cfg.resend_url.trim_end_matches('/').to_string(),
            api_key: cfg.api_key.clone(),
            from: cfg.from.clone(),
            log_only: cfg.log_only,
            http: reqwest::Client::builder().timeout(std::time::Duration::from_secs(20)).build().expect("http client"),
            last: Default::default(),
        }
    }

    pub async fn send(&self, to: &str, subject: &str, text: &str, html: &str) -> Result<()> {
        *self.last.lock() = Some((to.to_string(), subject.to_string(), text.to_string()));
        if self.log_only || self.url.is_empty() {
            tracing::info!(to, subject, "email (log only):\n{text}");
            return Ok(());
        }
        let mut req = self.http.post(format!("{}/emails", self.url)).json(&json!({
            "from": self.from, "to": [to], "subject": subject, "text": text, "html": html,
        }));
        // Without a key, a credential-injecting proxy at resend_url can supply it.
        if !self.api_key.is_empty() {
            req = req.bearer_auth(&self.api_key);
        }
        let resp = req.send().await?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            bail!("email API returned {status}: {}", body.chars().take(300).collect::<String>());
        }
        tracing::info!(to, subject, "email sent");
        Ok(())
    }
}
