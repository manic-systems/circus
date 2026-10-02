//! `circus-agent effect run`, the local counterpart of `hci effect run`.

use std::{
  fs::OpenOptions,
  io::Write as _,
  os::unix::fs::OpenOptionsExt as _,
  path::PathBuf,
  time::Duration,
};

use capnp::capability::Promise;
use circus_proto::log_sink;
use clap::Args;
use color_eyre::eyre::{Context as _, Result, bail};
use tokio_util::sync::CancellationToken;

use crate::{
  config::EffectsConfig,
  effect::{self, ClosureSource, EffectContext, RunOptions},
  sandbox::{self, NixTool},
};

#[derive(Args)]
pub struct EffectRunArgs {
  /// Flake installable of the effect, e.g. `.#effects.deploy`.
  installable: String,

  /// Hercules-format secrets.json to resolve `secretsMap` against.
  #[arg(long)]
  secrets_file: Option<PathBuf>,

  /// Repository owner the secret conditions see.
  #[arg(long, default_value = "local")]
  owner: String,

  /// Repository name the secret conditions see.
  #[arg(long, default_value = "local")]
  repo: String,

  /// Pretend the run is for this branch.
  #[arg(long, conflicts_with = "pretend_tag")]
  pretend_branch: Option<String>,

  /// Pretend the run is for this tag.
  #[arg(long)]
  pretend_tag: Option<String>,

  /// Treat the pretended branch as the default branch.
  #[arg(long, requires = "pretend_branch")]
  default_branch: bool,

  /// Circus server URL exposed as `HERCULES_CI_API_BASE_URL`.
  #[arg(long)]
  api_url: Option<String>,

  /// Task token handed to the effect, e.g. for state files.
  #[arg(long, requires = "api_url")]
  token: Option<String>,

  /// Effect timeout in seconds.
  #[arg(long, default_value_t = 3600)]
  timeout: u64,
}

struct StdoutLogSink;

#[expect(
  refining_impl_trait_internal,
  reason = "capnp-rpc declares these methods with opaque future returns"
)]
impl log_sink::Server for StdoutLogSink {
  fn write(
    self: capnp::capability::Rc<Self>,
    params: log_sink::WriteParams,
    _results: log_sink::WriteResults,
  ) -> Promise<(), capnp::Error> {
    let chunk = match params
      .get()
      .and_then(log_sink::write_params::Reader::get_chunk)
    {
      Ok(chunk) => chunk,
      Err(e) => return Promise::err(e),
    };
    let mut stdout = std::io::stdout().lock();
    match stdout
      .write_all(chunk)
      .and_then(|()| stdout.write_all(b"\n"))
    {
      Ok(()) => Promise::ok(()),
      Err(e) => Promise::err(capnp::Error::failed(format!("stdout: {e}"))),
    }
  }

  fn close(
    self: capnp::capability::Rc<Self>,
    _params: log_sink::CloseParams,
    _results: log_sink::CloseResults,
  ) -> Promise<(), capnp::Error> {
    Promise::ok(())
  }
}

async fn nix_output(tool: NixTool, args: &[&str]) -> Result<String> {
  let mut cmd = sandbox::nix_command(false, tool)?;
  let output = cmd.args(args).output().await.context("run nix")?;
  if !output.status.success() {
    bail!(
      "{} failed: {}",
      args.join(" "),
      String::from_utf8_lossy(&output.stderr).trim()
    );
  }
  Ok(String::from_utf8(output.stdout)?.trim().to_owned())
}

/// Realise every input of the effect locally, then run it like an agent would.
///
/// # Errors
///
/// Returns an error when evaluation, realisation or effect preparation fails.
/// A failing effect is reported through the returned exit code.
pub async fn run(args: EffectRunArgs) -> Result<i32> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let drv_path = nix_output(NixTool::Nix, &[
    "eval",
    "--raw",
    &format!("{}.drvPath", args.installable),
  ])
  .await?;
  let references =
    nix_output(NixTool::NixStore, &["--query", "--references", &drv_path])
      .await?;
  let mut build = vec!["build", "--no-link"];
  let input_drvs = references
    .lines()
    .filter(|path| {
      std::path::Path::new(path)
        .extension()
        .is_some_and(|extension| extension == "drv")
    })
    .map(|path| format!("{path}^*"))
    .collect::<Vec<_>>();
  build.extend(input_drvs.iter().map(String::as_str));
  if !input_drvs.is_empty() {
    nix_output(NixTool::Nix, &build).await?;
  }

  let work_dir = tempfile::tempdir().context("create effect work directory")?;
  let secrets_file = if let Some(path) = args.secrets_file {
    path
  } else {
    let path = work_dir.path().join("no-secrets.json");
    OpenOptions::new()
      .write(true)
      .create_new(true)
      .mode(0o600)
      .open(&path)?
      .write_all(b"{}")?;
    path
  };
  let config = EffectsConfig {
    secrets_file,
    allow_insecure_transport: false,
  };
  let branch = args.pretend_branch.unwrap_or_default();
  let context = EffectContext {
    project_id: "local".into(),
    project_path: format!("local/{}/{}", args.owner, args.repo),
    api_base_url: args.api_url.unwrap_or_default(),
    owner: args.owner,
    repo: args.repo,
    is_default_branch: args.default_branch && !branch.is_empty(),
    branch,
    tag: args.pretend_tag.unwrap_or_default(),
    task_token: args.token.unwrap_or_default(),
  };
  let log: log_sink::Client = capnp_rpc::new_client(StdoutLogSink);
  let result = effect::run(
    RunOptions {
      drv_path: &drv_path,
      max_log_size: u64::MAX,
      max_silent_time: Duration::ZERO,
      build_timeout: Duration::from_secs(args.timeout),
      cores: 0,
      closure_source: ClosureSource::LocalStore,
      rootless: false,
      work_dir: work_dir.path(),
      config: &config,
      context,
    },
    log,
    CancellationToken::new(),
  )
  .await?;

  if result.outcome == circus_proto::BuildOutcome::Success {
    return Ok(0);
  }
  tracing::error!(
    outcome = ?result.outcome,
    exit_code = result.exit_code,
    "effect failed: {}",
    result.error_message
  );
  Ok(if result.exit_code > 0 {
    result.exit_code
  } else {
    1
  })
}
