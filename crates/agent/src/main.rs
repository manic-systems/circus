//! Circus build agent entrypoint.
//!
//! Reads config, resolves the persistent machine ID, then loops on
//! `session::run_once` with backoff between connection attempts.

use std::{path::PathBuf, time::Duration};

use circus_agent::{config::AgentConfig, session};
use clap::Parser;
use uuid::Uuid;

#[derive(Parser)]
#[command(name = "circus-agent", about = "Circus distributed build agent")]
struct Cli {
  #[arg(short, long, value_name = "FILE")]
  config: Option<PathBuf>,
}

fn main() -> anyhow::Result<()> {
  let cli = Cli::parse();
  tracing_subscriber::fmt()
    .with_env_filter(
      tracing_subscriber::EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| tracing_subscriber::EnvFilter::new("info")),
    )
    .init();

  let cfg = AgentConfig::load(cli.config.as_deref())?;
  tracing::info!(name = %cfg.agent.name, "circus-agent starting");

  let machine_id = resolve_machine_id(&cfg)?;
  tracing::info!(machine_id = %machine_id, "agent identity resolved");

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
) -> anyhow::Result<()> {
  #![expect(clippy::infinite_loop, reason = "intentional reconnect loop")]
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let backoff = Duration::from_secs(cfg.reconnect_delay_secs.max(1));
  loop {
    match session::run_once(&cfg, machine_id).await {
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

/// Read or initialise the machine ID file. The runner uses this ID as the
/// stable key into `builder_sessions` and the `AgentPool`, so it must
/// outlive process restarts but be unique to this physical host.
fn resolve_machine_id(cfg: &AgentConfig) -> anyhow::Result<Uuid> {
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
