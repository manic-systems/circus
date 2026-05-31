//! Database migration CLI utility

use circus_common::migrate_cli::run;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
  color_eyre::install()?;
  run().await
}
