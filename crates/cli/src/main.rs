use pound::Parse;

#[tokio::main]
async fn main() -> misstep::Result<()> {
  caress::handle(circus_cli::Cli::SPEC);
  misstep::install_panic_hook();
  circus_common::install_crypto_provider()?;
  circus_cli::run().await
}
