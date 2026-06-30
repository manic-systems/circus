#[tokio::main]
async fn main() -> color_eyre::Result<()> {
  color_eyre::install()?;
  circus_common::install_crypto_provider()?;
  circus_cli::run().await
}
