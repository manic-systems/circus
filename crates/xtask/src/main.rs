//! Workspace task runner.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod api_docs;
mod openapi_check;

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
  /// entry in the hand-written `OpenAPI` document.
  OpenapiCheck,
}

fn main() -> ExitCode {
  #![expect(clippy::print_stderr, reason = "xtask error output is intentional")]
  if let Err(e) = color_eyre::install() {
    eprintln!("failed to install color-eyre reporter: {e}");
    return ExitCode::FAILURE;
  }
  let cli = Cli::parse();
  let result = match cli.command {
    Cmd::ApiDocs { check } => api_docs::run(check),
    Cmd::OpenapiCheck => openapi_check::run(),
  };
  match result {
    Ok(()) => ExitCode::SUCCESS,
    Err(e) => {
      eprintln!("xtask failed: {e:#}");
      ExitCode::FAILURE
    },
  }
}
