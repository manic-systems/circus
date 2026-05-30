//! Configuration management for circus

use std::{path::PathBuf, time::Duration};

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
  pub max_connections: u32,
  pub min_connections: u32,
  pub connect_timeout: u64,
  pub idle_timeout:    u64,
  pub max_lifetime:    u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerConfig {
  pub host:                   String,
  pub port:                   u16,
  pub request_timeout:        u64,
  pub max_body_size:          usize,
  pub api_key:                Option<String>,
  pub allowed_origins:        Vec<String>,
  pub cors_permissive:        bool,
  pub rate_limit_rps:         Option<u64>,
  pub rate_limit_burst:       Option<u32>,
  /// Allowed URL schemes for repository URLs. Insecure schemes emit a warning
  /// on startup
  pub allowed_url_schemes:    Vec<String>,
  /// Force Secure flag on session cookies (enable when behind HTTPS reverse
  /// proxy)
  pub force_secure_cookies:   bool,
  /// Optional regex for email format validation.
  /// When unset (the default), only structural checks are applied: the address
  /// must be non-empty, at most 255 characters, and contain `@`. Set this to
  /// enforce a stricter pattern, e.g.:
  /// `'^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$'`
  pub email_validation_regex: Option<String>,
  /// LDAP authentication configuration.
  pub ldap:                   Option<LdapConfig>,
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

  /// Extra arguments appended to every `nix build` invocation (after the
  /// queue-runner's defaults, before the installable). Use this to inject
  /// substituters, trusted public keys, or override sandbox settings without
  /// changing the daemon's `nix.conf`. Example:
  /// `["--option", "extra-substituters", "https://cache.nixos.org"]`.
  #[serde(default)]
  pub extra_nix_build_args: Vec<String>,

  /// Capnp-rpc endpoint for persistent build agents. When set, the
  /// queue-runner listens on this address and dispatches eligible builds
  /// to connected agents in preference to the SSH `remote_builders`
  /// path. Leave unset to disable the agent path entirely.
  #[serde(default)]
  pub rpc: Option<RpcConfig>,
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

  /// Heartbeat freshness window. Heartbeats older than this drop the
  /// agent from scheduling decisions.
  #[serde(default = "default_heartbeat_ttl_secs")]
  pub heartbeat_ttl_secs: u64,
}

/// Server-side TLS material for the capnp-rpc endpoint. When `client_ca`
/// is set the server requires mTLS and pins the client certificate's CN
/// to the registered agent name; without it the server accepts any TLS
/// client and authentication relies on the bearer token alone.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcTlsConfig {
  pub cert_file: PathBuf,
  pub key_file:  PathBuf,
  #[serde(default)]
  pub client_ca: Option<PathBuf>,
  /// When `client_ca` is set and this is true, the server requires the
  /// client certificate's CN to equal the agent's `name`. Defaults to
  /// true; flip to false for cluster operators using a per-tenant CA
  /// rather than per-host certs.
  #[serde(default = "default_true")]
  pub pin_cn:    bool,
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
  pub client_id:     String,
  pub client_secret: String,
  pub redirect_uri:  String,
}

impl std::fmt::Debug for GitHubOAuthConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("GitHubOAuthConfig")
      .field("client_id", &self.client_id)
      .field("client_secret", &"[REDACTED]")
      .field("redirect_uri", &self.redirect_uri)
      .finish()
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
// Manual Default impl below so the default tree fed into `config-rs` matches
// the per-field `#[serde(default = ...)]` annotations. `#[derive(Default)]`
// would silently set `enable_retry_queue = false`, which is wrong.
pub struct NotificationsConfig {
  pub webhook_url:         Option<String>,
  pub github_token:        Option<String>,
  pub gitea_url:           Option<String>,
  pub gitea_token:         Option<String>,
  pub gitlab_url:          Option<String>,
  pub gitlab_token:        Option<String>,
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

impl Default for NotificationsConfig {
  fn default() -> Self {
    Self {
      webhook_url:         None,
      github_token:        None,
      gitea_url:           None,
      gitea_token:         None,
      gitlab_url:          None,
      gitlab_token:        None,
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
  pub webhook_url:     String,
  /// Only send notifications for failed builds (default false).
  #[serde(default)]
  pub on_failure_only: bool,
}

impl std::fmt::Debug for SlackNotificationConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("SlackNotificationConfig")
      .field("webhook_url", &"[REDACTED]")
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

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct EmailConfig {
  pub smtp_host:       String,
  pub smtp_port:       u16,
  pub smtp_user:       Option<String>,
  pub smtp_password:   Option<String>,
  pub from_address:    String,
  pub to_addresses:    Vec<String>,
  pub tls:             bool,
  pub on_failure_only: bool,
}

/// NAR compression algorithm served by the binary cache.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum NarCompression {
  #[default]
  Zstd,
  Bzip2,
  Brotli,
  Xz,
  None,
}

impl NarCompression {
  #[must_use]
  pub const fn as_str(&self) -> &'static str {
    match self {
      Self::Zstd => "zstd",
      Self::Bzip2 => "bzip2",
      Self::Brotli => "br",
      Self::Xz => "xz",
      Self::None => "none",
    }
  }

  #[must_use]
  pub const fn file_extension(&self) -> &'static str {
    match self {
      Self::Zstd => ".nar.zst",
      Self::Bzip2 => ".nar.bz2",
      Self::Brotli => ".nar.br",
      Self::Xz => ".nar.xz",
      Self::None => ".nar",
    }
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheConfig {
  pub enabled:         bool,
  pub secret_key_file: Option<PathBuf>,
  /// NAR compression algorithm (default: zstd)
  #[serde(default)]
  pub compression:     NarCompression,
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
#[derive(Default)]
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

/// S3-specific cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
#[derive(Default)]
pub struct S3CacheConfig {
  /// AWS region (e.g., "us-east-1")
  pub region:            Option<String>,
  /// Path prefix within the bucket (e.g., "nix-cache/")
  pub prefix:            Option<String>,
  /// AWS access key ID (optional - uses IAM role if not provided)
  pub access_key_id:     Option<String>,
  /// AWS secret access key (optional - uses IAM role if not provided)
  pub secret_access_key: Option<String>,
  /// Session token for temporary credentials (optional)
  pub session_token:     Option<String>,
  /// Endpoint URL for S3-compatible services (e.g., `MinIO`)
  pub endpoint_url:      Option<String>,
  /// Whether to use path-style addressing (for `MinIO` compatibility)
  pub use_path_style:    bool,
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct TracingConfig {
  pub level:           String,
  pub format:          String,
  pub show_targets:    bool,
  pub show_timestamps: bool,
}

impl Default for TracingConfig {
  fn default() -> Self {
    Self {
      level:           "info".to_string(),
      format:          "compact".to_string(),
      show_targets:    true,
      show_timestamps: true,
    }
  }
}

impl Default for DatabaseConfig {
  fn default() -> Self {
    Self {
      url:             "postgresql://circus:password@localhost/circus"
        .to_string(),
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
  pub fn validate(&self) -> anyhow::Result<()> {
    if self.url.is_empty() {
      return Err(anyhow::anyhow!("Database URL cannot be empty"));
    }

    if !self.url.starts_with("postgresql://")
      && !self.url.starts_with("postgres://")
    {
      return Err(anyhow::anyhow!(
        "Database URL must start with postgresql:// or postgres://"
      ));
    }

    if self.max_connections == 0 {
      return Err(anyhow::anyhow!(
        "Max database connections must be greater than 0"
      ));
    }

    if self.min_connections > self.max_connections {
      return Err(anyhow::anyhow!(
        "Min database connections cannot exceed max connections"
      ));
    }

    Ok(())
  }
}

impl Default for ServerConfig {
  fn default() -> Self {
    Self {
      host:                   "127.0.0.1".to_string(),
      port:                   3000,
      request_timeout:        30,
      max_body_size:          10 * 1024 * 1024, // 10MB
      api_key:                None,
      allowed_origins:        Vec::new(),
      cors_permissive:        false,
      rate_limit_rps:         None,
      rate_limit_burst:       None,
      allowed_url_schemes:    vec![
        "https".into(),
        "http".into(),
        "git".into(),
        "ssh".into(),
      ],
      force_secure_cookies:   false,
      email_validation_regex: None,
      ldap:                   None,
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
      work_dir:             PathBuf::from("/tmp/circus-queue-runner"),
      strict_errors:        false,
      failed_paths_cache:   true,
      failed_paths_ttl:     86400,
      unsupported_timeout:  None,
      scheduling_strategy:  BuilderSchedulingStrategy::SpeedFactorOnly,
      psi_threshold:        None,
      psi_check_timeout:    5,
      extra_nix_build_args: Vec::new(),
      rpc:                  None,
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
      compression:     NarCompression::Zstd,
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
  pub notifications_config: NotificationsConfig,
  pub failed_paths_ttl:     u64,
  pub scheduling_strategy:  BuilderSchedulingStrategy,
  pub psi_threshold:        Option<f64>,
  pub psi_check_timeout:    std::time::Duration,
  pub extra_nix_build_args: Vec<String>,
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
      notifications_config: config.notifications.clone(),
      failed_paths_ttl:     config.queue_runner.failed_paths_ttl,
      scheduling_strategy:  config.queue_runner.scheduling_strategy.clone(),
      psi_threshold:        config.queue_runner.psi_threshold,
      psi_check_timeout:    std::time::Duration::from_secs(
        config.queue_runner.psi_check_timeout,
      ),
      extra_nix_build_args: config.queue_runner.extra_nix_build_args.clone(),
    }
  }
}

impl Config {
  /// Load configuration from file and environment variables.
  ///
  /// # Errors
  ///
  /// Returns error if configuration loading or validation fails.
  pub fn load() -> anyhow::Result<Self> {
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

    // Validate configuration
    config.validate()?;

    Ok(config)
  }

  /// Validate all configuration sections.
  ///
  /// # Errors
  ///
  /// Returns error if any configuration section is invalid.
  pub fn validate(&self) -> anyhow::Result<()> {
    // Validate database URL
    if self.database.url.is_empty() {
      return Err(anyhow::anyhow!("Database URL cannot be empty"));
    }

    if !self.database.url.starts_with("postgresql://")
      && !self.database.url.starts_with("postgres://")
    {
      return Err(anyhow::anyhow!(
        "Database URL must start with postgresql:// or postgres://"
      ));
    }

    // Validate connection pool settings
    if self.database.max_connections == 0 {
      return Err(anyhow::anyhow!(
        "Max database connections must be greater than 0"
      ));
    }

    if self.database.min_connections > self.database.max_connections {
      return Err(anyhow::anyhow!(
        "Min database connections cannot exceed max connections"
      ));
    }

    // Validate server settings
    if self.server.port == 0 {
      return Err(anyhow::anyhow!("Server port must be greater than 0"));
    }

    // Validate evaluator settings
    if self.evaluator.poll_interval == 0 {
      return Err(anyhow::anyhow!(
        "Evaluator poll interval must be greater than 0"
      ));
    }

    // Validate queue runner settings
    if self.queue_runner.workers == 0 {
      return Err(anyhow::anyhow!(
        "Queue runner workers must be greater than 0"
      ));
    }
    if let Some(t) = self.queue_runner.psi_threshold
      && !(0.0..=100.0).contains(&t)
    {
      return Err(anyhow::anyhow!(
        "queue_runner.psi_threshold must be in [0.0, 100.0], got {t}"
      ));
    }
    if self.queue_runner.psi_check_timeout == 0 {
      return Err(anyhow::anyhow!(
        "queue_runner.psi_check_timeout must be greater than 0 seconds"
      ));
    }

    // Validate LDAP settings
    if let Some(ldap) = self.server.ldap.as_ref() {
      if ldap.url.is_empty() {
        return Err(anyhow::anyhow!("server.ldap.url cannot be empty"));
      }
      if ldap.base_dn.is_empty() {
        return Err(anyhow::anyhow!("server.ldap.base_dn cannot be empty"));
      }
      if ldap.bind_dn_template.is_empty() {
        return Err(anyhow::anyhow!(
          "server.ldap.bind_dn_template cannot be empty"
        ));
      }
      if !ldap.bind_dn_template.contains("{username}") {
        return Err(anyhow::anyhow!(
          "server.ldap.bind_dn_template must contain the literal \
           '{{username}}' placeholder"
        ));
      }
    }

    // Validate GC config
    if self.gc.enabled && self.gc.gc_roots_dir.as_os_str().is_empty() {
      return Err(anyhow::anyhow!(
        "GC roots directory cannot be empty when GC is enabled"
      ));
    }

    // Validate log config
    if self.logs.log_dir.as_os_str().is_empty() {
      return Err(anyhow::anyhow!("Log directory cannot be empty"));
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

  // Notifications: Option<String> fields
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__WEBHOOK_URL") {
    config.notifications.webhook_url = Some(v);
  }
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__GITHUB_TOKEN") {
    config.notifications.github_token = Some(v);
  }
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__GITEA_URL") {
    config.notifications.gitea_url = Some(v);
  }
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__GITEA_TOKEN") {
    config.notifications.gitea_token = Some(v);
  }
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__GITLAB_URL") {
    config.notifications.gitlab_url = Some(v);
  }
  if let Some(v) = opt_str("CIRCUS_NOTIFICATIONS__GITLAB_TOKEN") {
    config.notifications.gitlab_token = Some(v);
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
    assert_eq!(config.api_keys.len(), 1);
    assert_eq!(config.api_keys[0].role, "admin");
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

    // SAFETY: ditto — cleaning up test state.
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
