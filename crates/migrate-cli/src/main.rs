//! Database migration CLI utility

use circus_common::migrate_cli::run;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
  color_eyre::install()?;
  circus_common::install_crypto_provider()?;
  run().await
}
