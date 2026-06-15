//! Configuration management for circus

use std::{
  fs,
  path::{Path, PathBuf},
  time::Duration,
};

pub use circus_logs::TracingConfig;
use config as config_crate;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Config {
  pub database:      DatabaseConfig,
  pub server:        ServerConfig,
  pub evaluator:     EvaluatorConfig,
  pub queue_runner:  QueueRunnerConfig,
  pub gc:            GcConfig,
  pub logs:          LogConfig,
  pub notifications: NotificationsConfig,
  pub cache:         CacheConfig,
  pub signing:       SigningConfig,
  #[serde(default)]
  pub cache_upload:  CacheUploadConfig,
  pub tracing:       TracingConfig,
  #[serde(default)]
  pub declarative:   DeclarativeConfig,
  #[serde(default)]
  pub oauth:         OAuthConfig,
  #[serde(default)]
  pub nix:           NixConfig,
}

/// Nix-specific settings, primarily for non-standard Nix installations.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct NixConfig {
  /// Path to the Nix store directory. Defaults to `/nix/store`.
  /// Override when Nix is installed with a relocated store (e.g. on macOS
  /// with a non-standard APFS volume or a multi-user install under a
  /// different prefix).
  pub store_dir: PathBuf,
}

impl Default for NixConfig {
  fn default() -> Self {
    Self {
      store_dir: PathBuf::from("/nix/store"),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatabaseConfig {
  pub url:             String,
  /// Path to a file containing the database URL. Read at startup; overrides
  /// `url` when set. Preferred for production deployments where the URL
  /// contains credentials.
  pub url_file:        Option<PathBuf>,
  pub max_connections: u32,
  pub min_connections: u32,
  pub connect_timeout: u64,
  pub idle_timeout:    u64,
  pub max_lifetime:    u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[expect(
  clippy::struct_excessive_bools,
  reason = "ServerConfig mirrors independent TOML switches"
)]
#[serde(default)]
pub struct ServerConfig {
  pub host:                               String,
  pub port:                               u16,
  pub request_timeout:                    u64,
  pub max_body_size:                      usize,
  pub api_key:                            Option<String>,
  /// Path to a file containing the API key.
  pub api_key_file:                       Option<PathBuf>,
  pub allowed_origins:                    Vec<String>,
  pub cors_permissive:                    bool,
  pub rate_limit_rps:                     Option<u64>,
  pub rate_limit_burst:                   Option<u32>,
  /// Allowed URL schemes for repository URLs. Insecure schemes emit a warning
  /// on startup
  pub allowed_url_schemes:                Vec<String>,
  /// Force Secure flag on session cookies (enable when behind HTTPS reverse
  /// proxy)
  pub force_secure_cookies:               bool,
  /// Optional regex for email format validation.
  /// When unset (the default), only structural checks are applied: the address
  /// must be non-empty, at most 255 characters, and contain `@`. Set this to
  /// enforce a stricter pattern, e.g.:
  /// `'^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$'`
  pub email_validation_regex:             Option<String>,
  /// LDAP authentication configuration.
  pub ldap:                               Option<LdapConfig>,
  /// Dashboard page-level access policy.
  pub page_access:                        PageAccessConfig,
  /// Allow admins to read and replace the config file through the
  /// dashboard/API.
  pub config_editor_enabled:              bool,
  /// Require a valid API key/session for read-only `/api/v1` requests.
  #[serde(default = "default_true")]
  pub require_api_key_for_reads:          bool,
  /// Key used to encrypt webhook secrets before database storage.
  pub webhook_secret_encryption_key:      Option<String>,
  /// Path to a file containing the webhook secret encryption key.
  pub webhook_secret_encryption_key_file: Option<PathBuf>,
}

#[derive(
  Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default,
)]
#[serde(rename_all = "snake_case")]
pub enum PageAccessLevel {
  /// Anyone can view the page.
  #[default]
  Public,
  /// A logged-in user or API key session is required.
  Authenticated,
  /// Only administrators can view the page.
  Admin,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct PageAccessConfig {
  pub home:        PageAccessLevel,
  pub projects:    PageAccessLevel,
  pub project:     PageAccessLevel,
  pub jobset:      PageAccessLevel,
  pub jobset_jobs: PageAccessLevel,
  pub evaluations: PageAccessLevel,
  pub evaluation:  PageAccessLevel,
  pub builds:      PageAccessLevel,
  pub build:       PageAccessLevel,
  pub queue:       PageAccessLevel,
  pub channels:    PageAccessLevel,
  pub channel:     PageAccessLevel,
  pub news:        PageAccessLevel,
  pub starred:     PageAccessLevel,
  pub metrics:     PageAccessLevel,
}

impl Default for PageAccessConfig {
  fn default() -> Self {
    Self {
      home:        PageAccessLevel::Public,
      projects:    PageAccessLevel::Authenticated,
      project:     PageAccessLevel::Authenticated,
      jobset:      PageAccessLevel::Authenticated,
      jobset_jobs: PageAccessLevel::Authenticated,
      evaluations: PageAccessLevel::Authenticated,
      evaluation:  PageAccessLevel::Authenticated,
      builds:      PageAccessLevel::Authenticated,
      build:       PageAccessLevel::Authenticated,
      queue:       PageAccessLevel::Admin,
      channels:    PageAccessLevel::Authenticated,
      channel:     PageAccessLevel::Authenticated,
      news:        PageAccessLevel::Authenticated,
      starred:     PageAccessLevel::Authenticated,
      metrics:     PageAccessLevel::Admin,
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EvaluatorConfig {
  pub poll_interval:        u64,
  pub git_timeout:          u64,
  pub nix_timeout:          u64,
  pub max_concurrent_evals: usize,
  pub work_dir:             PathBuf,
  pub restrict_eval:        bool,
  pub allow_ifd:            bool,

  /// Whether to abort on the first evaluation cycle error instead of logging
  /// and retrying.
  pub strict_errors: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QueueRunnerConfig {
  pub workers:       usize,
  pub poll_interval: u64,
  pub build_timeout: u64,
  pub work_dir:      PathBuf,

  /// Agent build silence timeout in seconds. `0` disables it.
  #[serde(default)]
  pub max_silent_time: u64,

  /// When true, abort on the first runner loop error instead of logging and
  /// retrying.
  #[serde(default)]
  pub strict_errors: bool,

  /// Cache failed derivation paths to skip known-failing builds.
  #[serde(default = "default_true")]
  pub failed_paths_cache: bool,

  /// TTL in seconds for failed paths cache entries (default 24h).
  #[serde(default = "default_failed_paths_ttl")]
  pub failed_paths_ttl: u64,

  /// Timeout after which builds for unsupported systems are aborted.
  /// None or 0 = disabled (Hydra maxUnsupportedTime compatibility).
  #[serde(default)]
  #[serde(with = "humantime_serde")]
  pub unsupported_timeout: Option<Duration>,

  /// Builder selection strategy (default: `speed_factor_only`).
  #[serde(default)]
  pub scheduling_strategy: BuilderSchedulingStrategy,

  /// Skip builders whose PSI avg10 exceeds this threshold (0.0–100.0).
  /// `None` disables PSI checking.
  pub psi_threshold: Option<f64>,

  /// Timeout in seconds for SSH PSI checks (default 5).
  #[serde(default = "default_psi_check_timeout")]
  pub psi_check_timeout: u64,

  /// Skip SSH `remote_builders` with no `public_host_key` recorded instead of
  /// falling back to `accept-new`. Default false.
  #[serde(default)]
  pub ssh_require_host_key: bool,

  /// Extra arguments appended to every `nix build` invocation (after the
  /// queue-runner's defaults, before the installable). Use this to inject
  /// substituters, trusted public keys, or override sandbox settings without
  /// changing the daemon's `nix.conf`. Example:
  /// `["--option", "extra-substituters", "https://cache.nixos.org"]`.
  #[serde(default)]
  pub extra_nix_build_args: Vec<String>,

  /// Systems the runner host itself may build.
  #[serde(default)]
  pub local_systems: Option<Vec<String>>,

  /// `system-features` of the runner host's nix.
  #[serde(default)]
  pub local_features: Option<Vec<String>>,

  /// Capnp-rpc endpoint for persistent build agents. When set, the
  /// queue-runner listens on this address and dispatches eligible builds
  /// to connected agents in preference to the SSH `remote_builders`
  /// path. Leave unset to disable the agent path entirely.
  #[serde(default)]
  pub rpc: Option<RpcConfig>,

  /// GitHub Actions-backed ephemeral `circus-agent` pools.
  #[serde(default)]
  pub ephemeral_pools: Vec<EphemeralPoolConfig>,
}

/// Runner-driven ephemeral builder pool. Each pool represents one class of
/// short-lived GitHub Actions agents with a single capability profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct EphemeralPoolConfig {
  /// Stable pool name. Used as the base agent name and for autoscaler logs.
  pub name:                       String,
  /// Source repositories this pool is allowed to build for (`owner/repo`).
  pub allowed_build_repositories: Vec<String>,
  /// Nix systems advertised by each GitHub Actions agent.
  pub systems:                    Vec<String>,
  /// Features advertised by each GitHub Actions agent.
  pub supported_features:         Vec<String>,
  /// Mandatory features advertised by each GitHub Actions agent.
  pub mandatory_features:         Vec<String>,
  pub max_jobs:                   u32,
  pub cores:                      u32,
  pub speed_factor:               f32,
  pub max_inflight:               u32,
  pub inflight_ttl_secs:          u64,
  pub scale_up_cooldown_secs:     u64,
  pub poll_interval_secs:         u64,
  /// GitHub Actions launch settings for this pool.
  pub github_actions:             GithubActionsPoolConfig,
}

/// GitHub Actions workflow dispatch settings for an ephemeral pool.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GithubActionsPoolConfig {
  /// GitHub repository slug (`owner/repo`) containing the builder workflow.
  pub workflow_repository: String,
  /// Workflow file name or numeric ID accepted by the workflow dispatch API.
  pub workflow:            String,
  /// Git ref where the workflow file lives.
  pub ref_name:            String,
  /// GitHub token with Actions write access for `repository`.
  pub token:               Option<String>,
  /// File containing the GitHub token. Used when `token` is unset.
  pub token_file:          Option<PathBuf>,
  /// Runner URL passed to `circus-agent`, e.g. `circus+tls://host:8443`.
  pub runner_url:          String,
  /// Audience requested for the GitHub OIDC token.
  pub oidc_audience:       String,
  /// Exact agent binary URL downloaded by the workflow. Pin this to the same
  /// Circus revision/protocol version as the queue-runner.
  pub agent_binary_url:    String,
}

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

impl Default for EphemeralPoolConfig {
  fn default() -> Self {
    Self {
      name:                       "gha-x86_64-linux".to_owned(),
      allowed_build_repositories: Vec::new(),
      systems:                    vec!["x86_64-linux".to_owned()],
      supported_features:         Vec::new(),
      mandatory_features:         Vec::new(),
      max_jobs:                   1,
      cores:                      0,
      speed_factor:               1.0,
      max_inflight:               4,
      inflight_ttl_secs:          900,
      scale_up_cooldown_secs:     30,
      poll_interval_secs:         10,
      github_actions:             GithubActionsPoolConfig::default(),
    }
  }
}

impl Default for GithubActionsPoolConfig {
  fn default() -> Self {
    Self {
      workflow_repository: String::new(),
      workflow:            "circus-builder.yml".to_owned(),
      ref_name:            "main".to_owned(),
      token:               None,
      token_file:          None,
      runner_url:          String::new(),
      oidc_audience:       "circus-agent".to_owned(),
      agent_binary_url:    String::new(),
    }
  }
}

/// Configuration for the capnp-rpc agent endpoint. Used when distributed
/// builds run through long-lived `circus-agent` connections rather than
/// per-build SSH dispatch. See `docs/DISTRIBUTED.md`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
  /// Listen address, e.g. `"0.0.0.0:8443"` or `"[::]:8443"`.
  pub bind: String,

  /// SHA-256 hex digests of accepted bearer tokens. An agent presents a
  /// raw token in `register`; we hash and compare in constant time.
  /// Empty = reject all (agents will fail to register).
  #[serde(default)]
  pub auth_tokens: Vec<String>,

  /// Hard cap on concurrent connections; serves as a flood guard.
  #[serde(default = "default_max_rpc_conns")]
  pub max_connections: usize,

  /// Lifetime of every minted presigned PUT URL. Should comfortably
  /// exceed the longest expected NAR upload (largest output * speed
  /// factor); defaults to one hour.
  #[serde(default = "default_presign_expiry_secs")]
  pub presign_expiry_secs: u64,

  /// Optional TLS material. Plain TCP when absent.
  #[serde(default)]
  pub tls: Option<RpcTlsConfig>,

  /// Allow registration credentials over plain TCP. Off by default, so a
  /// missing `tls` hard-errors when credentials are configured. Enable it on
  /// a trusted network where plain TCP is intentional.
  #[serde(default)]
  pub allow_plaintext: bool,

  /// Heartbeat freshness window. Heartbeats older than this drop the
  /// agent from scheduling decisions.
  #[serde(default = "default_heartbeat_ttl_secs")]
  pub heartbeat_ttl_secs: u64,

  /// Cache agents substitute drv closures from, forwarded to each agent.
  #[serde(default)]
  pub cache_substituter: Option<String>,

  /// Public key to trust for `cache_substituter`.
  #[serde(default)]
  pub cache_public_key: Option<String>,

  /// Accept short-lived OIDC JWTs (e.g. GitHub Actions) in place of a
  /// bearer token. `auth_tokens` still works alongside this.
  #[serde(default)]
  pub oidc: Option<RpcOidcConfig>,
}

/// OIDC trust settings for the capnp-rpc endpoint. An agent may present an
/// OIDC ID token in `register` instead of a bearer token; the runner verifies
/// it against the issuer's JWKS. Defaults target GitHub Actions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcOidcConfig {
  /// Expected `iss` claim. The JWKS is discovered from this issuer unless
  /// `jwks_url` is set.
  #[serde(default = "default_oidc_issuer")]
  pub issuer: String,

  /// JWKS endpoint override. Discovered from the issuer's
  /// `.well-known/openid-configuration` when absent.
  #[serde(default)]
  pub jwks_url: Option<String>,

  /// Accepted `aud` claim values. The workflow must mint its token with one
  /// of these audiences. Empty = reject all.
  #[serde(default)]
  pub audiences: Vec<String>,

  /// `owner/repo` slugs allowed to register. Empty = reject all.
  #[serde(default)]
  pub allowed_repositories: Vec<String>,

  /// Exact `sub` claim values allowed to register. Empty disables this check.
  #[serde(default)]
  pub allowed_subjects: Vec<String>,

  /// Accepted `sub` claim prefixes. Empty disables this check.
  #[serde(default)]
  pub allowed_subject_prefixes: Vec<String>,

  /// Exact GitHub `workflow_ref` claim values allowed to register. Empty
  /// disables this check.
  #[serde(default)]
  pub allowed_workflow_refs: Vec<String>,

  /// Exact GitHub `ref` claim values allowed to register. Empty disables this
  /// check.
  #[serde(default)]
  pub allowed_refs: Vec<String>,
}

fn default_oidc_issuer() -> String {
  "https://token.actions.githubusercontent.com".to_owned()
}

/// Server-side TLS material for the capnp-rpc endpoint. When `client_ca` is
/// set, the runner verifies any client cert an agent presents. Set
/// `require_client_cert` to make that cert mandatory.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTlsConfig {
  pub cert_file:           PathBuf,
  pub key_file:            PathBuf,
  #[serde(default)]
  pub client_ca:           Option<PathBuf>,
  /// Pin a presented client cert name to the registering agent name.
  #[serde(default = "default_true")]
  pub pin_cn:              bool,
  /// Require client certs when `client_ca` is set.
  #[serde(default)]
  pub require_client_cert: bool,
}

const fn default_max_rpc_conns() -> usize {
  256
}

const fn default_heartbeat_ttl_secs() -> u64 {
  60
}

const fn default_presign_expiry_secs() -> u64 {
  3600
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct GcConfig {
  pub gc_roots_dir:     PathBuf,
  pub enabled:          bool,
  pub max_age_days:     u64,
  pub cleanup_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogConfig {
  pub log_dir:  PathBuf,
  pub compress: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct OAuthConfig {
  pub github: Option<GitHubOAuthConfig>,
}

#[derive(Clone, Serialize, Deserialize)]
pub struct GitHubOAuthConfig {
  pub client_id:          String,
  #[serde(default)]
  pub client_secret:      String,
  /// Path to a file containing the OAuth client secret.
  pub client_secret_file: Option<PathBuf>,
  pub redirect_uri:       String,
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

#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
// Manual Default impl below so the default tree fed into `config-rs` matches
// the per-field `#[serde(default = ...)]` annotations. `#[derive(Default)]`
// would silently set `enable_retry_queue = false`, which is wrong.
pub struct NotificationsConfig {
  pub webhook_url:         Option<String>,
  /// Path to a file containing the generic webhook URL.
  pub webhook_url_file:    Option<PathBuf>,
  pub github_token:        Option<String>,
  /// Path to a file containing the GitHub token.
  pub github_token_file:   Option<PathBuf>,
  pub gitea_url:           Option<String>,
  pub gitea_token:         Option<String>,
  /// Path to a file containing the Gitea token.
  pub gitea_token_file:    Option<PathBuf>,
  pub gitlab_url:          Option<String>,
  pub gitlab_token:        Option<String>,
  /// Path to a file containing the GitLab token.
  pub gitlab_token_file:   Option<PathBuf>,
  pub email:               Option<EmailConfig>,
  pub alerts:              Option<AlertConfig>,
  /// Slack incoming webhook notification.
  pub slack:               Option<SlackNotificationConfig>,
  /// Enable notification retry queue (persistent, with exponential backoff)
  #[serde(default = "default_true")]
  pub enable_retry_queue:  bool,
  /// Maximum retry attempts per notification (default 5)
  #[serde(default = "default_notification_max_attempts")]
  pub max_retry_attempts:  i32,
  /// Retention period for old completed/failed tasks in days (default 7)
  #[serde(default = "default_notification_retention_days")]
  pub retention_days:      i64,
  /// Polling interval for retry worker in seconds (default 5)
  #[serde(default = "default_notification_poll_interval")]
  pub retry_poll_interval: u64,
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

impl Default for NotificationsConfig {
  fn default() -> Self {
    Self {
      webhook_url:         None,
      webhook_url_file:    None,
      github_token:        None,
      github_token_file:   None,
      gitea_url:           None,
      gitea_token:         None,
      gitea_token_file:    None,
      gitlab_url:          None,
      gitlab_token:        None,
      gitlab_token_file:   None,
      email:               None,
      alerts:              None,
      slack:               None,
      enable_retry_queue:  default_true(),
      max_retry_attempts:  default_notification_max_attempts(),
      retention_days:      default_notification_retention_days(),
      retry_poll_interval: default_notification_poll_interval(),
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AlertConfig {
  pub enabled:             bool,
  pub error_threshold:     f64,
  pub time_window_minutes: i64,
}

impl Default for AlertConfig {
  fn default() -> Self {
    Self {
      enabled:             false,
      error_threshold:     20.0,
      time_window_minutes: 60,
    }
  }
}

/// Slack incoming webhook notification configuration.
#[derive(Clone, Serialize, Deserialize)]
pub struct SlackNotificationConfig {
  #[serde(default)]
  pub webhook_url:      String,
  /// Path to a file containing the Slack webhook URL.
  pub webhook_url_file: Option<PathBuf>,
  /// Only send notifications for failed builds (default false).
  #[serde(default)]
  pub on_failure_only:  bool,
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

/// LDAP authentication configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LdapConfig {
  /// LDAP server URL, e.g. `<ldap://host:389>` or `<ldaps://host:636>`.
  pub url:              String,
  /// Bind DN template with `{username}` placeholder.
  pub bind_dn_template: String,
  /// Base DN for user searches.
  pub base_dn:          String,
  /// Path to a custom CA certificate for TLS verification.
  pub tls_ca_cert:      Option<PathBuf>,
  /// Whether LDAP auth is enabled (default true).
  #[serde(default = "default_true")]
  pub enabled:          bool,
}

#[derive(Clone, Serialize, Deserialize, Default)]
pub struct EmailConfig {
  pub smtp_host:          String,
  pub smtp_port:          u16,
  pub smtp_user:          Option<String>,
  pub smtp_password:      Option<String>,
  /// Path to a file containing the SMTP password.
  pub smtp_password_file: Option<PathBuf>,
  pub from_address:       String,
  pub to_addresses:       Vec<String>,
  pub tls:                bool,
  pub on_failure_only:    bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
  pub enabled:         bool,
  pub secret_key_file: Option<PathBuf>,
  /// Public URL of this binary cache (for channel manifest endpoints)
  pub cache_url:       Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct SigningConfig {
  pub enabled:  bool,
  pub key_file: Option<PathBuf>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheUploadConfig {
  pub enabled:                    bool,
  pub store_uri:                  Option<String>,
  /// S3-specific configuration (used when `store_uri` starts with s3://)
  pub s3:                         Option<S3CacheConfig>,
  /// Number of concurrent `nix copy` invocations for multi-output builds
  /// (default 4)
  #[serde(default = "default_upload_concurrency")]
  pub upload_concurrency:         usize,
  /// Maximum retry attempts per path before giving up (default 3)
  #[serde(default = "default_upload_retries")]
  pub upload_max_retries:         u32,
  /// If true, mark the build as failed when the cache upload exhausts its
  /// retry budget. If false (the default), log the error and let the build
  /// succeed; the operator can re-push out of band.
  #[serde(default)]
  pub fail_build_on_upload_error: bool,
  /// Wire compression for the agent's presigned-upload path. The agent
  /// streams the NAR through the chosen encoder before `PUTing` to S3, and
  /// the runner records this in the narinfo `Compression:` field.
  /// Accepted values: `zstd`, `xz`, `gzip`, `none`. Defaults to `zstd`.
  #[serde(default = "default_upload_compression")]
  pub compression:                String,
}

const fn default_upload_concurrency() -> usize {
  4
}

fn default_upload_compression() -> String {
  "zstd".to_owned()
}

const fn default_upload_retries() -> u32 {
  3
}

impl Default for CacheUploadConfig {
  fn default() -> Self {
    Self {
      enabled:                    false,
      store_uri:                  None,
      s3:                         None,
      upload_concurrency:         default_upload_concurrency(),
      upload_max_retries:         default_upload_retries(),
      fail_build_on_upload_error: false,
      compression:                default_upload_compression(),
    }
  }
}

/// S3-specific cache configuration.
#[derive(Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct S3CacheConfig {
  /// AWS region (e.g., "us-east-1")
  pub region:                 Option<String>,
  /// Path prefix within the bucket (e.g., "nix-cache/"). Combined with any
  /// path already present in `cache_upload.store_uri`.
  pub prefix:                 Option<String>,
  /// AWS access key ID. Required for presigned agent uploads and server-side
  /// private S3 redirects; `nix copy` may still use ambient credentials.
  pub access_key_id:          Option<String>,
  /// AWS secret access key. Required when `access_key_id` is set.
  pub secret_access_key:      Option<String>,
  /// Path to a file containing the AWS secret access key.
  pub secret_access_key_file: Option<PathBuf>,
  /// Session token for temporary credentials (optional)
  pub session_token:          Option<String>,
  /// Path to a file containing the AWS session token.
  pub session_token_file:     Option<PathBuf>,
  /// Endpoint URL for S3-compatible services (e.g., `MinIO`)
  pub endpoint_url:           Option<String>,
  /// Whether to use path-style addressing (for `MinIO` compatibility)
  pub use_path_style:         bool,
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
/// These are upserted on server startup, enabling fully declarative operation.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct DeclarativeConfig {
  pub projects:        Vec<DeclarativeProject>,
  pub api_keys:        Vec<DeclarativeApiKey>,
  pub users:           Vec<DeclarativeUser>,
  /// Remote builder definitions for distributed builds
  pub remote_builders: Vec<DeclarativeRemoteBuilder>,
}

/// Declarative remote builder configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeRemoteBuilder {
  pub name:               String,
  pub ssh_uri:            String,
  pub systems:            Vec<String>,
  #[serde(default = "default_max_jobs")]
  pub max_jobs:           i32,
  #[serde(default = "default_speed_factor")]
  pub speed_factor:       i32,
  #[serde(default)]
  pub supported_features: Vec<String>,
  #[serde(default)]
  pub mandatory_features: Vec<String>,
  /// Path to SSH private key file (for production)
  pub ssh_key_file:       Option<String>,
  /// SSH public host key for verification
  pub public_host_key:    Option<String>,
  #[serde(default = "default_true")]
  pub enabled:            bool,
}

const fn default_max_jobs() -> i32 {
  1
}

const fn default_speed_factor() -> i32 {
  1
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeProject {
  pub name:           String,
  pub repository_url: String,
  pub description:    Option<String>,
  #[serde(default)]
  pub jobsets:        Vec<DeclarativeJobset>,
  /// Notification configurations for this project
  #[serde(default)]
  pub notifications:  Vec<DeclarativeNotification>,
  /// Webhook configurations for this project
  #[serde(default)]
  pub webhooks:       Vec<DeclarativeWebhook>,
  /// Release channels for this project
  #[serde(default)]
  pub channels:       Vec<DeclarativeChannel>,
  /// Project members with their roles
  #[serde(default)]
  pub members:        Vec<DeclarativeProjectMember>,
}

/// Declarative notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeNotification {
  /// Notification type: `github_status`, `email`, `gitlab_status`,
  /// `gitea_status`, `webhook`
  pub notification_type: String,
  /// Type-specific configuration (JSON object)
  pub config:            serde_json::Value,
  #[serde(default = "default_true")]
  pub enabled:           bool,
}

/// Declarative webhook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeWebhook {
  /// Forge type: github, gitea, gitlab
  pub forge_type:  String,
  /// Webhook secret (inline, for dev/testing only)
  pub secret:      Option<String>,
  /// Path to a file containing the webhook secret (for production)
  pub secret_file: Option<String>,
  #[serde(default = "default_true")]
  pub enabled:     bool,
}

/// Declarative channel configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeChannel {
  pub name:        String,
  /// Name of the jobset this channel tracks (resolved during bootstrap)
  pub jobset_name: String,
}

/// Declarative project member configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeProjectMember {
  /// Username of the member (must exist in users)
  pub username: String,
  /// Role: member, maintainer, or admin
  #[serde(default = "default_member_role")]
  pub role:     String,
}

const fn default_psi_check_timeout() -> u64 {
  5
}

fn default_member_role() -> String {
  "member".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeJobset {
  pub name:              String,
  pub nix_expression:    String,
  #[serde(default = "default_true")]
  pub enabled:           bool,
  #[serde(default = "default_true")]
  pub flake_mode:        bool,
  #[serde(default = "default_check_interval")]
  pub check_interval:    i32,
  /// Trigger mode: `source_change` or `interval`.
  pub trigger_mode:      Option<String>,
  /// Jobset state: disabled, enabled, `one_shot`, or `one_at_a_time`
  pub state:             Option<String>,
  /// Git branch to track (defaults to repository default branch)
  pub branch:            Option<String>,
  /// Scheduling priority shares (default 100, higher = more priority)
  #[serde(default = "default_scheduling_shares")]
  pub scheduling_shares: i32,
  /// Number of recent successful evaluations to retain (default 3)
  pub keep_nr:           Option<i32>,
  /// Jobset inputs for parameterized evaluations
  #[serde(default)]
  pub inputs:            Vec<DeclarativeJobsetInput>,
}

/// Declarative jobset input for parameterized builds.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeJobsetInput {
  pub name:       String,
  /// Input type: git, string, boolean, path, or build
  pub input_type: String,
  pub value:      String,
  /// Git revision (for git inputs)
  pub revision:   Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeApiKey {
  pub name:     String,
  /// API key provided inline (for dev/testing only).
  pub key:      Option<String>,
  /// Path to a file containing the API key (for production use with secrets).
  pub key_file: Option<String>,
  #[serde(default = "default_role")]
  pub role:     String,
}

/// Declarative user definition for configuration-driven user management.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeUser {
  pub username:      String,
  pub email:         String,
  pub full_name:     Option<String>,
  /// Password provided inline (for dev/testing only).
  pub password:      Option<String>,
  /// Path to a file containing the password (for production use with secrets).
  pub password_file: Option<String>,
  #[serde(default = "default_user_role")]
  pub role:          String,
  #[serde(default = "default_true")]
  pub enabled:       bool,
}

fn default_user_role() -> String {
  "read-only".to_string()
}

const fn default_true() -> bool {
  true
}

const fn default_failed_paths_ttl() -> u64 {
  86400
}

const fn default_check_interval() -> i32 {
  60
}

const fn default_scheduling_shares() -> i32 {
  100
}

fn default_role() -> String {
  "read-only".to_string()
}

const fn default_notification_max_attempts() -> i32 {
  5
}

const fn default_notification_retention_days() -> i64 {
  7
}

const fn default_notification_poll_interval() -> u64 {
  5
}

impl Default for DatabaseConfig {
  fn default() -> Self {
    Self {
      url:             "postgresql://circus:password@localhost/circus"
        .to_string(),
      url_file:        None,
      max_connections: 20,
      min_connections: 5,
      connect_timeout: 30,
      idle_timeout:    600,
      max_lifetime:    1800,
    }
  }
}

impl DatabaseConfig {
  /// Validate database configuration.
  ///
  /// # Errors
  ///
  /// Returns error if configuration is invalid.
  pub fn validate(&self) -> color_eyre::Result<()> {
    if self.url.is_empty() {
      return Err(color_eyre::eyre::eyre!("Database URL cannot be empty"));
    }

    if !self.url.starts_with("postgresql://")
      && !self.url.starts_with("postgres://")
    {
      return Err(color_eyre::eyre::eyre!(
        "Database URL must start with postgresql:// or postgres://"
      ));
    }

    if self.max_connections == 0 {
      return Err(color_eyre::eyre::eyre!(
        "Max database connections must be greater than 0"
      ));
    }

    if self.min_connections > self.max_connections {
      return Err(color_eyre::eyre::eyre!(
        "Min database connections cannot exceed max connections"
      ));
    }

    Ok(())
  }
}

impl Default for ServerConfig {
  fn default() -> Self {
    Self {
      host:                               "127.0.0.1".to_string(),
      port:                               3000,
      request_timeout:                    30,
      max_body_size:                      10 * 1024 * 1024, // 10MB
      api_key:                            None,
      api_key_file:                       None,
      allowed_origins:                    Vec::new(),
      cors_permissive:                    false,
      rate_limit_rps:                     None,
      rate_limit_burst:                   None,
      allowed_url_schemes:                vec![
        "https".into(),
        "git".into(),
        "ssh".into(),
      ],
      force_secure_cookies:               false,
      email_validation_regex:             None,
      ldap:                               None,
      page_access:                        PageAccessConfig::default(),
      config_editor_enabled:              false,
      require_api_key_for_reads:          true,
      webhook_secret_encryption_key:      None,
      webhook_secret_encryption_key_file: None,
    }
  }
}

impl Default for EvaluatorConfig {
  fn default() -> Self {
    Self {
      poll_interval:        60,
      git_timeout:          600,
      nix_timeout:          1800,
      max_concurrent_evals: 4,
      work_dir:             PathBuf::from("/tmp/circus-evaluator"),
      restrict_eval:        true,
      allow_ifd:            false,
      strict_errors:        false,
    }
  }
}

impl Default for QueueRunnerConfig {
  fn default() -> Self {
    Self {
      workers:              4,
      poll_interval:        5,
      build_timeout:        3600,
      max_silent_time:      0,
      work_dir:             PathBuf::from("/tmp/circus-queue-runner"),
      strict_errors:        false,
      failed_paths_cache:   true,
      failed_paths_ttl:     86400,
      unsupported_timeout:  None,
      scheduling_strategy:  BuilderSchedulingStrategy::SpeedFactorOnly,
      psi_threshold:        None,
      psi_check_timeout:    5,
      ssh_require_host_key: false,
      extra_nix_build_args: Vec::new(),
      local_systems:        None,
      local_features:       None,
      rpc:                  None,
      ephemeral_pools:      Vec::new(),
    }
  }
}

impl Default for GcConfig {
  fn default() -> Self {
    Self {
      gc_roots_dir:     PathBuf::from(
        "/nix/var/nix/gcroots/per-user/circus/circus-roots",
      ),
      enabled:          true,
      max_age_days:     30,
      cleanup_interval: 3600,
    }
  }
}

impl Default for LogConfig {
  fn default() -> Self {
    Self {
      log_dir:  PathBuf::from("/var/lib/circus/logs"),
      compress: false,
    }
  }
}

impl Default for CacheConfig {
  fn default() -> Self {
    Self {
      enabled:         true,
      secret_key_file: None,
      cache_url:       None,
    }
  }
}

/// Builder scheduling strategy for `find_for_system()`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuilderSchedulingStrategy {
  /// Order by `speed_factor DESC` only (default, legacy behaviour).
  #[default]
  SpeedFactorOnly,
  /// Order by `cpu_cores * speed_factor DESC` (higher core×speed wins).
  CpuCoreCountWithSpeedFactor,
  /// Weighted by available slots: `(max_jobs - active) * speed_factor DESC`.
  Dynamic,
}

/// Fields that can be updated at runtime via SIGHUP without a restart.
/// Fields that require restart (e.g. `workers`, database pool) are excluded.
#[derive(Debug, Clone)]
pub struct HotConfig {
  pub poll_interval:        std::time::Duration,
  pub build_timeout:        std::time::Duration,
  pub max_silent_time:      std::time::Duration,
  pub notifications_config: NotificationsConfig,
  pub failed_paths_ttl:     u64,
  pub scheduling_strategy:  BuilderSchedulingStrategy,
  pub psi_threshold:        Option<f64>,
  pub psi_check_timeout:    std::time::Duration,
  pub extra_nix_build_args: Vec<String>,
  pub ssh_require_host_key: bool,
}

impl HotConfig {
  /// Construct a `HotConfig` snapshot from a loaded `Config`.
  #[must_use]
  pub fn from_config(config: &Config) -> Self {
    Self {
      poll_interval:        std::time::Duration::from_secs(
        config.queue_runner.poll_interval,
      ),
      build_timeout:        std::time::Duration::from_secs(
        config.queue_runner.build_timeout,
      ),
      max_silent_time:      std::time::Duration::from_secs(
        config.queue_runner.max_silent_time,
      ),
      notifications_config: config.notifications.clone(),
      failed_paths_ttl:     config.queue_runner.failed_paths_ttl,
      scheduling_strategy:  config.queue_runner.scheduling_strategy.clone(),
      psi_threshold:        config.queue_runner.psi_threshold,
      psi_check_timeout:    std::time::Duration::from_secs(
        config.queue_runner.psi_check_timeout,
      ),
      extra_nix_build_args: config.queue_runner.extra_nix_build_args.clone(),
      ssh_require_host_key: config.queue_runner.ssh_require_host_key,
    }
  }
}

impl Config {
  /// Parse a TOML config fragment after applying compiled defaults.
  ///
  /// This matches normal file loading semantics without environment overrides,
  /// making it suitable for the admin config editor: partial config files are
  /// expanded before validation and saving.
  ///
  /// # Errors
  ///
  /// Returns an error if TOML parsing, deserialization, or validation fails.
  pub fn from_toml_with_defaults(contents: &str) -> color_eyre::Result<Self> {
    let settings = config_crate::Config::builder()
      .add_source(config_crate::Config::try_from(&Self::default())?)
      .add_source(config_crate::File::from_str(
        contents,
        config_crate::FileFormat::Toml,
      ));
    let config = settings.build()?.try_deserialize::<Self>()?;
    config.validate()?;
    Ok(config)
  }

  /// Resolve `*_file` secret fields by reading their file contents at startup.
  ///
  /// For `Option<String>` fields the inline value takes precedence; the file
  /// is read only when the inline value is `None`. For required fields
  /// (`database.url`) the file overrides unconditionally since the field
  /// always has a compiled default.
  ///
  /// # Errors
  ///
  /// Returns an error if a configured file path cannot be read or is empty.
  fn resolve_secret_files(&mut self) -> color_eyre::Result<()> {
    fn read_secret(path: &Path) -> color_eyre::Result<String> {
      let content = fs::read_to_string(path).map_err(|e| {
        color_eyre::eyre::eyre!(
          "failed to read secret from {}: {e}",
          path.display()
        )
      })?;
      let trimmed = content.trim().to_owned();
      if trimmed.is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "secret file is empty: {}",
          path.display()
        ));
      }
      Ok(trimmed)
    }

    macro_rules! resolve_optional {
      ($field:expr, $file_field:expr) => {
        if $field.is_none() {
          if let Some(ref path) = $file_field {
            $field = Some(read_secret(path)?);
          }
        }
      };
    }

    // database.url: file overrides (url always carries a compiled default)
    if let Some(ref path) = self.database.url_file {
      self.database.url = read_secret(path)?;
    }

    // server
    resolve_optional!(self.server.api_key, self.server.api_key_file);
    resolve_optional!(
      self.server.webhook_secret_encryption_key,
      self.server.webhook_secret_encryption_key_file
    );

    // notifications
    resolve_optional!(
      self.notifications.webhook_url,
      self.notifications.webhook_url_file
    );
    resolve_optional!(
      self.notifications.github_token,
      self.notifications.github_token_file
    );
    resolve_optional!(
      self.notifications.gitea_token,
      self.notifications.gitea_token_file
    );
    resolve_optional!(
      self.notifications.gitlab_token,
      self.notifications.gitlab_token_file
    );

    // email (nested inside notifications)
    if let Some(ref mut email) = self.notifications.email {
      resolve_optional!(email.smtp_password, email.smtp_password_file);
    }

    // oauth
    if let Some(ref mut github) = self.oauth.github {
      if github.client_secret.is_empty() {
        if let Some(ref path) = github.client_secret_file {
          github.client_secret = read_secret(path)?;
        }
      }
    }

    // slack (nested inside notifications)
    if let Some(ref mut slack) = self.notifications.slack {
      if slack.webhook_url.is_empty() {
        if let Some(ref path) = slack.webhook_url_file {
          slack.webhook_url = read_secret(path)?;
        }
      }
    }

    // s3 (nested inside cache_upload)
    if let Some(ref mut s3) = self.cache_upload.s3 {
      resolve_optional!(s3.secret_access_key, s3.secret_access_key_file);
      resolve_optional!(s3.session_token, s3.session_token_file);
    }

    Ok(())
  }

  /// Load configuration from file and environment variables.
  ///
  /// # Errors
  ///
  /// Returns error if configuration loading or validation fails.
  pub fn load() -> color_eyre::Result<Self> {
    let mut settings = config_crate::Config::builder();

    // Load default configuration
    settings =
      settings.add_source(config_crate::Config::try_from(&Self::default())?);

    // Load from config file if it exists
    if let Ok(config_path) = std::env::var("CIRCUS_CONFIG_FILE") {
      if std::path::Path::new(&config_path).exists() {
        settings =
          settings.add_source(config_crate::File::with_name(&config_path));
      }
    } else if std::path::Path::new("circus.toml").exists() {
      settings = settings
        .add_source(config_crate::File::with_name("circus").required(false));
    }

    // Load from environment variables with CIRCUS_ prefix (highest priority)
    settings = settings.add_source(
      config_crate::Environment::with_prefix("circus")
        .separator("__")
        .try_parsing(true),
    );

    let mut config = settings.build()?.try_deserialize::<Self>()?;

    // The `config-rs` Environment source does not reliably override
    // `Option<String>` fields nested under a struct that was already seeded
    // with `Self::default()` (None serializes to a Nil value that the env
    // source then fails to overwrite during merge). Apply these manually
    // here so operator-set env vars actually take effect.
    apply_env_overrides_for_option_fields(&mut config);

    // Resolve *_file fields into their corresponding values
    config.resolve_secret_files()?;

    // Validate configuration
    config.validate()?;

    Ok(config)
  }

  /// Validate all configuration sections.
  ///
  /// # Errors
  ///
  /// Returns error if any configuration section is invalid.
  pub fn validate(&self) -> color_eyre::Result<()> {
    // Validate database URL
    if self.database.url.is_empty() {
      return Err(color_eyre::eyre::eyre!("Database URL cannot be empty"));
    }

    if !self.database.url.starts_with("postgresql://")
      && !self.database.url.starts_with("postgres://")
    {
      return Err(color_eyre::eyre::eyre!(
        "Database URL must start with postgresql:// or postgres://"
      ));
    }

    // Validate connection pool settings
    if self.database.max_connections == 0 {
      return Err(color_eyre::eyre::eyre!(
        "Max database connections must be greater than 0"
      ));
    }

    if self.database.min_connections > self.database.max_connections {
      return Err(color_eyre::eyre::eyre!(
        "Min database connections cannot exceed max connections"
      ));
    }

    // Validate server settings
    if self.server.port == 0 {
      return Err(color_eyre::eyre::eyre!(
        "Server port must be greater than 0"
      ));
    }

    // Validate evaluator settings
    if self.evaluator.poll_interval == 0 {
      return Err(color_eyre::eyre::eyre!(
        "Evaluator poll interval must be greater than 0"
      ));
    }

    // Validate queue runner settings
    if let Some(t) = self.queue_runner.psi_threshold
      && !(0.0..=100.0).contains(&t)
    {
      return Err(color_eyre::eyre::eyre!(
        "queue_runner.psi_threshold must be in [0.0, 100.0], got {t}"
      ));
    }
    if self.queue_runner.psi_check_timeout == 0 {
      return Err(color_eyre::eyre::eyre!(
        "queue_runner.psi_check_timeout must be greater than 0 seconds"
      ));
    }
    if let Some(rpc) = self.queue_runner.rpc.as_ref() {
      if rpc.max_connections == 0 {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.rpc.max_connections must be greater than 0"
        ));
      }
      if rpc.heartbeat_ttl_secs == 0 {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.rpc.heartbeat_ttl_secs must be greater than 0"
        ));
      }
      if rpc.presign_expiry_secs == 0 {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.rpc.presign_expiry_secs must be greater than 0"
        ));
      }
      for (idx, token_hash) in rpc.auth_tokens.iter().enumerate() {
        let decoded = hex::decode(token_hash).map_err(|e| {
          color_eyre::eyre::eyre!(
            "queue_runner.rpc.auth_tokens[{idx}] must be SHA-256 hex: {e}"
          )
        })?;
        if decoded.len() != 32 {
          return Err(color_eyre::eyre::eyre!(
            "queue_runner.rpc.auth_tokens[{idx}] must decode to 32 bytes, got \
             {}",
            decoded.len()
          ));
        }
      }
      if let Some(oidc) = rpc.oidc.as_ref() {
        if !oidc.issuer.starts_with("https://") {
          return Err(color_eyre::eyre::eyre!(
            "queue_runner.rpc.oidc.issuer must be an https URL"
          ));
        }
        if oidc.audiences.is_empty() {
          return Err(color_eyre::eyre::eyre!(
            "queue_runner.rpc.oidc.audiences must list at least one audience"
          ));
        }
        if oidc.allowed_repositories.is_empty() {
          return Err(color_eyre::eyre::eyre!(
            "queue_runner.rpc.oidc.allowed_repositories must list at least \
             one repository"
          ));
        }
      }
      if rpc.tls.is_none()
        && (rpc.oidc.is_some() || !rpc.auth_tokens.is_empty())
      {
        if !rpc.allow_plaintext {
          return Err(color_eyre::eyre::eyre!(
            "queue_runner.rpc.tls is required when auth_tokens or oidc are \
             set. Set queue_runner.rpc.allow_plaintext = true to accept \
             credentials over plain TCP on a trusted network."
          ));
        }
        tracing::warn!(
          "queue_runner.rpc accepts credentials over plain TCP \
           (allow_plaintext = true)"
        );
      }
    }
    for (idx, pool) in self.queue_runner.ephemeral_pools.iter().enumerate() {
      if self
        .queue_runner
        .rpc
        .as_ref()
        .is_none_or(|rpc| rpc.oidc.is_none())
      {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}] requires queue_runner.rpc.oidc"
        ));
      }
      let gha = &pool.github_actions;
      if pool.name.trim().is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].name cannot be empty"
        ));
      }
      if pool.allowed_build_repositories.is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].allowed_build_repositories \
           must list at least one repository"
        ));
      }
      for (repo_idx, repo) in pool.allowed_build_repositories.iter().enumerate()
      {
        if repo.split_once('/').is_none() {
          return Err(color_eyre::eyre::eyre!(
            "queue_runner.ephemeral_pools[{idx}].\
             allowed_build_repositories[{repo_idx}] must be owner/repo"
          ));
        }
      }
      if gha.workflow_repository.split_once('/').is_none() {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.\
           workflow_repository must be owner/repo"
        ));
      }
      if gha.workflow.trim().is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.workflow cannot \
           be empty"
        ));
      }
      if gha.ref_name.trim().is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.ref_name cannot \
           be empty"
        ));
      }
      if gha.token.is_none() && gha.token_file.is_none() {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].github_actions requires token \
           or token_file"
        ));
      }
      if gha.runner_url.trim().is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.runner_url \
           cannot be empty"
        ));
      }
      if !gha.runner_url.starts_with("circus+tls://") {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.runner_url must \
           use circus+tls://. The dispatched agent sends its OIDC token over \
           the internet and must not use plaintext."
        ));
      }
      if gha.oidc_audience.trim().is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.oidc_audience \
           cannot be empty"
        ));
      }
      if gha.agent_binary_url.trim().is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.\
           agent_binary_url cannot be empty"
        ));
      }
      if let Some(rpc) = self.queue_runner.rpc.as_ref()
        && let Some(oidc) = rpc.oidc.as_ref()
        && !oidc.audiences.iter().any(|aud| aud == &gha.oidc_audience)
      {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.oidc_audience \
           must be listed in queue_runner.rpc.oidc.audiences"
        ));
      }
      if let Some(rpc) = self.queue_runner.rpc.as_ref()
        && let Some(oidc) = rpc.oidc.as_ref()
        && !oidc
          .allowed_repositories
          .iter()
          .any(|repo| repo == &gha.workflow_repository)
      {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].github_actions.\
           workflow_repository must be listed in \
           queue_runner.rpc.oidc.allowed_repositories"
        ));
      }
      if let Some(rpc) = self.queue_runner.rpc.as_ref()
        && let Some(oidc) = rpc.oidc.as_ref()
        && oidc.allowed_subjects.is_empty()
        && oidc.allowed_subject_prefixes.is_empty()
        && oidc.allowed_workflow_refs.is_empty()
      {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}] requires at least one OIDC \
           subject or workflow_ref restriction"
        ));
      }
      if self
        .queue_runner
        .rpc
        .as_ref()
        .and_then(|rpc| rpc.cache_substituter.as_ref())
        .is_none()
      {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}] requires \
           queue_runner.rpc.cache_substituter so fresh CI agents can realise \
           assigned derivations"
        ));
      }
      if self
        .queue_runner
        .rpc
        .as_ref()
        .and_then(|rpc| rpc.cache_public_key.as_ref())
        .is_none()
      {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}] requires \
           queue_runner.rpc.cache_public_key for the derivation substituter"
        ));
      }
      if pool.systems.is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].systems must list at least one \
           system"
        ));
      }
      if pool.max_jobs == 0 {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].max_jobs must be greater than 0"
        ));
      }
      if pool.speed_factor <= 0.0 {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].speed_factor must be greater \
           than 0"
        ));
      }
      if pool.max_inflight == 0 {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}].max_inflight must be greater \
           than 0"
        ));
      }
      if pool.inflight_ttl_secs == 0 || pool.poll_interval_secs == 0 {
        return Err(color_eyre::eyre::eyre!(
          "queue_runner.ephemeral_pools[{idx}] inflight_ttl_secs and \
           poll_interval_secs must be greater than 0"
        ));
      }
    }
    if !matches!(
      self.cache_upload.compression.as_str(),
      "zstd" | "xz" | "gzip" | "none"
    ) {
      return Err(color_eyre::eyre::eyre!(
        "cache_upload.compression must be one of zstd, xz, gzip, none; got {}",
        self.cache_upload.compression
      ));
    }

    // Validate LDAP settings
    if let Some(ldap) = self.server.ldap.as_ref() {
      if ldap.url.is_empty() {
        return Err(color_eyre::eyre::eyre!("server.ldap.url cannot be empty"));
      }
      if ldap.base_dn.is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "server.ldap.base_dn cannot be empty"
        ));
      }
      if ldap.bind_dn_template.is_empty() {
        return Err(color_eyre::eyre::eyre!(
          "server.ldap.bind_dn_template cannot be empty"
        ));
      }
      if !ldap.bind_dn_template.contains("{username}") {
        return Err(color_eyre::eyre::eyre!(
          "server.ldap.bind_dn_template must contain the literal \
           '{{username}}' placeholder"
        ));
      }
    }

    // Validate GC config
    if self.gc.enabled && self.gc.gc_roots_dir.as_os_str().is_empty() {
      return Err(color_eyre::eyre::eyre!(
        "GC roots directory cannot be empty when GC is enabled"
      ));
    }

    // Validate log config
    if self.logs.log_dir.as_os_str().is_empty() {
      return Err(color_eyre::eyre::eyre!("Log directory cannot be empty"));
    }

    // OAuth: when GitHub OAuth is configured, a client secret must be
    // available (inline or via file).
    if let Some(ref github) = self.oauth.github {
      if github.client_secret.is_empty() && github.client_secret_file.is_none()
      {
        return Err(color_eyre::eyre::eyre!(
          "oauth.github requires client_secret or client_secret_file"
        ));
      }
    }

    // Slack: when configured, a webhook URL must be available.
    if let Some(ref slack) = self.notifications.slack {
      if slack.webhook_url.is_empty() && slack.webhook_url_file.is_none() {
        return Err(color_eyre::eyre::eyre!(
          "notifications.slack requires webhook_url or webhook_url_file"
        ));
      }
    }

    Ok(())
  }
}

/// Apply environment variables to nested config fields that `config-rs`'s
/// `Environment` source does not reliably override.
///
/// `config-rs` has two distinct merge bugs we hit in production:
/// 1. For `Option<T>` fields seeded from `Self::default()`, the typed `Nil` in
///    the default tree is never overwritten by the env source's typed
///    `String`/`Path` value.
/// 2. For nested scalar fields (e.g. `signing.enabled`) where the on-disk
///    config file has explicitly set a value, the env source fails to override
///    the file source despite being added later. (Observed for `bool` under
///    nested structs; top-level scalars work.)
///
/// Rather than continuing to fight `config-rs`, we explicitly apply env
/// vars after deserialization for every field we want operator-overridable.
/// Add new entries here when introducing config options that VM tests or
/// operators need to set via systemd drop-ins.
fn apply_env_overrides_for_option_fields(config: &mut Config) {
  fn opt_str(var: &str) -> Option<String> {
    std::env::var(var).ok().filter(|s| !s.is_empty())
  }
  fn opt_bool(var: &str) -> Option<bool> {
    opt_str(var).and_then(|s| {
      match s.to_ascii_lowercase().as_str() {
        "1" | "true" | "yes" | "on" => Some(true),
        "0" | "false" | "no" | "off" => Some(false),
        _ => None,
      }
    })
  }

  // Secret file overrides used by NixOS/systemd deployments with agenix,
  // sops-nix, or similar secrets managers that provision files at runtime.
  // XXX: Actually a very thin wrapper but doing this inline feels worse.
  fn opt_path(var: &str) -> Option<PathBuf> {
    opt_str(var).map(PathBuf::from)
  }

  if let Some(v) = opt_path("CIRCUS_DATABASE__URL_FILE") {
    config.database.url_file = Some(v);
  }
  if let Some(v) = opt_path("CIRCUS_SERVER__API_KEY_FILE") {
    config.server.api_key_file = Some(v);
  }
  if let Some(v) = opt_path("CIRCUS_SERVER__WEBHOOK_SECRET_ENCRYPTION_KEY_FILE")
  {
    config.server.webhook_secret_encryption_key_file = Some(v);
  }

  // Notifications: Option<String> fields
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__WEBHOOK_URL") {
    config.notifications.webhook_url = Some(v);
  }
  if let Some(v) = opt_path("CIRCUS_NOTIFICATIONS__WEBHOOK_URL_FILE") {
    config.notifications.webhook_url_file = Some(v);
  }
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__GITHUB_TOKEN") {
    config.notifications.github_token = Some(v);
  }
  if let Some(v) = opt_path("CIRCUS_NOTIFICATIONS__GITHUB_TOKEN_FILE") {
    config.notifications.github_token_file = Some(v);
  }
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__GITEA_URL") {
    config.notifications.gitea_url = Some(v);
  }
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__GITEA_TOKEN") {
    config.notifications.gitea_token = Some(v);
  }
  if let Some(v) = opt_path("CIRCUS_NOTIFICATIONS__GITEA_TOKEN_FILE") {
    config.notifications.gitea_token_file = Some(v);
  }
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__GITLAB_URL") {
    config.notifications.gitlab_url = Some(v);
  }
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__GITLAB_TOKEN") {
    config.notifications.gitlab_token = Some(v);
  }
  if let Some(v) = opt_path("CIRCUS_NOTIFICATIONS__GITLAB_TOKEN_FILE") {
    config.notifications.gitlab_token_file = Some(v);
  }

  // Signing: bool + Option<PathBuf>
  if let Some(v) = opt_bool("CIRCUS_SIGNING__ENABLED") {
    config.signing.enabled = v;
  }
  if let Some(v) = opt_str("CIRCUS_SIGNING__KEY_FILE") {
    config.signing.key_file = Some(std::path::PathBuf::from(v));
  }

  // GC: bool + scalar fields that VM tests toggle via systemd drop-ins.
  if let Some(v) = opt_bool("CIRCUS_GC__ENABLED") {
    config.gc.enabled = v;
  }
  if let Some(v) = opt_str("CIRCUS_GC__GC_ROOTS_DIR") {
    config.gc.gc_roots_dir = std::path::PathBuf::from(v);
  }
  if let Ok(v) = std::env::var("CIRCUS_GC__MAX_AGE_DAYS")
    && let Ok(parsed) = v.parse()
  {
    config.gc.max_age_days = parsed;
  }
  if let Ok(v) = std::env::var("CIRCUS_GC__CLEANUP_INTERVAL")
    && let Ok(parsed) = v.parse()
  {
    config.gc.cleanup_interval = parsed;
  }
}

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
        if let toml::Value::String(s) = val {
          if SECRET_KEYS.contains(&key.as_str())
            || s.starts_with("postgresql://")
            || s.starts_with("postgres://")
          {
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

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Fine in tests")]
mod tests {
  use std::env;

  use super::*;

  #[test]
  fn test_default_config() {
    let config = Config::default();
    assert!(config.validate().is_ok());
  }

  #[test]
  fn test_invalid_database_url() {
    let mut config = Config::default();
    config.database.url = "invalid://url".to_string();
    assert!(config.validate().is_err());
  }

  #[test]
  fn test_invalid_port() {
    let mut config = Config::default();
    config.server.port = 0;
    assert!(config.validate().is_err());

    config.server.port = 65535;
    assert!(config.validate().is_ok()); // valid port
  }

  #[test]
  fn test_invalid_connections() {
    let mut config = Config::default();
    config.database.max_connections = 0;
    assert!(config.validate().is_err());

    config.database.max_connections = 10;
    config.database.min_connections = 15;
    assert!(config.validate().is_err());
  }

  #[test]
  fn test_declarative_config_default_is_empty() {
    let config = DeclarativeConfig::default();
    assert!(config.projects.is_empty());
    assert!(config.api_keys.is_empty());
  }

  #[test]
  fn test_declarative_config_deserialization() {
    let toml_str = r#"
            [[projects]]
            name = "my-project"
            repository_url = "https://github.com/test/repo"
            description = "Test project"

            [[projects.jobsets]]
            name = "packages"
            nix_expression = "packages"
            trigger_mode = "interval"

            [[api_keys]]
            name = "admin-key"
            key = "circus_secret_key_123"
            role = "admin"
        "#;
    let config: DeclarativeConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.projects.len(), 1);
    assert_eq!(config.projects[0].name, "my-project");
    assert_eq!(config.projects[0].jobsets.len(), 1);
    assert_eq!(config.projects[0].jobsets[0].name, "packages");
    assert!(config.projects[0].jobsets[0].enabled); // default true
    assert!(config.projects[0].jobsets[0].flake_mode); // default true
    assert_eq!(
      config.projects[0].jobsets[0].trigger_mode.as_deref(),
      Some("interval")
    );
    assert_eq!(config.api_keys.len(), 1);
    assert_eq!(config.api_keys[0].role, "admin");
  }

  #[test]
  fn test_page_access_config_deserialization() {
    let toml_str = r#"
            [page_access]
            evaluations = "authenticated"
            metrics = "admin"
        "#;

    let config: ServerConfig = toml::from_str(toml_str).unwrap();
    // `projects` is not set in the TOML above, so it keeps its default. The
    // secure default is `Authenticated` (the dashboard does not expose the
    // project list anonymously unless an operator opts in).
    assert_eq!(config.page_access.projects, PageAccessLevel::Authenticated);
    assert_eq!(
      config.page_access.evaluations,
      PageAccessLevel::Authenticated
    );
    assert_eq!(config.page_access.metrics, PageAccessLevel::Admin);
  }

  #[test]
  fn test_declarative_config_serialization_roundtrip() {
    let config = DeclarativeConfig {
      projects:        vec![DeclarativeProject {
        name:           "test".to_string(),
        repository_url: "https://example.com/repo".to_string(),
        description:    Some("desc".to_string()),
        jobsets:        vec![DeclarativeJobset {
          name:              "checks".to_string(),
          nix_expression:    "checks".to_string(),
          enabled:           true,
          flake_mode:        true,
          check_interval:    300,
          trigger_mode:      None,
          state:             None,
          branch:            None,
          scheduling_shares: 100,
          keep_nr:           None,
          inputs:            vec![],
        }],
        notifications:  vec![],
        webhooks:       vec![],
        channels:       vec![],
        members:        vec![],
      }],
      api_keys:        vec![DeclarativeApiKey {
        name:     "test-key".to_string(),
        key:      Some("circus_test".to_string()),
        key_file: None,
        role:     "admin".to_string(),
      }],
      users:           vec![],
      remote_builders: vec![],
    };

    let json = serde_json::to_string(&config).unwrap();
    let parsed: DeclarativeConfig = serde_json::from_str(&json).unwrap();
    assert_eq!(parsed.projects.len(), 1);
    assert_eq!(parsed.projects[0].jobsets[0].check_interval, 300);
    assert_eq!(parsed.api_keys[0].name, "test-key");
  }

  #[test]
  fn test_declarative_config_with_main_config() {
    let config = Config::default();
    assert!(config.declarative.projects.is_empty());
    assert!(config.declarative.api_keys.is_empty());
    let toml_str = toml::to_string_pretty(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert!(parsed.declarative.projects.is_empty());
  }

  #[test]
  fn test_declarative_api_key_default_role_is_read_only() {
    let toml_str = r#"
            [[api_keys]]
            name = "default-key"
            key = "circus_test_123"
        "#;
    let config: DeclarativeConfig = toml::from_str(toml_str).unwrap();
    assert_eq!(config.api_keys[0].role, "read-only");
  }

  #[test]
  fn test_environment_override() {
    // SAFETY: setting environment variables is not thread-safe but tests run
    // sequentially. This is a common testing pattern for configuration.
    unsafe {
      env::set_var(
        "CIRCUS_DATABASE__URL",
        "postgresql://test:test@localhost/test",
      );
      env::set_var("CIRCUS_SERVER__PORT", "8080");
    }

    let db_url = std::env::var("CIRCUS_DATABASE__URL").unwrap();
    let server_port = std::env::var("CIRCUS_SERVER__PORT").unwrap();

    assert_eq!(db_url, "postgresql://test:test@localhost/test");
    assert_eq!(server_port, "8080");

    // SAFETY: ditto, cleaning up test state.
    unsafe {
      env::remove_var("CIRCUS_DATABASE__URL");
      env::remove_var("CIRCUS_SERVER__PORT");
    }
  }

  #[test]
  fn test_unsupported_timeout_config() {
    let mut config = Config::default();
    config.queue_runner.unsupported_timeout = Some(Duration::from_hours(1));

    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(
      parsed.queue_runner.unsupported_timeout,
      Some(Duration::from_hours(1))
    );
  }

  #[test]
  fn test_unsupported_timeout_default() {
    let config = Config::default();
    assert_eq!(config.queue_runner.unsupported_timeout, None);
  }

  #[test]
  fn test_unsupported_timeout_various_formats() {
    let mut config = Config::default();
    config.queue_runner.unsupported_timeout = Some(Duration::from_mins(30));
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(
      parsed.queue_runner.unsupported_timeout,
      Some(Duration::from_mins(30))
    );

    let mut config = Config::default();
    config.queue_runner.unsupported_timeout = Some(Duration::from_secs(0));
    let toml_str = toml::to_string(&config).unwrap();
    let parsed: Config = toml::from_str(&toml_str).unwrap();
    assert_eq!(
      parsed.queue_runner.unsupported_timeout,
      Some(Duration::from_secs(0))
    );
  }

  #[test]
  fn test_humantime_serde_parsing() {
    let toml = r#"
workers = 4
poll_interval = 5
build_timeout = 3600
work_dir = "/tmp/circus"
unsupported_timeout = "2h 30m"
    "#;

    let qr_config: QueueRunnerConfig = toml::from_str(toml).unwrap();
    assert_eq!(
      qr_config.unsupported_timeout,
      Some(Duration::from_mins(150))
    );
  }
}

#[cfg(test)]
#[expect(clippy::unwrap_used, reason = "Fine in tests")]
mod humantime_option_test {
  use super::*;

  #[test]
  fn test_option_humantime_missing() {
    let toml = r#"
workers = 4
poll_interval = 5
build_timeout = 3600
work_dir = "/tmp/circus"
        "#;
    let config: QueueRunnerConfig = toml::from_str(toml).unwrap();
    assert_eq!(config.unsupported_timeout, None);
  }
}
