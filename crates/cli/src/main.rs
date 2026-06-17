#![expect(
  clippy::print_stdout,
  reason = "circusctl is a CLI and stdout is its user interface"
)]

mod app;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
  color_eyre::install()?;
  circus_common::install_crypto_provider()?;
  app::run().await
}
