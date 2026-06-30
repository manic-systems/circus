#![expect(
  clippy::print_stdout,
  reason = "circus is a CLI and stdout is its user interface"
)]
use std::{
  env,
  ffi::{OsStr, OsString},
  path::Path,
};

/// Run the multi-call Circus CLI.
///
/// # Errors
///
/// Returns an error from the selected applet, or when the command name is not
/// recognized.
pub fn run() -> color_eyre::Result<()> {
  let args = env::args_os().collect::<Vec<_>>();

  if env::var_os(evix::WORKER_ENV).is_some() {
    return circus_evaluator::cli::run_from(args);
  }
  if args.get(1).is_some_and(|arg| arg == "--circus-sandbox") {
    return circus_agent::cli::run_from(args);
  }

  let applet = args
    .first()
    .and_then(|arg| Path::new(arg).file_name())
    .and_then(applet_from_name);

  match applet {
    Some(Applet::Agent) => circus_agent::cli::run_from(args),
    Some(Applet::Ctl) => run_ctl(args),
    Some(Applet::Evaluator) => circus_evaluator::cli::run_from(args),
    Some(Applet::QueueRunner) => circus_queue_runner::cli::run_from(args),
    Some(Applet::Server) => circus_server::cli::run_from(args),
    None => run_subcommand(args),
  }
}

fn run_subcommand(mut args: Vec<OsString>) -> color_eyre::Result<()> {
  let Some(command) = args.get(1).and_then(|arg| arg.to_str()) else {
    print_help();
    return Ok(());
  };

  if matches!(command, "-h" | "--help") {
    print_help();
    return Ok(());
  }
  if matches!(command, "-V" | "--version") {
    println!("circus {}", env!("CARGO_PKG_VERSION"));
    return Ok(());
  }

  let applet = applet_from_command(command).ok_or_else(|| {
    color_eyre::eyre::eyre!(
      "unknown circus command `{command}`; run `circus --help`"
    )
  })?;
  args.remove(1);
  args[0] = OsString::from(applet.name());

  match applet {
    Applet::Agent => circus_agent::cli::run_from(args),
    Applet::Ctl => run_ctl(args),
    Applet::Evaluator => circus_evaluator::cli::run_from(args),
    Applet::QueueRunner => circus_queue_runner::cli::run_from(args),
    Applet::Server => circus_server::cli::run_from(args),
  }
}

fn run_ctl(args: Vec<OsString>) -> color_eyre::Result<()> {
  color_eyre::install()?;
  circus_common::install_crypto_provider()?;

  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?;
  runtime.block_on(circus_cli::run_from(args))
}

fn print_help() {
  println!(
    "\
Usage: circus <COMMAND> [ARGS]...

Commands:
  server        Run the API server and dashboard
  queue-runner  Run the build queue runner
  evaluator     Run the evaluator
  agent         Run a distributed build agent
  ctl           Run the API client CLI

The same binary also dispatches by argv0 when installed as circus-server,
circus-queue-runner, circus-evaluator, circus-agent, or circusctl."
  );
}

#[derive(Clone, Copy)]
enum Applet {
  Agent,
  Ctl,
  Evaluator,
  QueueRunner,
  Server,
}

impl Applet {
  const fn name(self) -> &'static str {
    match self {
      Self::Agent => "circus-agent",
      Self::Ctl => "circusctl",
      Self::Evaluator => "circus-evaluator",
      Self::QueueRunner => "circus-queue-runner",
      Self::Server => "circus-server",
    }
  }
}

fn applet_from_name(name: &OsStr) -> Option<Applet> {
  match name.to_str()? {
    "agent" | "circus-agent" => Some(Applet::Agent),
    "ctl" | "circus-cli" | "circusctl" => Some(Applet::Ctl),
    "evaluator" | "circus-evaluator" => Some(Applet::Evaluator),
    "queue-runner" | "circus-queue-runner" => Some(Applet::QueueRunner),
    "server" | "circus-server" => Some(Applet::Server),
    _ => None,
  }
}

fn applet_from_command(command: &str) -> Option<Applet> {
  match command {
    "agent" => Some(Applet::Agent),
    "ctl" | "cli" | "circusctl" => Some(Applet::Ctl),
    "evaluator" => Some(Applet::Evaluator),
    "queue-runner" | "runner" => Some(Applet::QueueRunner),
    "server" => Some(Applet::Server),
    _ => None,
  }
}
