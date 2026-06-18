#[cfg(not(unix))] use std::future::pending;
use std::{net::SocketAddr, path::PathBuf, sync::Arc};

use circus_common::Database;
use circus_config::Config;
use circus_server::{routes, state};
use clap::Parser;
use state::{AppState, NixStore};
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(name = "circus-server")]
#[command(about = "CI Server - Web API and UI")]
struct Cli {
  #[arg(short, long)]
  config: Option<PathBuf>,

  #[arg(short = 'H', long)]
  host: Option<String>,

  #[arg(short, long)]
  port: Option<u16>,

  /// Run API/public routes only; do not mount bundled dashboard HTML or
  /// assets.
  #[arg(long, conflicts_with = "ui")]
  headless: bool,

  /// Force mounting the bundled dashboard UI even if config disables it.
  #[arg(long)]
  ui: bool,
}

#[expect(
  clippy::expect_used,
  reason = "fatal if the runtime cannot deliver signals"
)]
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

  tracing::info!("Shutdown signal received");
}

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
  color_eyre::install()?;
  circus_common::install_crypto_provider()?;

  let cli = Cli::parse();

  let mut config = Config::load(cli.config.as_deref())?;
  circus_common::init_tracing(&config.tracing);

  let host = cli.host.unwrap_or_else(|| config.server.host.clone());
  let port = cli.port.unwrap_or(config.server.port);
  if cli.headless {
    config.ui.enabled = false;
  } else if cli.ui {
    config.ui.enabled = true;
  }

  circus_common::validate::warn_insecure_schemes(
    &config.server.allowed_url_schemes,
  );

  if config.cache.secret_key_file.is_some() {
    tracing::warn!(
      "[cache] secret_key_file no longer signs narinfos on the fly; configure \
       [signing] on the queue-runner so outputs are signed at build time"
    );
  }

  let db = Database::new(config.database.clone()).await?;

  // Bootstrap declarative projects, jobsets, and API keys from config.
  // Notification secrets are validated and encrypted here, before bootstrap
  // stores the config blobs verbatim, so circus-common needs no dependency on
  // circus-notification.
  let mut declarative = config.declarative.clone();
  circus_notification::encrypt_declarative_notifications(
    &mut declarative,
    config.server.webhook_secret_encryption_key.as_deref(),
  )?;
  circus_common::bootstrap::run(
    db.pool(),
    &declarative,
    config.server.webhook_secret_encryption_key.as_deref(),
  )
  .await?;

  // Per-process CSRF secret. Concatenating two v4 UUIDs gives 32 bytes of
  // entropy from the system CSPRNG with no extra dependency.
  let mut csrf_secret = [0u8; 32];
  csrf_secret[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
  csrf_secret[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());

  let email_regex = config
    .server
    .email_validation_regex
    .as_deref()
    .map(|pat| {
      regex::Regex::new(pat).map(Arc::new).map_err(|e| {
        color_eyre::eyre::eyre!("Invalid email_validation_regex: {e}")
      })
    })
    .transpose()?;

  let nix_store = NixStore::new(config.nix.store_dir.clone())
    .map_err(|e| color_eyre::eyre::eyre!(e))?;

  let state = AppState {
    pool: db.pool().clone(),
    nix_store,
    config: config.clone(),
    sessions: Arc::new(dashmap::DashMap::new()),
    narinfo_cache: AppState::new_narinfo_cache(),
    http_client: reqwest::Client::new(),
    csrf_secret: Arc::new(csrf_secret),
    email_regex,
  };

  // Start background session cleanup to prevent memory leaks
  state.spawn_session_cleanup();

  let app = routes::router(state, &config);

  let bind_addr = format!("{host}:{port}");
  tracing::info!(
    mode = if config.ui.enabled {
      "full"
    } else {
      "headless"
    },
    "Starting CI Server on {}",
    bind_addr
  );

  let listener = TcpListener::bind(&bind_addr).await?;
  let app = app.into_make_service_with_connect_info::<SocketAddr>();
  axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;

  tracing::info!("Server shutting down, closing database pool");
  db.close().await;

  Ok(())
}
