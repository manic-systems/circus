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
  crypto::{decrypt_secret, encrypt_secret},
  error::{CiError, Result as CiResult},
  models::NotificationType,
  validate::validate_https_webhook_url,
};
use circus_config::EmailConfig;
use serde::{Deserialize, Serialize, de::DeserializeOwned};

use crate::BuildEvent;

mod delivery;

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

trait StoredChannel: Clone + Serialize + DeserializeOwned {
  fn validate_stored(&self) -> CiResult<()> {
    Ok(())
  }

  fn encrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()>;

  fn decrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()>;
}

fn from_stored_channel<T: StoredChannel>(
  notification_type: NotificationType,
  config: &serde_json::Value,
  key: Option<&str>,
) -> CiResult<T> {
  let mut channel: T = serde_json::from_value(config.clone()).map_err(|e| {
    CiError::Validation(format!(
      "invalid {notification_type} notification config: {e}"
    ))
  })?;
  channel.decrypt_secrets(key)?;
  channel.validate_stored()?;
  Ok(channel)
}

fn to_stored_value<T: StoredChannel>(
  channel: &T,
  key: Option<&str>,
) -> CiResult<serde_json::Value> {
  let mut channel = channel.clone();
  channel.encrypt_secrets(key)?;
  serde_json::to_value(channel).map_err(|e| {
    CiError::Validation(format!("failed to serialize notification config: {e}"))
  })
}

impl NotificationChannel {
  /// The stored `notification_type` discriminant for this channel.
  #[must_use]
  pub const fn notification_type(&self) -> NotificationType {
    match self {
      Self::Webhook(_) => NotificationType::Webhook,
      Self::GithubStatus(_) => NotificationType::GithubStatus,
      Self::GiteaStatus(_) => NotificationType::GiteaStatus,
      Self::GitlabStatus(_) => NotificationType::GitlabStatus,
      Self::Slack(_) => NotificationType::Slack,
      Self::Email(_) => NotificationType::Email,
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
    notification_type: NotificationType,
    config: &serde_json::Value,
    key: Option<&str>,
  ) -> CiResult<Self> {
    match notification_type {
      NotificationType::Webhook => {
        Ok(Self::Webhook(from_stored_channel(
          notification_type,
          config,
          key,
        )?))
      },
      NotificationType::GithubStatus => {
        Ok(Self::GithubStatus(from_stored_channel(
          notification_type,
          config,
          key,
        )?))
      },
      NotificationType::GiteaStatus | NotificationType::ForgejoStatus => {
        Ok(Self::GiteaStatus(from_stored_channel(
          notification_type,
          config,
          key,
        )?))
      },
      NotificationType::GitlabStatus => {
        Ok(Self::GitlabStatus(from_stored_channel(
          notification_type,
          config,
          key,
        )?))
      },
      NotificationType::Slack => {
        Ok(Self::Slack(from_stored_channel(
          notification_type,
          config,
          key,
        )?))
      },
      NotificationType::Email => {
        Ok(Self::Email(from_stored_channel(
          notification_type,
          config,
          key,
        )?))
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
  ) -> CiResult<(NotificationType, serde_json::Value)> {
    let value = match self {
      Self::Webhook(c) => to_stored_value(c, key)?,
      Self::GithubStatus(c) => to_stored_value(c, key)?,
      Self::GiteaStatus(c) => to_stored_value(c, key)?,
      Self::GitlabStatus(c) => to_stored_value(c, key)?,
      Self::Slack(c) => to_stored_value(c, key)?,
      Self::Email(c) => to_stored_value(c, key)?,
    };
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
    notification_type: NotificationType,
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

impl StoredChannel for WebhookChannel {
  fn validate_stored(&self) -> CiResult<()> {
    validate_https_webhook_url(&self.url).map_err(CiError::Validation)
  }

  fn encrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.secret = self
      .secret
      .take()
      .map(|s| encrypt_secret(&s, key))
      .transpose()?;
    self.headers = std::mem::take(&mut self.headers)
      .into_iter()
      .map(|(k, v)| encrypt_secret(&v, key).map(|v| (k, v)))
      .collect::<CiResult<_>>()?;
    Ok(())
  }

  fn decrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.secret = self
      .secret
      .take()
      .map(|s| decrypt_secret(&s, key))
      .transpose()?;
    self.headers = std::mem::take(&mut self.headers)
      .into_iter()
      .map(|(k, v)| decrypt_secret(&v, key).map(|v| (k, v)))
      .collect::<CiResult<_>>()?;
    Ok(())
  }
}

impl StoredChannel for GithubStatusChannel {
  fn encrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.token = encrypt_secret(&self.token, key)?;
    Ok(())
  }

  fn decrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.token = decrypt_secret(&self.token, key)?;
    Ok(())
  }
}

impl StoredChannel for GiteaStatusChannel {
  fn encrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.token = encrypt_secret(&self.token, key)?;
    Ok(())
  }

  fn decrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.token = decrypt_secret(&self.token, key)?;
    Ok(())
  }
}

impl StoredChannel for GitlabStatusChannel {
  fn encrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.token = encrypt_secret(&self.token, key)?;
    Ok(())
  }

  fn decrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.token = decrypt_secret(&self.token, key)?;
    Ok(())
  }
}

impl StoredChannel for SlackChannel {
  fn validate_stored(&self) -> CiResult<()> {
    validate_https_webhook_url(&self.webhook_url).map_err(CiError::Validation)
  }

  fn encrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.webhook_url = encrypt_secret(&self.webhook_url, key)?;
    Ok(())
  }

  fn decrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.webhook_url = decrypt_secret(&self.webhook_url, key)?;
    Ok(())
  }
}

impl StoredChannel for EmailConfig {
  fn encrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.smtp_password = self
      .smtp_password
      .take()
      .map(|s| encrypt_secret(&s, key))
      .transpose()?;
    Ok(())
  }

  fn decrypt_secrets(&mut self, key: Option<&str>) -> CiResult<()> {
    self.smtp_password = self
      .smtp_password
      .take()
      .map(|s| decrypt_secret(&s, key))
      .transpose()?;
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

  fn nt(s: &str) -> NotificationType {
    s.parse().unwrap()
  }

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
      NotificationChannel::encrypt_into_stored(nt("webhook"), &config, KEY)
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
      NotificationChannel::from_stored(nt("webhook"), &stored, KEY).unwrap();
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
    let once = NotificationChannel::encrypt_into_stored(
      nt("github_status"),
      &config,
      KEY,
    )
    .unwrap();
    let twice =
      NotificationChannel::encrypt_into_stored(nt("github_status"), &once, KEY)
        .unwrap();
    let channel =
      NotificationChannel::from_stored(nt("github_status"), &twice, KEY)
        .unwrap();
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
      NotificationChannel::encrypt_into_stored(nt("slack"), &config, KEY)
        .unwrap();
    assert_ne!(
      stored["webhook_url"],
      "https://hooks.slack.com/services/T/B/secret"
    );
    assert!(stored["webhook_url"].as_str().unwrap().starts_with("v1:"));

    let channel =
      NotificationChannel::from_stored(nt("slack"), &stored, KEY).unwrap();
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
    let err = NotificationChannel::from_stored(nt("webhook"), &config, KEY)
      .unwrap_err();
    assert!(err.to_string().contains("https"), "got: {err}");
  }

  #[test]
  fn webhook_rejects_internal_host() {
    let config = serde_json::json!({ "url": "https://169.254.169.254/latest" });
    assert!(
      NotificationChannel::from_stored(nt("webhook"), &config, KEY).is_err()
    );
  }

  #[test]
  fn slack_rejects_internal_host() {
    let config =
      serde_json::json!({ "webhook_url": "https://localhost/services/x" });
    assert!(
      NotificationChannel::from_stored(nt("slack"), &config, KEY).is_err()
    );
  }

  #[test]
  fn unknown_type_is_rejected() {
    assert!("carrier_pigeon".parse::<NotificationType>().is_err());
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
