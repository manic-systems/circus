//! Circus build agent entrypoint.
//!
//! Reads config, resolves the persistent machine ID, then loops on
//! `session::run_once` with backoff between connection attempts.

use std::{path::PathBuf, time::Duration};

use circus_agent::{config::AgentConfig, sandbox, session};
use circus_logs::init_tracing;
use clap::Parser;
use color_eyre::eyre::{Result, eyre};
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
  let mut cfg = AgentConfig::load(cli.config.as_deref())?;
  init_tracing(&cfg.tracing);

  // `--ephemeral` enables it without an `[agent.ephemeral]` table.
  if cli.ephemeral && cfg.agent.ephemeral.is_none() {
    cfg.agent.ephemeral =
      Some(circus_agent::config::EphemeralConfig::default());
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
