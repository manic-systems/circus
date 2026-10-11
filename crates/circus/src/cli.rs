#![expect(
  clippy::print_stdout,
  reason = "circus is a CLI and stdout is its user interface"
)]
use std::{env, ffi::OsString, path::Path};

use pound::{ErrorKind, Parse};

/// Run the multi-call Circus CLI.
///
/// # Errors
///
/// Returns an error from the selected applet.
pub fn run() -> misstep::Result<()> {
  let mut args = env::args_os().collect::<Vec<_>>();

  if env::var_os(evix::WORKER_ENV).is_some() {
    return circus_evaluator::cli::run_from(args);
  }
  if args.get(1).is_some_and(|arg| arg == "--circus-sandbox") {
    return circus_agent::cli::run_from(args);
  }

  let invoked = args
    .first()
    .and_then(|arg| Path::new(arg).file_name()?.to_str())
    .and_then(|name| Applet::try_parse_from([name]).ok());
  if let Some(applet) = invoked {
    return applet.run(args);
  }

  let command = args.get(1).and_then(|arg| arg.to_str());
  let applet = match Applet::try_parse_from(command) {
    Ok(applet) => applet,
    Err(error) if matches!(error.kind, ErrorKind::Version(_)) => {
      println!("circus {}", circus_common::version::long());
      return Ok(());
    },
    Err(error) => error.exit(),
  };

  args.remove(1);
  args[0] = OsString::from(applet.name());
  applet.run(args)
}

/// Circus services and client, also run by their own names via symlinks.
#[derive(Clone, Copy, Parse)]
#[pound(name = "circus", help_if_empty)]
enum Applet {
  /// Run the API server and dashboard
  #[pound(alias = "circus-server")]
  Server,
  /// Run the build queue runner
  #[pound(alias = ["runner", "circus-queue-runner"])]
  QueueRunner,
  /// Run the evaluator
  #[pound(alias = "circus-evaluator")]
  Evaluator,
  /// Run a distributed build agent
  #[pound(alias = "circus-agent")]
  Agent,
  /// Run the API client CLI
  #[pound(alias = ["cli", "circusctl", "circus-cli"])]
  Ctl,
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

  fn run(self, args: Vec<OsString>) -> misstep::Result<()> {
    match self {
      Self::Agent => circus_agent::cli::run_from(args),
      Self::Ctl => run_ctl(args),
      Self::Evaluator => circus_evaluator::cli::run_from(args),
      Self::QueueRunner => circus_queue_runner::cli::run_from(args),
      Self::Server => circus_server::cli::run_from(args),
    }
  }
}

fn run_ctl(args: Vec<OsString>) -> misstep::Result<()> {
  misstep::install_panic_hook();
  circus_common::install_crypto_provider()?;

  let runtime = tokio::runtime::Builder::new_multi_thread()
    .enable_all()
    .build()?;
  runtime.block_on(circus_cli::run_from(args))
}
