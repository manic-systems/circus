#![expect(
  clippy::print_stdout,
  reason = "circusctl is a CLI and stdout is its user interface"
)]
#![expect(
  clippy::redundant_pub_crate,
  reason = "CLI internals are split across private sibling modules"
)]

mod admin;
mod app;
mod client;
mod commands;
mod output;

#[tokio::main]
async fn main() -> color_eyre::Result<()> {
  color_eyre::install()?;
  circus_common::install_crypto_provider()?;
  app::run().await
}
