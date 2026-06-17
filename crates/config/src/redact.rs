use crate::{
  EmailConfig,
  GitHubOAuthConfig,
  GithubActionsPoolConfig,
  NotificationsConfig,
  S3CacheConfig,
  SlackNotificationConfig,
};

macro_rules! redact_debug_field {
  ($debug:ident, $self:ident, $field:ident,visible) => {
    $debug.field(stringify!($field), &$self.$field);
  };
  ($debug:ident, $self:ident, $field:ident,secret) => {
    $debug.field(stringify!($field), &"[REDACTED]");
  };
  ($debug:ident, $self:ident, $field:ident,optional_secret) => {
    $debug.field(
      stringify!($field),
      &$self.$field.as_ref().map(|_| "[REDACTED]"),
    );
  };
}

macro_rules! redact_debug {
  ($ty:ident { $($field:ident: $kind:ident),* $(,)? }) => {
    impl std::fmt::Debug for $ty {
      fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let mut debug = f.debug_struct(stringify!($ty));
        $(
          redact_debug_field!(debug, self, $field, $kind);
        )*
        debug.finish()
      }
    }
  };
}

redact_debug!(GithubActionsPoolConfig {
  workflow_repository: visible,
  workflow:            visible,
  ref_name:            visible,
  token:               optional_secret,
  token_file:          visible,
  runner_url:          visible,
  oidc_audience:       visible,
  agent_binary_url:    visible,
});

redact_debug!(GitHubOAuthConfig {
  client_id:          visible,
  client_secret:      secret,
  client_secret_file: visible,
  redirect_uri:       visible,
});

redact_debug!(NotificationsConfig {
  webhook_url:         optional_secret,
  webhook_url_file:    visible,
  github_token:        optional_secret,
  github_token_file:   visible,
  gitea_url:           visible,
  gitea_token:         optional_secret,
  gitea_token_file:    visible,
  gitlab_url:          visible,
  gitlab_token:        optional_secret,
  gitlab_token_file:   visible,
  email:               visible,
  alerts:              visible,
  slack:               visible,
  enable_retry_queue:  visible,
  max_retry_attempts:  visible,
  retention_days:      visible,
  retry_poll_interval: visible,
});

redact_debug!(SlackNotificationConfig {
  webhook_url:      secret,
  webhook_url_file: visible,
  on_failure_only:  visible,
});

redact_debug!(EmailConfig {
  smtp_host:          visible,
  smtp_port:          visible,
  smtp_user:          visible,
  smtp_password:      optional_secret,
  smtp_password_file: visible,
  from_address:       visible,
  to_addresses:       visible,
  tls:                visible,
  on_failure_only:    visible,
});

redact_debug!(S3CacheConfig {
  region:                 visible,
  prefix:                 visible,
  access_key_id:          visible,
  secret_access_key:      optional_secret,
  secret_access_key_file: visible,
  session_token:          optional_secret,
  session_token_file:     visible,
  endpoint_url:           visible,
  use_path_style:         visible,
});

/// Declarative project/jobset/api-key/user definitions.
/// Keep this list in sync with the `redact_debug!` secret fields above.
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
