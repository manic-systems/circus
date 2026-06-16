//! Circus build agent entrypoint.
//!
//! Reads config, resolves the persistent machine ID, then loops on
//! `session::run_once` with backoff between connection attempts.

use std::{path::PathBuf, time::Duration};

use circus_agent::{
  config::{Agent, AgentConfig, EphemeralConfig, TracingConfig},
  sandbox,
  session,
};
use circus_logs::init_tracing;
use clap::Parser;
use color_eyre::eyre::{Result, bail, eyre};
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "circus-agent", about = "Circus distributed build agent")]
struct Cli {
  #[arg(short, long, value_name = "FILE")]
  config: Option<PathBuf>,

  /// Run as a single-session, short-lived agent: generate a fresh machine ID,
  /// drain the queue, then exit instead of reconnecting. Enables ephemeral
  /// mode even if `[agent.ephemeral]` is absent (using defaults). Intended for
  /// CI runners such as GitHub Actions.
  #[arg(long)]
  ephemeral: bool,

  /// Agent name. Overrides `[agent].name`; required for config-free launches.
  #[arg(long)]
  name: Option<String>,

  /// Runner RPC endpoint. Overrides `[agent].runner_url`.
  #[arg(long)]
  runner_url: Option<String>,

  /// Nix system this agent can build. Repeat for multiple systems.
  #[arg(long = "system")]
  systems: Vec<String>,

  /// Nix system feature this agent supports. Repeat for multiple features.
  #[arg(long = "supported-feature")]
  supported_features: Vec<String>,

  /// Nix system feature required by every build assigned to this agent.
  #[arg(long = "mandatory-feature")]
  mandatory_features: Vec<String>,

  /// Maximum concurrent builds accepted by this agent.
  #[arg(long)]
  max_jobs: Option<u32>,

  /// Per-build Nix cores setting. 0 keeps Nix's default.
  #[arg(long)]
  cores: Option<u32>,

  /// Scheduler speed factor advertised by this agent.
  #[arg(long)]
  speed_factor: Option<f32>,

  /// Agent work directory.
  #[arg(long)]
  work_dir: Option<PathBuf>,
}

fn main() -> Result<()> {
  color_eyre::install()?;

  if let Some(code) = sandbox::maybe_run_helper(std::env::args_os())? {
    std::process::exit(code);
  }

  // Use ring for tls.
  rustls::crypto::ring::default_provider()
    .install_default()
    .map_err(|_| eyre!("a rustls CryptoProvider is already installed"))?;

  let cli = Cli::parse();
  let mut cfg = load_config(&cli)?;
  init_tracing(&cfg.tracing);

  // `--ephemeral` enables it without an `[agent.ephemeral]` table.
  if cli.ephemeral && cfg.agent.ephemeral.is_none() {
    cfg.agent.ephemeral = Some(EphemeralConfig::default());
  }
  let ephemeral = cfg.agent.ephemeral.is_some();
  tracing::info!(name = %cfg.agent.name, ephemeral, "circus-agent starting");

  let machine_id = resolve_machine_id(&cfg, ephemeral)?;

  // Uniquify the shared name so concurrent CI runs don't collide.
  if let Some(eph) = &cfg.agent.ephemeral
    && eph.unique_name
  {
    cfg.agent.name = unique_ephemeral_name(&cfg.agent.name, machine_id);
  }
  tracing::info!(machine_id = %machine_id, name = %cfg.agent.name, "agent identity resolved");

  if cfg.agent.rootless {
    if let Some(dir) = &cfg.agent.rootless_data_dir {
      // SAFETY: this is before the runtime starts
      unsafe { std::env::set_var(sandbox::DATA_DIR_ENV, dir) };
    }
    sandbox::preflight()?;
  }

  let rt = tokio::runtime::Builder::new_current_thread()
    .enable_all()
    .build()?;
  let local = tokio::task::LocalSet::new();

  rt.block_on(
    local.run_until(async move { run_supervisor(cfg.agent, machine_id).await }),
  )
}

fn load_config(cli: &Cli) -> Result<AgentConfig> {
  let mut cfg = match AgentConfig::load_if_available(cli.config.as_deref())? {
    Some(cfg) => cfg,
    None => inline_config(cli)?,
  };
  apply_cli_overrides(&mut cfg.agent, cli);
  if cfg.agent.auth_token.is_empty() {
    bail!("no auth token: set CIRCUS_AGENT_TOKEN or agent.auth_token");
  }
  Ok(cfg)
}

fn inline_config(cli: &Cli) -> Result<AgentConfig> {
  let name = cli
    .name
    .clone()
    .ok_or_else(|| eyre!("--name is required without a config file"))?;
  let runner_url = cli
    .runner_url
    .clone()
    .ok_or_else(|| eyre!("--runner-url is required without a config file"))?;
  if cli.systems.is_empty() {
    bail!("at least one --system is required without a config file");
  }
  let auth_token = std::env::var("CIRCUS_AGENT_TOKEN").map_err(|_| {
    eyre!("CIRCUS_AGENT_TOKEN is required without a config file")
  })?;
  Ok(AgentConfig {
    agent:   Agent {
      name,
      runner_url,
      auth_token,
      systems: cli.systems.clone(),
      supported_features: cli.supported_features.clone(),
      mandatory_features: cli.mandatory_features.clone(),
      max_jobs: cli.max_jobs.unwrap_or(1),
      cores: cli.cores.unwrap_or(0),
      speed_factor: cli.speed_factor.unwrap_or(1.0),
      reconnect_delay_secs: 5,
      heartbeat_interval_secs: 10,
      work_dir: cli
        .work_dir
        .clone()
        .unwrap_or_else(|| PathBuf::from("/tmp/circus-agent")),
      machine_id_file: None,
      tls: None,
      rootless: false,
      rootless_data_dir: None,
      ephemeral: None,
    },
    tracing: TracingConfig::default(),
  })
}

fn apply_cli_overrides(agent: &mut Agent, cli: &Cli) {
  if let Some(name) = &cli.name {
    agent.name.clone_from(name);
  }
  if let Some(runner_url) = &cli.runner_url {
    agent.runner_url.clone_from(runner_url);
  }
  if !cli.systems.is_empty() {
    agent.systems.clone_from(&cli.systems);
  }
  if !cli.supported_features.is_empty() {
    agent.supported_features.clone_from(&cli.supported_features);
  }
  if !cli.mandatory_features.is_empty() {
    agent.mandatory_features.clone_from(&cli.mandatory_features);
  }
  if let Some(max_jobs) = cli.max_jobs {
    agent.max_jobs = max_jobs;
  }
  if let Some(cores) = cli.cores {
    agent.cores = cores;
  }
  if let Some(speed_factor) = cli.speed_factor {
    agent.speed_factor = speed_factor;
  }
  if let Some(work_dir) = &cli.work_dir {
    agent.work_dir.clone_from(work_dir);
  }
}

async fn run_supervisor(
  cfg: circus_agent::config::Agent,
  machine_id: Uuid,
) -> Result<()> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  // One session, then exit; the orphan sweeper recovers any in-flight build.
  if cfg.ephemeral.is_some() {
    match session::run_once(&cfg, machine_id).await {
      Ok(()) => tracing::info!("ephemeral session ended; exiting"),
      Err(e) => tracing::warn!(error = %e, "ephemeral session failed; exiting"),
    }
    return Ok(());
  }

  reconnect_forever(&cfg, machine_id).await
}

async fn reconnect_forever(
  cfg: &circus_agent::config::Agent,
  machine_id: Uuid,
) -> Result<()> {
  #![expect(clippy::infinite_loop, reason = "intentional reconnect loop")]
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let backoff = Duration::from_secs(cfg.reconnect_delay_secs.max(1));
  loop {
    match session::run_once(cfg, machine_id).await {
      Ok(()) => {
        tracing::warn!("connection ended cleanly; reconnecting");
      },
      Err(e) => {
        tracing::warn!(error = %e, "connection failed; reconnecting");
      },
    }
    tokio::time::sleep(backoff).await;
  }
}

/// Resolve the machine ID: persistent agents read/init the ID file (identity
/// survives reconnects); ephemeral agents mint a fresh, unpersisted ID.
fn resolve_machine_id(cfg: &AgentConfig, ephemeral: bool) -> Result<Uuid> {
  if ephemeral {
    return Ok(Uuid::new_v4());
  }
  let path = cfg
    .agent
    .machine_id_file
    .clone()
    .unwrap_or_else(|| cfg.agent.work_dir.join("machine_id"));
  if let Ok(s) = std::fs::read_to_string(&path)
    && let Ok(id) = Uuid::parse_str(s.trim())
  {
    return Ok(id);
  }
  if let Some(parent) = path.parent() {
    let _ = std::fs::create_dir_all(parent);
  }
  let id = Uuid::new_v4();
  std::fs::write(&path, id.to_string())?;
  Ok(id)
}

/// Cluster-unique name for an ephemeral run: GitHub run identifiers (for
/// traceability) plus a slice of the random machine ID (for uniqueness).
fn unique_ephemeral_name(base: &str, machine_id: Uuid) -> String {
  const MAX: usize = 128;

  let short = &machine_id.simple().to_string()[..8];
  let suffix = match (
    std::env::var("GITHUB_RUN_ID").ok(),
    std::env::var("GITHUB_RUN_ATTEMPT").ok(),
  ) {
    (Some(run), Some(attempt)) => {
      format!("-gh{run}.{attempt}-{short}")
    },
    (Some(run), None) => format!("-gh{run}-{short}"),
    _ => format!("-{short}"),
  };
  let available = MAX.saturating_sub(suffix.len());
  let base = if base.len() > available {
    let mut end = available;
    while !base.is_char_boundary(end) {
      end -= 1;
    }
    &base[..end]
  } else {
    base
  };
  format!("{base}{suffix}")
}
