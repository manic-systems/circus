use std::sync::Arc;

use circus_common::{Config, Database};
use clap::Parser;

#[derive(Parser)]
#[command(name = "circus-evaluator")]
#[command(about = "CI Evaluator - Git polling and Nix evaluation")]
struct Cli {
  #[arg(short, long)]
  config: Option<String>,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let _cli = Cli::parse();

  let config = Config::load()?;
  circus_common::init_tracing(&config.tracing);

  tracing::info!("Starting CI Evaluator");
  tracing::info!("Configuration loaded");

  // Ensure work directory exists
  tokio::fs::create_dir_all(&config.evaluator.work_dir).await?;
  tracing::info!(work_dir = %config.evaluator.work_dir.display(), "Work directory ready");

  let db = Database::new(config.database.clone()).await?;
  tracing::info!("Database connection established");

  let pool = db.pool().clone();
  let eval_config = config.evaluator;
  let notifications_config = config.notifications;

  let wakeup = Arc::new(tokio::sync::Notify::new());
  let listener_handle = circus_common::pg_notify::spawn_listener(
    db.pool(),
    &[circus_common::pg_notify::CHANNEL_JOBSETS_CHANGED],
    wakeup.clone(),
  );

  tokio::select! {
      result = circus_evaluator::eval_loop::run(pool, eval_config, notifications_config, wakeup) => {
          if let Err(e) = result {
              tracing::error!("Evaluator loop failed: {e}");
          }
      }
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
  let terminate = std::future::pending::<()>();

  tokio::select! {
      () = ctrl_c => {},
      () = terminate => {},
  }
}
