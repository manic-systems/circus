use std::{collections::BTreeMap, path::PathBuf, time::Duration};

use circus_logs::TracingConfig;
use circus_types::{
  BinaryCacheUpstream,
  ForgeType,
  GlobalRole,
  InputType,
  NotificationType,
  ProjectRole,
};
use serde::{Deserialize, Serialize};

use crate::queue::{BuilderSchedulingStrategy, QueueRunnerConfig};

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Config {
  pub database:      DatabaseConfig,
  pub server:        ServerConfig,
  #[serde(default)]
  pub ui:            UiConfig,
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
#[serde(default)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct UiConfig {
  /// Mount any bundled UI route. Disable this for API-only/headless
  /// deployments.
  pub enabled:        bool,
  /// Mount server-rendered dashboard, login, and logout pages.
  pub dashboard:      bool,
  /// Serve bundled static UI assets.
  pub assets:         bool,
  /// Display name rendered in the dashboard sidebar and document title.
  pub brand_name:     String,
  /// Short subtitle rendered under the brand name.
  pub brand_subtitle: String,
  /// Optional logo URL. Use `/static/custom/<file>` with `static_dir` for
  /// self-hosted logos.
  pub logo_url:       Option<String>,
  /// Optional favicon URL. Use `/static/custom/<file>` with `static_dir` for
  /// self-hosted favicons.
  pub favicon_url:    Option<String>,
  /// Optional CSS file served at `/static/custom.css` after bundled styles.
  pub custom_css:     Option<PathBuf>,
  /// Optional directory served below `/static/custom/`.
  pub static_dir:     Option<PathBuf>,
  /// CSS custom properties emitted at `/static/theme.css`.
  pub css_variables:  BTreeMap<String, String>,
}

impl UiConfig {
  #[must_use]
  pub const fn dashboard_enabled(&self) -> bool {
    self.enabled && self.dashboard
  }

  #[must_use]
  pub const fn assets_enabled(&self) -> bool {
    self.enabled && self.assets
  }
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
#[serde(default)]
pub struct GcConfig {
  pub gc_roots_dir:     PathBuf,
  pub enabled:          bool,
  pub max_age_days:     u64,
  pub cleanup_interval: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct CacheConfig {
  pub enabled:         bool,
  pub secret_key_file: Option<PathBuf>,
  /// Public URL of this binary cache (for channel manifest endpoints)
  pub cache_url:       Option<String>,
  #[serde(default)]
  pub upstreams:       Vec<BinaryCacheUpstream>,
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
  pub name:            String,
  pub repository_url:  String,
  pub description:     Option<String>,
  #[serde(default = "default_true")]
  pub cache_enabled:   bool,
  pub cache_url:       Option<String>,
  #[serde(default)]
  pub cache_upstreams: Vec<BinaryCacheUpstream>,
  #[serde(default)]
  pub jobsets:         Vec<DeclarativeJobset>,
  /// Notification configurations for this project
  #[serde(default)]
  pub notifications:   Vec<DeclarativeNotification>,
  /// Webhook configurations for this project
  #[serde(default)]
  pub webhooks:        Vec<DeclarativeWebhook>,
  /// Release channels for this project
  #[serde(default)]
  pub channels:        Vec<DeclarativeChannel>,
  /// Project members with their roles
  #[serde(default)]
  pub members:         Vec<DeclarativeProjectMember>,
}

/// Declarative notification configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeNotification {
  /// Notification type: `github_status`, `email`, `gitlab_status`,
  /// `gitea_status`, `webhook`
  pub notification_type: NotificationType,
  /// Type-specific configuration (JSON object)
  pub config:            serde_json::Value,
  #[serde(default = "default_true")]
  pub enabled:           bool,
}

/// Declarative webhook configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeclarativeWebhook {
  /// Forge type: github, gitea, gitlab
  pub forge_type:  ForgeType,
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
  pub role:     ProjectRole,
}

const fn default_member_role() -> ProjectRole {
  ProjectRole::Member
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
  /// Glob pattern for repository branches to evaluate, for example `*`.
  pub branch_pattern:    Option<String>,
  /// Glob pattern for repository tags to evaluate, for example `v*`.
  pub tag_pattern:       Option<String>,
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
  pub input_type: InputType,
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
  pub role:     GlobalRole,
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
  pub role:          GlobalRole,
  #[serde(default = "default_true")]
  pub enabled:       bool,
}

const fn default_user_role() -> GlobalRole {
  GlobalRole::ReadOnly
}

const fn default_true() -> bool {
  true
}

const fn default_check_interval() -> i32 {
  60
}

const fn default_scheduling_shares() -> i32 {
  100
}

const fn default_role() -> GlobalRole {
  GlobalRole::ReadOnly
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

impl Default for UiConfig {
  fn default() -> Self {
    Self {
      enabled:        true,
      dashboard:      true,
      assets:         true,
      brand_name:     "circus".to_string(),
      brand_subtitle: "Nix CI".to_string(),
      logo_url:       None,
      favicon_url:    None,
      custom_css:     None,
      static_dir:     None,
      css_variables:  BTreeMap::new(),
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
      upstreams:       Vec::new(),
    }
  }
}

/// Fields that can be updated at runtime via SIGHUP without a restart.
/// Fields that require restart (e.g. `workers`, database pool) are excluded.
#[derive(Debug, Clone)]
pub struct HotConfig {
  pub poll_interval:           Duration,
  pub build_timeout:           Duration,
  pub max_silent_time:         Duration,
  pub notifications_config:    NotificationsConfig,
  /// Key used to encrypt/decrypt notification secrets in the database and in
  /// retry-queue payloads. Mirrors `server.webhook_secret_encryption_key`.
  pub notification_secret_key: Option<String>,
  pub failed_paths_ttl:        u64,
  pub scheduling_strategy:     BuilderSchedulingStrategy,
  pub psi_threshold:           Option<f64>,
  pub psi_check_timeout:       Duration,
  pub extra_nix_build_args:    Vec<String>,
  pub ssh_require_host_key:    bool,
}

impl HotConfig {
  /// Construct a `HotConfig` snapshot from a loaded `Config`.
  #[must_use]
  pub fn from_config(config: &Config) -> Self {
    Self {
      poll_interval:           Duration::from_secs(
        config.queue_runner.poll_interval,
      ),
      build_timeout:           Duration::from_secs(
        config.queue_runner.build_timeout,
      ),
      max_silent_time:         Duration::from_secs(
        config.queue_runner.max_silent_time,
      ),
      notifications_config:    config.notifications.clone(),
      notification_secret_key: config
        .server
        .webhook_secret_encryption_key
        .clone(),
      failed_paths_ttl:        config.queue_runner.failed_paths_ttl,
      scheduling_strategy:     config.queue_runner.scheduling_strategy.clone(),
      psi_threshold:           config.queue_runner.psi_threshold,
      psi_check_timeout:       Duration::from_secs(
        config.queue_runner.psi_check_timeout,
      ),
      extra_nix_build_args:    config.queue_runner.extra_nix_build_args.clone(),
      ssh_require_host_key:    config.queue_runner.ssh_require_host_key,
    }
  }
}
