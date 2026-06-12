//! Agent configuration loaded from TOML + environment.
//!
//! Lookup order (first wins): `--config` flag, `$CIRCUS_AGENT_CONFIG`,
//! `/etc/circus-agent.toml`. Environment overrides with prefix
//! `CIRCUS_AGENT__` and `__` as a path separator.

use std::path::{Path, PathBuf};

pub use circus_logs::TracingConfig;
use serde::{Deserialize, Serialize};

/// Top-level agent config.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AgentConfig {
  pub agent:   Agent,
  #[serde(default)]
  pub tracing: TracingConfig,
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
  #[serde(default)]
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

  /// Per-build parallelism cap, passed to nix as the `cores` setting. This
  /// bounds total build CPU at roughly `max_jobs * cores` threads. 0 keeps
  /// the host's nix default.
  #[serde(default)]
  pub cores: u32,

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

  /// Persistent state file holding the agent's `UUIDv4` machine ID. The
  /// file is created on first start and read on every subsequent start
  /// so reconnects preserve identity. Defaults to
  /// `<work_dir>/machine_id`.
  #[serde(default)]
  pub machine_id_file: Option<PathBuf>,

  /// TLS material. When present, the agent uses `circus+tls://` even if
  /// the URL scheme is `circus://`.
  #[serde(default)]
  pub tls: Option<TlsConfig>,

  /// Indicates whether the builder will use rootless, sandboxed Nix.
  #[serde(default)]
  pub rootless: bool,

  /// Sandbox data directory for rootless mode. Defaults to `$XDG_DATA_HOME`,
  /// falling back to `~/.local/share`. Both come from the agent's environment,
  /// and under service accounts, the resulting path often cannot be created or
  /// written. Set this to a directory the agent can write, such as its
  /// `StateDirectory`.
  #[serde(default)]
  pub rootless_data_dir: Option<PathBuf>,

  /// When present (or `--ephemeral`), run as a single-session builder: fresh
  /// machine ID, drain the queue, then exit instead of reconnecting. For CI
  /// runners such as GitHub Actions.
  #[serde(default)]
  pub ephemeral: Option<EphemeralConfig>,
}

/// Lifecycle bounds for an ephemeral (single-session) agent. In-flight builds
/// drain before exit; the runner's orphan sweeper recovers anything still
/// running if the CI host dies first.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct EphemeralConfig {
  /// Exit after this many completed builds. `None` = unbounded.
  #[serde(default)]
  pub max_builds: Option<u32>,

  /// Hard cap on session wall-clock (seconds). `None` = no cap.
  #[serde(default)]
  pub max_lifetime_secs: Option<u64>,

  /// Exit after this many seconds with no running builds.
  #[serde(default = "default_max_idle")]
  pub max_idle_secs: u64,

  /// Append a unique suffix to `name` so concurrent CI runs don't collide on
  /// the unique-name constraint.
  #[serde(default = "default_true")]
  pub unique_name: bool,
}

impl Default for EphemeralConfig {
  fn default() -> Self {
    Self {
      max_builds:        None,
      max_lifetime_secs: None,
      max_idle_secs:     default_max_idle(),
      unique_name:       true,
    }
  }
}

#[derive(Debug, Clone, Default, Deserialize, Serialize)]
pub struct TlsConfig {
  #[serde(default)]
  pub ca_file:   Option<PathBuf>,
  #[serde(default)]
  pub cert_file: Option<PathBuf>,
  #[serde(default)]
  pub key_file:  Option<PathBuf>,
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
const fn default_max_idle() -> u64 {
  120
}
const fn default_true() -> bool {
  true
}

impl AgentConfig {
  /// Load from explicit path, env var, or the default location.
  ///
  /// # Errors
  ///
  /// Returns the underlying `config` error on missing file or parse failure.
  pub fn load(path: Option<&Path>) -> Result<Self, config::ConfigError> {
    let chosen = path
      .map(Path::to_path_buf)
      .or_else(|| std::env::var("CIRCUS_AGENT_CONFIG").ok().map(PathBuf::from))
      .unwrap_or_else(|| PathBuf::from("/etc/circus-agent.toml"));

    let cfg = config::Config::builder()
      .add_source(config::File::from(chosen.as_path()))
      .add_source(
        config::Environment::with_prefix("CIRCUS_AGENT").separator("__"),
      )
      .build()?;

    let mut parsed = cfg.try_deserialize::<Self>()?;

    if let Ok(token) = std::env::var("CIRCUS_AGENT_TOKEN") {
      parsed.agent.auth_token = token;
    }
    if parsed.agent.auth_token.is_empty() {
      return Err(config::ConfigError::Message(
        "no auth token: set CIRCUS_AGENT_TOKEN or agent.auth_token".into(),
      ));
    }

    Ok(parsed)
  }
}
