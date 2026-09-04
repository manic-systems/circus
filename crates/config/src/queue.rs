use std::{path::PathBuf, time::Duration};

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
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

  /// Skip build agents whose heartbeat PSI avg10 exceeds this threshold
  /// (0.0-100.0). `None` disables PSI checking.
  pub psi_threshold: Option<f64>,

  /// Extra arguments appended to every `nix build` invocation.
  #[serde(default)]
  pub extra_nix_build_args: Vec<String>,

  /// Systems the runner host itself may build.
  #[serde(default)]
  pub local_systems: Option<Vec<String>>,

  /// `system-features` of the runner host's nix.
  #[serde(default)]
  pub local_features: Option<Vec<String>>,

  /// Capnp-rpc endpoint for persistent build agents.
  #[serde(default)]
  pub rpc: Option<RpcConfig>,

  /// GitHub Actions-backed ephemeral `circus-agent` pools.
  #[serde(default)]
  pub ephemeral_pools: Vec<EphemeralPoolConfig>,
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
      extra_nix_build_args: Vec::new(),
      local_systems:        None,
      local_features:       None,
      rpc:                  None,
      ephemeral_pools:      Vec::new(),
    }
  }
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
  /// Exact agent binary URL downloaded by the workflow.
  pub agent_binary_url:    String,
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

/// Configuration for the capnp-rpc agent endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcConfig {
  /// Listen address, e.g. `"0.0.0.0:8443"` or `"[::]:8443"`.
  pub bind: String,

  /// SHA-256 hex digests of accepted bearer tokens.
  #[serde(default)]
  pub auth_tokens: Vec<String>,

  /// Hard cap on concurrent connections; serves as a flood guard.
  #[serde(default = "default_max_rpc_conns")]
  pub max_connections: usize,

  /// Lifetime of every minted presigned PUT URL.
  #[serde(default = "default_presign_expiry_secs")]
  pub presign_expiry_secs: u64,

  /// Optional TLS material. Plain TCP when absent.
  #[serde(default)]
  pub tls: Option<RpcTlsConfig>,

  /// Allow registration credentials over plain TCP.
  #[serde(default)]
  pub allow_plaintext: bool,

  /// Heartbeat freshness window.
  #[serde(default = "default_heartbeat_ttl_secs")]
  pub heartbeat_ttl_secs: u64,

  /// Cache agents substitute drv closures from, forwarded to each agent.
  #[serde(default)]
  pub cache_substituter: Option<String>,

  /// Public key to trust for `cache_substituter`.
  #[serde(default)]
  pub cache_public_key: Option<String>,

  /// Accept short-lived OIDC JWTs in place of a bearer token.
  #[serde(default)]
  pub oidc: Option<RpcOidcConfig>,
}

/// OIDC trust settings for the capnp-rpc endpoint.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RpcOidcConfig {
  /// Expected `iss` claim.
  #[serde(default = "default_oidc_issuer")]
  pub issuer: String,

  /// JWKS endpoint override.
  #[serde(default)]
  pub jwks_url: Option<String>,

  /// Accepted `aud` claim values.
  #[serde(default)]
  pub audiences: Vec<String>,

  /// `owner/repo` slugs allowed to register.
  #[serde(default)]
  pub allowed_repositories: Vec<String>,

  /// Exact `sub` claim values allowed to register.
  #[serde(default)]
  pub allowed_subjects: Vec<String>,

  /// Accepted `sub` claim prefixes.
  #[serde(default)]
  pub allowed_subject_prefixes: Vec<String>,

  /// Exact GitHub `workflow_ref` claim values allowed to register.
  #[serde(default)]
  pub allowed_workflow_refs: Vec<String>,

  /// Exact GitHub `ref` claim values allowed to register.
  #[serde(default)]
  pub allowed_refs: Vec<String>,
}

fn default_oidc_issuer() -> String {
  "https://token.actions.githubusercontent.com".to_owned()
}

/// Server-side TLS material for the capnp-rpc endpoint.
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

const fn default_true() -> bool {
  true
}

const fn default_failed_paths_ttl() -> u64 {
  86400
}

/// Builder scheduling strategy for `find_for_system()`.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BuilderSchedulingStrategy {
  /// Order by `speed_factor DESC` only (default, legacy behaviour).
  #[default]
  SpeedFactorOnly,
  /// Order by `cpu_cores * speed_factor DESC` (higher core x speed wins).
  CpuCoreCountWithSpeedFactor,
  /// Weighted by available slots: `(max_jobs - active) * speed_factor DESC`.
  Dynamic,
}
