use crate::{
  EmailConfig,
  GitHubOAuthConfig,
  GithubActionsPoolConfig,
  NotificationsConfig,
  S3CacheConfig,
  SlackNotificationConfig,
};

impl std::fmt::Debug for GithubActionsPoolConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("GithubActionsPoolConfig")
      .field("workflow_repository", &self.workflow_repository)
      .field("workflow", &self.workflow)
      .field("ref_name", &self.ref_name)
      .field("token", &self.token.as_ref().map(|_| "[REDACTED]"))
      .field("token_file", &self.token_file)
      .field("runner_url", &self.runner_url)
      .field("oidc_audience", &self.oidc_audience)
      .field("agent_binary_url", &self.agent_binary_url)
      .finish()
  }
}
impl std::fmt::Debug for GitHubOAuthConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("GitHubOAuthConfig")
      .field("client_id", &self.client_id)
      .field("client_secret", &"[REDACTED]")
      .field("client_secret_file", &self.client_secret_file)
      .field("redirect_uri", &self.redirect_uri)
      .finish()
  }
}
impl std::fmt::Debug for NotificationsConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("NotificationsConfig")
      .field(
        "webhook_url",
        &self.webhook_url.as_ref().map(|_| "[REDACTED]"),
      )
      .field("webhook_url_file", &self.webhook_url_file)
      .field(
        "github_token",
        &self.github_token.as_ref().map(|_| "[REDACTED]"),
      )
      .field("github_token_file", &self.github_token_file)
      .field("gitea_url", &self.gitea_url)
      .field(
        "gitea_token",
        &self.gitea_token.as_ref().map(|_| "[REDACTED]"),
      )
      .field("gitea_token_file", &self.gitea_token_file)
      .field("gitlab_url", &self.gitlab_url)
      .field(
        "gitlab_token",
        &self.gitlab_token.as_ref().map(|_| "[REDACTED]"),
      )
      .field("gitlab_token_file", &self.gitlab_token_file)
      .field("email", &self.email)
      .field("alerts", &self.alerts)
      .field("slack", &self.slack)
      .field("enable_retry_queue", &self.enable_retry_queue)
      .field("max_retry_attempts", &self.max_retry_attempts)
      .field("retention_days", &self.retention_days)
      .field("retry_poll_interval", &self.retry_poll_interval)
      .finish()
  }
}
impl std::fmt::Debug for SlackNotificationConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("SlackNotificationConfig")
      .field("webhook_url", &"[REDACTED]")
      .field("webhook_url_file", &self.webhook_url_file)
      .field("on_failure_only", &self.on_failure_only)
      .finish()
  }
}

impl std::fmt::Debug for EmailConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("EmailConfig")
      .field("smtp_host", &self.smtp_host)
      .field("smtp_port", &self.smtp_port)
      .field("smtp_user", &self.smtp_user)
      .field(
        "smtp_password",
        &self.smtp_password.as_ref().map(|_| "[REDACTED]"),
      )
      .field("smtp_password_file", &self.smtp_password_file)
      .field("from_address", &self.from_address)
      .field("to_addresses", &self.to_addresses)
      .field("tls", &self.tls)
      .field("on_failure_only", &self.on_failure_only)
      .finish()
  }
}
impl std::fmt::Debug for S3CacheConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("S3CacheConfig")
      .field("region", &self.region)
      .field("prefix", &self.prefix)
      .field("access_key_id", &self.access_key_id)
      .field(
        "secret_access_key",
        &self.secret_access_key.as_ref().map(|_| "[REDACTED]"),
      )
      .field("secret_access_key_file", &self.secret_access_key_file)
      .field(
        "session_token",
        &self.session_token.as_ref().map(|_| "[REDACTED]"),
      )
      .field("session_token_file", &self.session_token_file)
      .field("endpoint_url", &self.endpoint_url)
      .field("use_path_style", &self.use_path_style)
      .finish()
  }
}

/// Declarative project/jobset/api-key/user definitions.
const SECRET_KEYS: &[&str] = &[
  "api_key",
  "client_secret",
  "gitea_token",
  "github_token",
  "gitlab_token",
  "secret_access_key",
  "session_token",
  "smtp_password",
  "token",
  "webhook_secret_encryption_key",
  "webhook_url",
];

/// Replace secret values in a serialized config with `"***"`.
pub fn redact_secrets(value: &mut toml::Value) {
  match value {
    toml::Value::Table(table) => {
      for (key, val) in table.iter_mut() {
        if SECRET_KEYS.contains(&key.as_str()) {
          *val = toml::Value::String("***".into());
        } else if let toml::Value::String(s) = val {
          if s.starts_with("postgresql://") || s.starts_with("postgres://") {
            *s = "***".into();
          }
        } else {
          redact_secrets(val);
        }
      }
    },
    toml::Value::Array(arr) => {
      for item in arr {
        redact_secrets(item);
      }
    },
    _ => {},
  }
}
