use clap::Parser;
use fc_common::{Config, Database};
use fc_server::{routes, state};
use state::AppState;
use tokio::net::TcpListener;

#[derive(Parser)]
#[command(name = "fc-server")]
#[command(about = "CI Server - Web API and UI")]
struct Cli {
  #[arg(short = 'H', long)]
  host: Option<String>,

  #[arg(short, long)]
  port: Option<u16>,
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

  tracing::info!("Shutdown signal received");
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  let config = Config::load()?;
  fc_common::init_tracing(&config.tracing);

  let cli = Cli::parse();

  let host = cli.host.unwrap_or(config.server.host.clone());
  let port = cli.port.unwrap_or(config.server.port);

  fc_common::validate::warn_insecure_schemes(
    &config.server.allowed_url_schemes,
  );

  let db = Database::new(config.database.clone()).await?;

  // Bootstrap declarative projects, jobsets, and API keys from config
  fc_common::bootstrap::run(db.pool(), &config.declarative).await?;

  // Per-process CSRF secret. Concatenating two v4 UUIDs gives 32 bytes of
  // entropy from the system CSPRNG with no extra dependency.
  let mut csrf_secret = [0u8; 32];
  csrf_secret[..16].copy_from_slice(uuid::Uuid::new_v4().as_bytes());
  csrf_secret[16..].copy_from_slice(uuid::Uuid::new_v4().as_bytes());

  let state = AppState {
    pool:        db.pool().clone(),
    config:      config.clone(),
    sessions:    std::sync::Arc::new(dashmap::DashMap::new()),
    http_client: reqwest::Client::new(),
    csrf_secret: std::sync::Arc::new(csrf_secret),
  };

  // Start background session cleanup to prevent memory leaks
  state.spawn_session_cleanup();

  let app = routes::router(state, &config.server);

  let bind_addr = format!("{host}:{port}");
  tracing::info!("Starting CI Server on {}", bind_addr);

  let listener = TcpListener::bind(&bind_addr).await?;
  let app = app.into_make_service_with_connect_info::<std::net::SocketAddr>();
  axum::serve(listener, app)
    .with_graceful_shutdown(shutdown_signal())
    .await?;

  tracing::info!("Server shutting down, closing database pool");
  db.close().await;

  Ok(())
}
