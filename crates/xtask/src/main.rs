//! Workspace task runner.

use std::process::ExitCode;

use clap::{Parser, Subcommand};

mod openapi_check;

#[derive(Parser)]
#[command(name = "xtask", about = "Circus workspace tasks")]
struct Cli {
  #[command(subcommand)]
  command: Cmd,
}

#[derive(Subcommand)]
enum Cmd {
  /// Verify that every API route registered in the server has a matching
  /// entry in the hand-written `OpenAPI` document.
  OpenapiCheck,
}

fn main() -> ExitCode {
  #![expect(clippy::print_stderr, reason = "xtask error output is intentional")]
  let cli = Cli::parse();
  let result = match cli.command {
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
