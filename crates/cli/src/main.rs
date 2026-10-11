#[tokio::main]
async fn main() -> misstep::Result<()> {
  misstep::install_panic_hook();
  circus_common::install_crypto_provider()?;
  circus_cli::run().await
}
