#[cfg(not(unix))] use std::future::pending;
use std::{ffi::OsString, path::PathBuf, sync::Arc, time::Duration};

use circus_common::Database;
use circus_config::Config;
use clap::Parser;

#[derive(Parser)]
#[command(name = "circus-evaluator")]
#[command(about = "CI Evaluator - Git polling and Nix evaluation")]
struct Cli {
  #[arg(short, long)]
  config: Option<PathBuf>,
}

/// Run the Circus evaluator CLI.
///
/// # Errors
///
/// Returns an error when worker startup, configuration, database setup, or the
/// evaluator runtime fails.
pub fn run() -> color_eyre::Result<()> {
  run_from(std::env::args_os())
}

/// Run the Circus evaluator CLI with explicit argv values.
///
/// # Errors
///
/// Returns an error when worker startup, configuration, database setup, or the
/// evaluator runtime fails.
pub fn run_from<I, T>(args: I) -> color_eyre::Result<()>
where
  I: IntoIterator<Item = T>,
  T: Into<OsString> + Clone,
{
  // evix evaluates Nix in worker subprocesses that re-execute this binary with
  // `EVIX_WORKER` set. When invoked that way, act purely as an evix worker and
  // do not start the evaluator service (tokio runtime, database, etc.).
  if std::env::var_os(evix::WORKER_ENV).is_some() {
    return evix::run_worker()
      .map_err(|e| color_eyre::eyre::eyre!("evix worker failed: {e:#}"));
  }
  color_eyre::install()?;
  circus_common::install_crypto_provider()?;

  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?;
  runtime.block_on(run_async(args))
}

async fn run_async<I, T>(args: I) -> color_eyre::Result<()>
where
  I: IntoIterator<Item = T>,
  T: Into<OsString> + Clone,
{
  let cli = Cli::parse_from(args);

  let config = Config::load(cli.config.as_deref())?;
  circus_common::init_tracing(&config.tracing);

  tracing::info!("Starting CI Evaluator");
  tracing::info!("Configuration loaded");

  // Ensure work directory exists
  tokio::fs::create_dir_all(&config.evaluator.work_dir).await?;
  tracing::info!(work_dir = %config.evaluator.work_dir.display(), "Work directory ready");

  let db = Database::new(config.database.clone()).await?;
  tracing::info!("Database connection established");

  let pool = db.pool().clone();
  let poll_interval = config.evaluator.poll_interval;
  let eval_config = config.evaluator;
  let notifications_config = config.notifications;
  let notification_secret_key = config.server.webhook_secret_encryption_key;

  let wakeup = Arc::new(tokio::sync::Notify::new());
  let listener_handle = circus_common::pg_notify::spawn_listener(
    db.pool(),
    &[circus_common::pg_notify::CHANNEL_JOBSETS_CHANGED],
    Arc::clone(&wakeup),
  );

  tokio::select! {
      result = crate::eval_loop::run(pool, eval_config, notifications_config, notification_secret_key, wakeup) => {
          if let Err(e) = result {
              tracing::error!("Evaluator loop failed: {e}");
          }
      }
      () = heartbeat_loop(db.pool().clone(), poll_interval) => {}
      () = shutdown_signal() => {
          tracing::info!("Shutdown signal received");
      }
  }

  listener_handle.abort();
  let _ = listener_handle.await;

  tracing::info!("Evaluator shutting down, closing database pool");
  db.close().await;

  Ok(())
}

/// Write a service heartbeat on every poll tick so the server's /health
/// endpoint can report evaluator liveness.
async fn heartbeat_loop(pool: sqlx::PgPool, poll_interval_seconds: u64) {
  let interval = Duration::from_secs(poll_interval_seconds.max(1));
  let poll_u32 = u32::try_from(poll_interval_seconds.min(u64::from(u32::MAX)))
    .unwrap_or(u32::MAX);

  if let Err(e) = circus_common::service_heartbeat::record(
    &pool,
    circus_common::service_heartbeat::SERVICE_EVALUATOR,
    poll_u32,
    Some(env!("CARGO_PKG_VERSION")),
  )
  .await
  {
    tracing::warn!("initial evaluator heartbeat failed: {e}");
  }

  #[expect(clippy::infinite_loop, reason = "intentional heartbeat loop")]
  loop {
    tokio::time::sleep(interval).await;
    if let Err(e) = circus_common::service_heartbeat::record(
      &pool,
      circus_common::service_heartbeat::SERVICE_EVALUATOR,
      poll_u32,
      Some(env!("CARGO_PKG_VERSION")),
    )
    .await
    {
      tracing::warn!("evaluator heartbeat failed: {e}");
    }
  }
}

#[expect(clippy::expect_used, reason = "standard signal handler pattern")]
async fn shutdown_signal() {
  let ctrl_c = async {
    tokio::signal::ctrl_c()
      .await
      .expect("failed to install Ctrl+C handler");
  };

  #[cfg(unix)]
  let terminate = async {
    tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
      .expect("failed to install SIGTERM handler")
      .recv()
      .await;
  };

  #[cfg(not(unix))]
  let terminate = pending::<()>();

  tokio::select! {
      () = ctrl_c => {},
      () = terminate => {},
  }
}
