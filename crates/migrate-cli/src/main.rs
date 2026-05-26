//! Database migration CLI utility

use circus_common::migrate_cli::run;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
  run().await
}
