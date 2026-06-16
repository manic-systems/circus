//! Typed, per-project notification channels.
//!
//! A [`NotificationChannel`] is a fully-resolved, ready-to-deliver channel: its
//! secrets are decrypted and its URLs validated. The same type is used on the
//! immediate-delivery path and the retry-queue path, so behavior is identical
//! regardless of whether the retry queue is enabled.
//!
//! Secrets live with the type that owns them:
//! [`NotificationChannel::from_stored`] decrypts when loading from the
//! database, and [`NotificationChannel::to_stored`] re-encrypts when persisting
//! (DB row or queue payload). The plaintext secret only ever exists in memory.

use std::{
  collections::BTreeMap,
  fmt::{Debug, Formatter, Result as FmtResult},
};

use circus_common::{
  config::EmailConfig,
  crypto::{decrypt_secret, encrypt_secret},
  error::{CiError, Result as CiResult},
  validate::validate_https_webhook_url,
};
use lettre::{
  AsyncSmtpTransport,
  AsyncTransport,
  Message,
  Tokio1Executor,
  message::{Mailbox, header::ContentType},
  transport::smtp::authentication::Credentials,
};
use serde::{Deserialize, Serialize};
use tracing::{info, warn};

use crate::{
  BuildEvent,
  http_client,
  parse_gitea_repo,
  parse_github_repo,
  parse_gitlab_project,
};

/// HTTP header carrying the HMAC-SHA256 signature of a webhook body.
const SIGNATURE_HEADER: &str = "X-Circus-Signature";
/// Header names a caller may not override via custom `headers`.
const RESERVED_HEADERS: &[&str] = &["content-type", SIGNATURE_HEADER];

/// Generic outbound HTTPS webhook with optional HMAC signing and headers.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct WebhookChannel {
  pub url:             String,
  /// HMAC-SHA256 signing key. Plaintext in memory, encrypted at rest.
  #[serde(default, skip_serializing_if = "Option::is_none")]
  pub secret:          Option<String>,
  /// Extra static headers (e.g. `Authorization`). Values are encrypted at
  /// rest.
  #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
  pub headers:         BTreeMap<String, String>,
  #[serde(default)]
  pub on_failure_only: bool,
}

impl Debug for WebhookChannel {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    f.debug_struct("WebhookChannel")
      .field("url", &self.url)
      .field("secret", &self.secret.as_ref().map(|_| "[REDACTED]"))
      .field("headers", &format!("{} header(s)", self.headers.len()))
      .field("on_failure_only", &self.on_failure_only)
      .finish()
  }
}

/// GitHub commit-status channel. The repo is derived from the project URL.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct GithubStatusChannel {
  pub token: String,
}

/// Gitea/Forgejo commit-status channel.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct GiteaStatusChannel {
  pub base_url: String,
  pub token:    String,
}

/// GitLab commit-status channel.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct GitlabStatusChannel {
  pub base_url: String,
  pub token:    String,
}

/// Slack incoming-webhook channel.
#[derive(Clone, Default, Serialize, Deserialize)]
pub struct SlackChannel {
  pub webhook_url:     String,
  #[serde(default)]
  pub on_failure_only: bool,
}

/// A resolved notification channel for a project.
#[derive(Debug, Clone)]
pub enum NotificationChannel {
  Webhook(WebhookChannel),
  GithubStatus(GithubStatusChannel),
  GiteaStatus(GiteaStatusChannel),
  GitlabStatus(GitlabStatusChannel),
  Slack(SlackChannel),
  Email(EmailConfig),
}

impl Debug for GithubStatusChannel {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    f.debug_struct("GithubStatusChannel")
      .field("token", &"[REDACTED]")
      .finish()
  }
}

impl Debug for GiteaStatusChannel {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    f.debug_struct("GiteaStatusChannel")
      .field("base_url", &self.base_url)
      .field("token", &"[REDACTED]")
      .finish()
  }
}

impl Debug for GitlabStatusChannel {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    f.debug_struct("GitlabStatusChannel")
      .field("base_url", &self.base_url)
      .field("token", &"[REDACTED]")
      .finish()
  }
}

impl Debug for SlackChannel {
  fn fmt(&self, f: &mut Formatter<'_>) -> FmtResult {
    f.debug_struct("SlackChannel")
      .field("webhook_url", &"[REDACTED]")
      .field("on_failure_only", &self.on_failure_only)
      .finish()
  }
}

/// Delivery behavior shared by every channel kind. Implemented per concrete
/// channel struct; [`NotificationChannel`] dispatches by value (no `dyn`).
trait Notifier {
  /// Whether this channel should fire for the given event (honors
  /// `on_failure_only`). Commit-status channels always fire so the status
  /// reflects the build's current state.
  fn applies_to(&self, event: &BuildEvent) -> bool;

  /// Deliver the notification. Returns a human-readable error on failure so the
  /// retry queue can record `last_error`.
  async fn deliver(&self, event: &BuildEvent) -> Result<(), String>;
}

impl NotificationChannel {
  /// The stored `notification_type` discriminant for this channel.
  #[must_use]
  pub const fn notification_type(&self) -> &'static str {
    match self {
      Self::Webhook(_) => "webhook",
      Self::GithubStatus(_) => "github_status",
      Self::GiteaStatus(_) => "gitea_status",
      Self::GitlabStatus(_) => "gitlab_status",
      Self::Slack(_) => "slack",
      Self::Email(_) => "email",
    }
  }

  /// True for commit-status channels (`github`/`gitea`/`gitlab`), which are the
  /// only channels dispatched on build-created and build-started transitions.
  #[must_use]
  pub const fn is_commit_status(&self) -> bool {
    matches!(
      self,
      Self::GithubStatus(_) | Self::GiteaStatus(_) | Self::GitlabStatus(_)
    )
  }

  /// Whether this channel should fire for the given event.
  #[must_use]
  pub fn applies_to(&self, event: &BuildEvent) -> bool {
    match self {
      Self::Webhook(c) => c.applies_to(event),
      Self::GithubStatus(c) => c.applies_to(event),
      Self::GiteaStatus(c) => c.applies_to(event),
      Self::GitlabStatus(c) => c.applies_to(event),
      Self::Slack(c) => c.applies_to(event),
      Self::Email(c) => c.applies_to(event),
    }
  }

  /// Deliver this channel's notification for the event.
  ///
  /// # Errors
  ///
  /// Returns a human-readable error string when delivery fails.
  pub async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    match self {
      Self::Webhook(c) => c.deliver(event).await,
      Self::GithubStatus(c) => c.deliver(event).await,
      Self::GiteaStatus(c) => c.deliver(event).await,
      Self::GitlabStatus(c) => c.deliver(event).await,
      Self::Slack(c) => c.deliver(event).await,
      Self::Email(c) => c.deliver(event).await,
    }
  }

  /// Parse a channel from a stored config blob, decrypting any secret fields
  /// and validating URLs.
  ///
  /// # Errors
  ///
  /// Returns an error if the type is unknown, the config shape is invalid, a
  /// URL fails the SSRF/HTTPS guard, or decryption fails.
  pub fn from_stored(
    notification_type: &str,
    config: &serde_json::Value,
    key: Option<&str>,
  ) -> CiResult<Self> {
    let invalid = |e: serde_json::Error| {
      CiError::Validation(format!(
        "invalid {notification_type} notification config: {e}"
      ))
    };
    match notification_type {
      "webhook" => {
        let mut c: WebhookChannel =
          serde_json::from_value(config.clone()).map_err(invalid)?;
        validate_https_webhook_url(&c.url).map_err(CiError::Validation)?;
        c.secret = c.secret.map(|s| decrypt_secret(&s, key)).transpose()?;
        c.headers = c
          .headers
          .into_iter()
          .map(|(k, v)| decrypt_secret(&v, key).map(|v| (k, v)))
          .collect::<CiResult<_>>()?;
        Ok(Self::Webhook(c))
      },
      "github_status" => {
        let mut c: GithubStatusChannel =
          serde_json::from_value(config.clone()).map_err(invalid)?;
        c.token = decrypt_secret(&c.token, key)?;
        Ok(Self::GithubStatus(c))
      },
      "gitea_status" | "forgejo_status" => {
        let mut c: GiteaStatusChannel =
          serde_json::from_value(config.clone()).map_err(invalid)?;
        c.token = decrypt_secret(&c.token, key)?;
        Ok(Self::GiteaStatus(c))
      },
      "gitlab_status" => {
        let mut c: GitlabStatusChannel =
          serde_json::from_value(config.clone()).map_err(invalid)?;
        c.token = decrypt_secret(&c.token, key)?;
        Ok(Self::GitlabStatus(c))
      },
      "slack" => {
        let mut c: SlackChannel =
          serde_json::from_value(config.clone()).map_err(invalid)?;
        c.webhook_url = decrypt_secret(&c.webhook_url, key)?;
        validate_https_webhook_url(&c.webhook_url)
          .map_err(CiError::Validation)?;
        Ok(Self::Slack(c))
      },
      "email" => {
        let mut c: EmailConfig =
          serde_json::from_value(config.clone()).map_err(invalid)?;
        c.smtp_password = c
          .smtp_password
          .map(|s| decrypt_secret(&s, key))
          .transpose()?;
        Ok(Self::Email(c))
      },
      other => {
        Err(CiError::Validation(format!(
          "unknown notification type '{other}'"
        )))
      },
    }
  }

  /// Serialize this channel into its stored form, encrypting secret fields.
  /// Returns the `notification_type` discriminant and the encrypted config.
  ///
  /// # Errors
  ///
  /// Returns an error if encryption or serialization fails.
  pub fn to_stored(
    &self,
    key: Option<&str>,
  ) -> CiResult<(&'static str, serde_json::Value)> {
    let value = match self {
      Self::Webhook(c) => {
        let mut c = c.clone();
        c.secret = c.secret.map(|s| encrypt_secret(&s, key)).transpose()?;
        c.headers = c
          .headers
          .into_iter()
          .map(|(k, v)| encrypt_secret(&v, key).map(|v| (k, v)))
          .collect::<CiResult<_>>()?;
        serde_json::to_value(c)
      },
      Self::GithubStatus(c) => {
        let mut c = c.clone();
        c.token = encrypt_secret(&c.token, key)?;
        serde_json::to_value(c)
      },
      Self::GiteaStatus(c) => {
        let mut c = c.clone();
        c.token = encrypt_secret(&c.token, key)?;
        serde_json::to_value(c)
      },
      Self::GitlabStatus(c) => {
        let mut c = c.clone();
        c.token = encrypt_secret(&c.token, key)?;
        serde_json::to_value(c)
      },
      Self::Slack(c) => {
        let mut c = c.clone();
        c.webhook_url = encrypt_secret(&c.webhook_url, key)?;
        serde_json::to_value(c)
      },
      Self::Email(c) => {
        let mut c = c.clone();
        c.smtp_password = c
          .smtp_password
          .map(|s| encrypt_secret(&s, key))
          .transpose()?;
        serde_json::to_value(c)
      },
    }
    .map_err(|e| {
      CiError::Validation(format!(
        "failed to serialize notification config: {e}"
      ))
    })?;
    Ok((self.notification_type(), value))
  }

  /// Encrypt the secret fields of a stored config blob in place, validating it
  /// in the process. Used by write paths (declarative sync, dashboard) before
  /// persisting an admin-provided plaintext config.
  ///
  /// # Errors
  ///
  /// Returns an error if the config is invalid or encryption fails.
  pub fn encrypt_into_stored(
    notification_type: &str,
    config: &serde_json::Value,
    key: Option<&str>,
  ) -> CiResult<serde_json::Value> {
    // Round-trip through the typed channel: from_stored validates and treats
    // already-encrypted values as plaintext (decrypt is a no-op without the
    // v1: prefix), then to_stored encrypts. Idempotent on re-sync.
    let channel = Self::from_stored(notification_type, config, key)?;
    Ok(channel.to_stored(key)?.1)
  }
}

impl Notifier for WebhookChannel {
  fn applies_to(&self, event: &BuildEvent) -> bool {
    !self.on_failure_only || event.is_failure()
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let payload = serde_json::json!({
      "build_id":     event.build_id,
      "build_status": event.generic_status(),
      "build_job":    event.job_name,
      "build_drv":    event.drv_path,
      "build_output": event.build_output.as_deref().unwrap_or(""),
      "project_name": event.project_name,
      "project_url":  event.project_url,
      "commit_hash":  event.commit_hash,
    });
    // Serialize once: the exact bytes are both signed and sent.
    let body = serde_json::to_vec(&payload)
      .map_err(|e| format!("Failed to serialize webhook payload: {e}"))?;

    let mut req = http_client()
      .post(&self.url)
      .header("Content-Type", "application/json");

    if let Some(secret) = &self.secret {
      req = req.header(SIGNATURE_HEADER, sign_body(secret, &body)?);
    }
    for (name, value) in &self.headers {
      if RESERVED_HEADERS
        .iter()
        .any(|r| r.eq_ignore_ascii_case(name))
      {
        warn!(header = %name, "Ignoring reserved webhook header override");
        continue;
      }
      req = req.header(name, value);
    }

    let resp = req
      .body(body)
      .send()
      .await
      .map_err(|e| format!("Webhook request failed: {e}"))?;
    if resp.status().is_success() {
      info!(build_id = %event.build_id, "Webhook notification sent");
      Ok(())
    } else {
      Err(format!("Webhook returned status: {}", resp.status()))
    }
  }
}

impl Notifier for GithubStatusChannel {
  fn applies_to(&self, _event: &BuildEvent) -> bool {
    true
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let (owner, repo) =
      parse_github_repo(&event.project_url).ok_or_else(|| {
        format!("Cannot parse GitHub repo from {}", event.project_url)
      })?;
    let (state, description) = event.github_state();
    let url = format!(
      "https://api.github.com/repos/{owner}/{repo}/statuses/{}",
      event.commit_hash
    );
    let body = serde_json::json!({
      "state": state,
      "description": description,
      "context": format!("circus/{}", event.job_name),
    });

    let resp = http_client()
      .post(&url)
      .header("Authorization", format!("token {}", self.token))
      .header("User-Agent", "circus")
      .header("Accept", "application/vnd.github+json")
      .json(&body)
      .send()
      .await
      .map_err(|e| format!("GitHub API request failed: {e}"))?;

    let status = resp.status();
    let rate_limit = super::extract_rate_limit_from_headers(resp.headers());
    if !status.is_success() {
      let text = resp.text().await.unwrap_or_default();
      return Err(format!("GitHub API returned {status}: {text}"));
    }
    info!(build_id = %event.build_id, "Set GitHub commit status: {state}");
    super::apply_github_rate_limit(rate_limit).await;
    Ok(())
  }
}

impl Notifier for GiteaStatusChannel {
  fn applies_to(&self, _event: &BuildEvent) -> bool {
    true
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let (owner, repo) = parse_gitea_repo(&event.project_url, &self.base_url)
      .ok_or_else(|| {
        format!("Cannot parse Gitea repo from {}", event.project_url)
      })?;
    let (state, description) = event.github_state();
    let url = format!(
      "{}/api/v1/repos/{owner}/{repo}/statuses/{}",
      self.base_url.trim_end_matches('/'),
      event.commit_hash
    );
    let body = serde_json::json!({
      "state": state,
      "description": description,
      "context": format!("circus/{}", event.job_name),
    });

    let resp = http_client()
      .post(&url)
      .header("Authorization", format!("token {}", self.token))
      .json(&body)
      .send()
      .await
      .map_err(|e| format!("Gitea API request failed: {e}"))?;
    if resp.status().is_success() {
      info!(build_id = %event.build_id, "Set Gitea commit status: {state}");
      Ok(())
    } else {
      let status = resp.status();
      let text = resp.text().await.unwrap_or_default();
      Err(format!("Gitea API returned {status}: {text}"))
    }
  }
}

impl Notifier for GitlabStatusChannel {
  fn applies_to(&self, _event: &BuildEvent) -> bool {
    true
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let project_path = parse_gitlab_project(&event.project_url, &self.base_url)
      .ok_or_else(|| {
        format!("Cannot parse GitLab project from {}", event.project_url)
      })?;
    let (state, description) = event.gitlab_state();
    let encoded_project = urlencoding::encode(&project_path);
    let url = format!(
      "{}/api/v4/projects/{}/statuses/{}",
      self.base_url.trim_end_matches('/'),
      encoded_project,
      event.commit_hash
    );
    let body = serde_json::json!({
      "state": state,
      "description": description,
      "name": format!("circus/{}", event.job_name),
    });

    let resp = http_client()
      .post(&url)
      .header("PRIVATE-TOKEN", &self.token)
      .json(&body)
      .send()
      .await
      .map_err(|e| format!("GitLab API request failed: {e}"))?;
    if resp.status().is_success() {
      info!(build_id = %event.build_id, "Set GitLab commit status: {state}");
      Ok(())
    } else {
      let status = resp.status();
      let text = resp.text().await.unwrap_or_default();
      Err(format!("GitLab API returned {status}: {text}"))
    }
  }
}

impl Notifier for SlackChannel {
  fn applies_to(&self, event: &BuildEvent) -> bool {
    !self.on_failure_only || event.is_failure()
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let status = event.generic_status();
    let body = serde_json::json!({
      "text": format!("CI: {} - {status}", event.job_name),
      "blocks": [{
        "type": "section",
        "text": {
          "type": "mrkdwn",
          "text": format!(
            "*{}* - *{status}*\nProject: {} | Commit: `{}`",
            event.job_name, event.project_name, event.commit_hash
          ),
        },
      }],
    });

    let resp = http_client()
      .post(&self.webhook_url)
      .json(&body)
      .send()
      .await
      .map_err(|e| format!("Slack webhook request failed: {e}"))?;
    if resp.status().as_u16() == 429 {
      let retry = resp
        .headers()
        .get("retry-after")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("60");
      return Err(format!("Slack rate limited; retry-after={retry}"));
    }
    if resp.status().is_success() {
      info!(build_id = %event.build_id, "Slack notification sent");
      Ok(())
    } else {
      Err(format!("Slack returned status: {}", resp.status()))
    }
  }
}

impl Notifier for EmailConfig {
  fn applies_to(&self, event: &BuildEvent) -> bool {
    !self.on_failure_only || event.is_failure()
  }

  async fn deliver(&self, event: &BuildEvent) -> Result<(), String> {
    let status = event.email_status();
    let subject = format!(
      "[circus] {status} - {} ({})",
      event.job_name, event.project_name
    );
    let body = format!(
      "Build notification from circus\n\nProject: {}\nJob: {}\nStatus: \
       {}\nDerivation: {}\nOutput: {}\nBuild ID: {}\n",
      event.project_name,
      event.job_name,
      status,
      event.drv_path,
      event.build_output.as_deref().unwrap_or("N/A"),
      event.build_id,
    );

    let from: Mailbox = self.from_address.parse().map_err(|e| {
      format!("Invalid from address '{}': {e}", self.from_address)
    })?;

    let mailer = if self.tls {
      AsyncSmtpTransport::<Tokio1Executor>::relay(&self.smtp_host)
        .map_err(|e| format!("Failed to create SMTP transport: {e}"))?
    } else {
      AsyncSmtpTransport::<Tokio1Executor>::builder_dangerous(&self.smtp_host)
    }
    .port(self.smtp_port);
    let mailer = if let (Some(user), Some(pass)) =
      (&self.smtp_user, &self.smtp_password)
    {
      mailer.credentials(Credentials::new(user.clone(), pass.clone()))
    } else {
      mailer
    }
    .build();

    for to_addr in &self.to_addresses {
      let to: Mailbox = match to_addr.parse() {
        Ok(addr) => addr,
        Err(e) => {
          warn!("Invalid to address '{to_addr}': {e}");
          continue;
        },
      };
      let email = Message::builder()
        .from(from.clone())
        .to(to)
        .subject(&subject)
        .header(ContentType::TEXT_PLAIN)
        .body(body.clone())
        .map_err(|e| format!("Failed to build email: {e}"))?;
      AsyncTransport::send(&mailer, email)
        .await
        .map_err(|e| format!("Failed to send email to {to_addr}: {e}"))?;
      info!(build_id = %event.build_id, to = to_addr, "Email notification sent");
    }
    Ok(())
  }
}

/// Compute the `sha256=<hex>` HMAC signature of a webhook body.
fn sign_body(secret: &str, body: &[u8]) -> Result<String, String> {
  use hmac::{Hmac, KeyInit, Mac};
  use sha2::Sha256;

  let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes())
    .map_err(|e| format!("Invalid webhook signing key: {e}"))?;
  mac.update(body);
  Ok(format!(
    "sha256={}",
    hex::encode(mac.finalize().into_bytes())
  ))
}

#[cfg(test)]
#[expect(clippy::unwrap_used, clippy::panic, reason = "fine in tests")]
mod tests {
  use super::*;

  const KEY: Option<&str> = Some("test-encryption-key");

  #[test]
  fn webhook_round_trips_secrets_through_storage() {
    let config = serde_json::json!({
      "url": "https://hooks.example.com/ci",
      "secret": "super-secret",
      "headers": { "Authorization": "Bearer token123" },
      "on_failure_only": true,
    });

    // Encrypt for storage: url stays plaintext, secret/header values become
    // ciphertext.
    let stored =
      NotificationChannel::encrypt_into_stored("webhook", &config, KEY)
        .unwrap();
    assert_eq!(stored["url"], "https://hooks.example.com/ci");
    assert_ne!(stored["secret"], "super-secret");
    assert!(stored["secret"].as_str().unwrap().starts_with("v1:"));
    assert!(
      stored["headers"]["Authorization"]
        .as_str()
        .unwrap()
        .starts_with("v1:")
    );

    // Load back: secrets are decrypted to their original plaintext.
    let channel =
      NotificationChannel::from_stored("webhook", &stored, KEY).unwrap();
    let NotificationChannel::Webhook(webhook) = channel else {
      panic!("expected webhook channel");
    };
    assert_eq!(webhook.secret.as_deref(), Some("super-secret"));
    assert_eq!(
      webhook.headers.get("Authorization").map(String::as_str),
      Some("Bearer token123")
    );
    assert!(webhook.on_failure_only);
  }

  #[test]
  fn encrypt_into_stored_is_idempotent() {
    // Re-syncing an already-encrypted config (declarative bootstrap re-run)
    // must not double-encrypt.
    let config = serde_json::json!({ "token": "ghp_abc" });
    let once =
      NotificationChannel::encrypt_into_stored("github_status", &config, KEY)
        .unwrap();
    let twice =
      NotificationChannel::encrypt_into_stored("github_status", &once, KEY)
        .unwrap();
    let channel =
      NotificationChannel::from_stored("github_status", &twice, KEY).unwrap();
    let NotificationChannel::GithubStatus(gh) = channel else {
      panic!("expected github status channel");
    };
    assert_eq!(gh.token, "ghp_abc");
  }

  #[test]
  fn slack_round_trips_webhook_url_through_storage() {
    let config = serde_json::json!({
      "webhook_url": "https://hooks.slack.com/services/T/B/secret",
      "on_failure_only": true,
    });

    let stored =
      NotificationChannel::encrypt_into_stored("slack", &config, KEY).unwrap();
    assert_ne!(
      stored["webhook_url"],
      "https://hooks.slack.com/services/T/B/secret"
    );
    assert!(stored["webhook_url"].as_str().unwrap().starts_with("v1:"));

    let channel =
      NotificationChannel::from_stored("slack", &stored, KEY).unwrap();
    let NotificationChannel::Slack(slack) = channel else {
      panic!("expected slack channel");
    };
    assert_eq!(
      slack.webhook_url,
      "https://hooks.slack.com/services/T/B/secret"
    );
    assert!(slack.on_failure_only);
  }

  #[test]
  fn webhook_rejects_plaintext_http() {
    let config = serde_json::json!({ "url": "http://hooks.example.com/ci" });
    let err =
      NotificationChannel::from_stored("webhook", &config, KEY).unwrap_err();
    assert!(err.to_string().contains("https"), "got: {err}");
  }

  #[test]
  fn webhook_rejects_internal_host() {
    let config = serde_json::json!({ "url": "https://169.254.169.254/latest" });
    assert!(NotificationChannel::from_stored("webhook", &config, KEY).is_err());
  }

  #[test]
  fn slack_rejects_internal_host() {
    let config =
      serde_json::json!({ "webhook_url": "https://localhost/services/x" });
    assert!(NotificationChannel::from_stored("slack", &config, KEY).is_err());
  }

  #[test]
  fn unknown_type_is_rejected() {
    let config = serde_json::json!({});
    assert!(
      NotificationChannel::from_stored("carrier_pigeon", &config, KEY).is_err()
    );
  }

  #[test]
  fn signature_is_deterministic_and_key_sensitive() {
    let body = br#"{"build":"abc"}"#;
    let a = sign_body("secret", body).unwrap();
    let b = sign_body("secret", body).unwrap();
    let c = sign_body("other", body).unwrap();
    assert_eq!(a, b);
    assert_ne!(a, c);
    assert!(a.starts_with("sha256="));
    // sha256 hex digest is 64 characters.
    assert_eq!(a.len(), "sha256=".len() + 64);
  }

  #[test]
  fn on_failure_only_filters_successful_builds() {
    use circus_common::models::BuildStatus;

    let event = BuildEvent {
      build_id:     uuid::Uuid::nil(),
      status:       BuildStatus::Succeeded,
      job_name:     "job".into(),
      drv_path:     "/nix/store/x.drv".into(),
      build_output: None,
      project_name: "proj".into(),
      project_url:  "https://github.com/o/r".into(),
      commit_hash:  "abc".into(),
    };
    let only_failures = NotificationChannel::Slack(SlackChannel {
      webhook_url:     "https://hooks.slack.com/x".into(),
      on_failure_only: true,
    });
    assert!(!only_failures.applies_to(&event));

    let always = NotificationChannel::Slack(SlackChannel {
      webhook_url:     "https://hooks.slack.com/x".into(),
      on_failure_only: false,
    });
    assert!(always.applies_to(&event));
  }
}
