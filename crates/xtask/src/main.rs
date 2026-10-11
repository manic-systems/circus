//! Workspace task runner.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod api_docs;
mod openapi_check;
mod preview_frontend;

#[derive(Parser)]
#[command(name = "xtask", about = "Circus workspace tasks")]
struct Cli {
  #[command(subcommand)]
  command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
  /// Generate docs/API.md from the registered server routes.
  ApiDocs {
    /// Verify docs/API.md is current without rewriting it.
    #[arg(long)]
    check: bool,
  },
  /// Verify that every API route registered in the server has a matching
  /// entry in the generated `OpenAPI` document.
  OpenapiCheck,
  /// Serve the dashboard against a throwaway, seeded Postgres.
  PreviewFrontend {
    /// Address to bind.
    #[arg(long, default_value_t = preview_frontend::default_host())]
    host: std::net::IpAddr,
    /// Port to bind.
    #[arg(long, default_value_t = 3001)]
    port: u16,
  },
}

#[tokio::main]
async fn main() -> ExitCode {
  #![expect(clippy::print_stderr, reason = "xtask error output is intentional")]
  misstep::install_panic_hook();
  let cli = Cli::parse();
  let result = match cli.command {
    Cmd::ApiDocs { check } => api_docs::run(check),
    Cmd::OpenapiCheck => openapi_check::run(),
    Cmd::PreviewFrontend { host, port } => {
      preview_frontend::run(host, port).await
    },
  };
  match result {
    Ok(()) => ExitCode::SUCCESS,
    Err(e) => {
      eprintln!("xtask failed: {e}");
      ExitCode::FAILURE
    },
  }
}
