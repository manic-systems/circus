//! Database migration CLI utility

use circus_common::migrate_cli::run;
use circus_logs::TracingConfig;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
  color_eyre::install()?;
  circus_common::install_crypto_provider()?;
  circus_logs::init_tracing(&TracingConfig::default());
  run().await
}
