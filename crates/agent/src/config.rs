//! Agent configuration loaded from TOML + environment.
//!
//! Lookup order (first wins): `--config` flag, `$CIRCUS_AGENT_CONFIG`,
//! `/etc/circus-agent.toml`. Environment overrides with prefix
//! `CIRCUS_AGENT__` and `__` as a path separator.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Top-level agent config.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
  pub agent: Agent,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Agent {
  /// Operator-assigned name. Unique within the cluster.
  pub name: String,

  /// Runner endpoint. Today: `circus://host:port`. With TLS enabled:
  /// `circus+tls://host:port`. The scheme picks the transport.
  pub runner_url: String,

  /// Bearer token presented on `register`. Hashed and compared against
  /// `[builder].auth_tokens` on the runner.
  pub auth_token: String,

  /// Nix systems this agent can build. Must match what the host's Nix
  /// would emit for `currentSystem` plus any cross-systems wired up via
  /// binfmt.
  pub systems: Vec<String>,

  /// Features the agent advertises as available. A build whose
  /// `requiredFeatures` is a subset of this list is eligible.
  #[serde(default)]
  pub supported_features: Vec<String>,

  /// Features the agent insists on. A build that does not require all of
  /// these is rejected on this agent and falls through to the next.
  #[serde(default)]
  pub mandatory_features: Vec<String>,

  /// Maximum concurrent builds. The agent never accepts more than this
  /// from the runner.
  #[serde(default = "default_max_jobs")]
  pub max_jobs: u32,

  /// Scheduling weight relative to other agents. 1.0 = baseline.
  #[serde(default = "default_speed_factor")]
  pub speed_factor: f32,

  /// Reconnect delay after a connection drop.
  #[serde(default = "default_reconnect_delay")]
  pub reconnect_delay_secs: u64,

  /// Heartbeat interval. Match this to the runner's `heartbeat_ttl / 3`
  /// for a comfortable margin.
  #[serde(default = "default_heartbeat_interval")]
  pub heartbeat_interval_secs: u64,

  /// Working directory for transient build state (logs in flight, build
  /// dir overrides). Defaults to `/var/lib/circus-agent`.
  #[serde(default = "default_work_dir")]
  pub work_dir: PathBuf,

  /// Persistent state file holding the agent's UUIDv4 machine ID. The
  /// file is created on first start and read on every subsequent start
  /// so reconnects preserve identity. Defaults to
  /// `<work_dir>/machine_id`.
  #[serde(default)]
  pub machine_id_file: Option<PathBuf>,

  /// TLS material. When present, the agent uses `circus+tls://` even if
  /// the URL scheme is `circus://`.
  #[serde(default)]
  pub tls: Option<TlsConfig>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct TlsConfig {
  pub ca_file:   PathBuf,
  pub cert_file: PathBuf,
  pub key_file:  PathBuf,
}

const fn default_max_jobs() -> u32 {
  4
}
const fn default_speed_factor() -> f32 {
  1.0
}
const fn default_reconnect_delay() -> u64 {
  5
}
const fn default_heartbeat_interval() -> u64 {
  10
}
fn default_work_dir() -> PathBuf {
  PathBuf::from("/var/lib/circus-agent")
}

impl AgentConfig {
  /// Load from explicit path, env var, or the default location.
  ///
  /// # Errors
  /// Returns the underlying `config` error on missing file or parse failure.
  pub fn load(
    path: Option<&std::path::Path>,
  ) -> Result<Self, config::ConfigError> {
    let chosen = path
      .map(std::path::Path::to_path_buf)
      .or_else(|| std::env::var("CIRCUS_AGENT_CONFIG").ok().map(PathBuf::from))
      .unwrap_or_else(|| PathBuf::from("/etc/circus-agent.toml"));

    let cfg = config::Config::builder()
      .add_source(config::File::from(chosen.as_path()))
      .add_source(
        config::Environment::with_prefix("CIRCUS_AGENT").separator("__"),
      )
      .build()?;

    cfg.try_deserialize()
  }
}
