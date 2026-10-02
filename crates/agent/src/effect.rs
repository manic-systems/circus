//! Hercules-compatible post-build effect execution.

use std::{
  collections::{BTreeMap, BTreeSet},
  ffi::OsStr,
  fs::{self, OpenOptions},
  future::Future,
  io::{self, BufWriter, Write as _},
  os::unix::fs::{OpenOptionsExt as _, PermissionsExt as _},
  path::{Component, Path, PathBuf},
  process::Stdio,
  time::{Duration, Instant},
};

use circus_proto::log_sink;
use color_eyre::eyre::{Context as _, bail, eyre};
use nix::unistd::Uid;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::{Map, Value};
use sha2::{Digest as _, Sha256};
use tokio::process::Command;
use tokio_util::sync::CancellationToken;

use crate::{
  build::{self, BuildOptions, LocalResult, Tunables},
  config::EffectsConfig,
  sandbox::{self, EffectSandboxOptions, EffectView, NixTool},
};

const STORE_PREFIX: &str = "/nix/store/";
const MAX_CONDITION_DEPTH: usize = 64;
const MIN_REDACTION_LEN: usize = 8;
/// hercules-ci-effects reads the API token from this `secrets.json` key.
const TASK_TOKEN_SECRET: &str = "hercules-ci";

#[derive(Clone)]
pub struct EffectContext {
  pub project_id:        String,
  pub project_path:      String,
  pub api_base_url:      String,
  pub owner:             String,
  pub repo:              String,
  pub branch:            String,
  pub tag:               String,
  pub is_default_branch: bool,
  /// Empty when the runner issued no token, e.g. for local runs.
  pub task_token:        String,
}

pub struct RunOptions<'a> {
  pub drv_path:          &'a str,
  pub max_log_size:      u64,
  pub max_silent_time:   Duration,
  pub build_timeout:     Duration,
  pub cores:             u32,
  pub cache_substituter: String,
  pub cache_public_key:  String,
  pub rootless:          bool,
  pub work_dir:          &'a Path,
  pub config:            &'a EffectsConfig,
  pub context:           EffectContext,
}

#[derive(Deserialize)]
struct Derivation {
  args:             Vec<String>,
  builder:          String,
  env:              BTreeMap<String, String>,
  #[serde(default, rename = "structuredAttrs")]
  structured_attrs: Map<String, Value>,
  #[serde(default)]
  inputs:           DerivationInputs,
  #[serde(default, rename = "inputDrvs")]
  input_drvs:       BTreeMap<String, InputDerivation>,
  #[serde(default, rename = "inputSrcs")]
  input_srcs:       Vec<String>,
  #[serde(default)]
  outputs:          BTreeMap<String, DerivationOutput>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum DerivationShow {
  Wrapped {
    derivations: BTreeMap<String, Derivation>,
  },
  Legacy(BTreeMap<String, Derivation>),
}

#[derive(Default, Deserialize)]
struct DerivationInputs {
  #[serde(default)]
  drvs: BTreeMap<String, InputDerivation>,
  #[serde(default)]
  srcs: Vec<String>,
}

struct InputDerivation {
  outputs: Vec<String>,
}

#[derive(Deserialize)]
#[serde(untagged)]
enum InputDerivationRepr {
  Object {
    #[serde(default)]
    outputs: Vec<String>,
  },
  List(Vec<String>),
}

impl<'de> Deserialize<'de> for InputDerivation {
  fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
  where
    D: Deserializer<'de>,
  {
    let outputs = match InputDerivationRepr::deserialize(deserializer)? {
      InputDerivationRepr::Object { outputs }
      | InputDerivationRepr::List(outputs) => outputs,
    };
    Ok(Self { outputs })
  }
}

#[derive(Deserialize)]
struct DerivationOutput {
  path: Option<String>,
}

#[derive(Deserialize)]
struct SecretDefinition {
  kind:      String,
  data:      Map<String, Value>,
  #[serde(default)]
  condition: Option<Value>,
}

#[derive(Serialize)]
struct ResolvedSecret {
  kind: &'static str,
  data: Map<String, Value>,
}

enum SecretReference {
  Local(String),
  Unsupported,
}

enum Condition {
  Or(Vec<Self>),
  And(Vec<Self>),
  IsDefaultBranch,
  IsBranch(String),
  IsTag,
  IsRepo(String),
  IsOwner(String),
  Const(bool),
}

struct PreparedEffect {
  task_dir:     tempfile::TempDir,
  build_dir:    PathBuf,
  secrets_file: PathBuf,
  redactions:   Vec<String>,
}

struct PreparedCommand<R> {
  command:    Command,
  redactions: Vec<String>,
  resources:  R,
}

enum PreparationOutcome<T> {
  Ready(T),
  Interrupted(LocalResult),
}

/// Validate the opt-in secrets configuration before advertising effects.
///
/// # Errors
///
/// Returns an error when the file is missing, unsafe, or malformed.
pub fn validate_config(config: &EffectsConfig) -> color_eyre::Result<()> {
  let metadata = fs::metadata(&config.secrets_file).with_context(|| {
    format!(
      "read effects secrets file {}",
      config.secrets_file.display()
    )
  })?;
  if !metadata.is_file() {
    bail!("effects secrets path must name a regular file");
  }
  if metadata.permissions().mode() & 0o077 != 0 {
    bail!("effects secrets file must not be accessible by group or others");
  }
  let secrets = load_secret_definitions(&config.secrets_file)?;
  for (name, secret) in secrets {
    if secret.kind != "Secret" {
      bail!("secret '{name}' has an unsupported kind");
    }
    if let Some(value) = secret.condition {
      parse_condition(&value, 0)
        .map_err(|e| eyre!("secret '{name}' has an invalid condition: {e}"))?;
    }
  }
  Ok(())
}

/// Validate both the secrets file and the host identity used for effects.
///
/// # Errors
///
/// Returns an error when effects would run with real root privileges.
pub fn validate_runtime(
  config: &EffectsConfig,
  rootless: bool,
) -> color_eyre::Result<()> {
  if Uid::effective().is_root() {
    bail!("effects refuse to run when circus-agent is root");
  }
  validate_mode(rootless)?;
  validate_config(config)
}

fn validate_mode(rootless: bool) -> color_eyre::Result<()> {
  if rootless {
    bail!(
      "effects do not support rootless mode because the persistent direct \
       store cannot be exposed safely"
    );
  }
  Ok(())
}

fn validate_cache_trust(opts: &RunOptions<'_>) -> color_eyre::Result<()> {
  validate_cache_settings(&opts.cache_substituter, &opts.cache_public_key)
}

fn validate_cache_settings(
  substituter: &str,
  public_key: &str,
) -> color_eyre::Result<()> {
  if substituter.trim().is_empty() {
    bail!("effects require a configured cache_substituter");
  }
  if public_key.trim().is_empty() {
    bail!(
      "effects require cache_public_key when cache_substituter is configured"
    );
  }
  Ok(())
}

/// Run an effect's derivation builder in the relaxed effect sandbox.
///
/// # Errors
///
/// Returns preparation and process-spawn failures. Process exit failures are
/// represented in [`LocalResult`].
pub async fn run(
  opts: RunOptions<'_>,
  log_sink: log_sink::Client,
  cancel: CancellationToken,
) -> color_eyre::Result<LocalResult> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let started = Instant::now();
  let prepared = supervise_preparation(
    prepare_effect(&opts),
    cancel.clone(),
    opts.build_timeout,
  )
  .await?;
  let prepared = match prepared {
    PreparationOutcome::Ready(prepared) => prepared,
    PreparationOutcome::Interrupted(mut result) => {
      result.build_time_ms = started.elapsed().as_millis() as u64;
      return Ok(result);
    },
  };

  if cancel.is_cancelled() {
    let mut result = interrupted_effect(
      circus_proto::BuildOutcome::Aborted,
      "aborted by runner",
    );
    result.build_time_ms = started.elapsed().as_millis() as u64;
    return Ok(result);
  }
  let execution_timeout = if opts.build_timeout.is_zero() {
    Duration::ZERO
  } else {
    let Some(remaining) = opts.build_timeout.checked_sub(started.elapsed())
    else {
      let mut result = interrupted_effect(
        circus_proto::BuildOutcome::TimedOut,
        "build-timeout exceeded",
      );
      result.build_time_ms = started.elapsed().as_millis() as u64;
      return Ok(result);
    };
    if remaining.is_zero() {
      let mut result = interrupted_effect(
        circus_proto::BuildOutcome::TimedOut,
        "build-timeout exceeded",
      );
      result.build_time_ms = started.elapsed().as_millis() as u64;
      return Ok(result);
    }
    remaining
  };

  let mut result =
    execute_prepared(prepared, &opts, execution_timeout, log_sink, cancel)
      .await?;
  result.build_time_ms = started.elapsed().as_millis() as u64;
  Ok(result)
}

async fn prepare_effect(
  opts: &RunOptions<'_>,
) -> color_eyre::Result<PreparedCommand<tempfile::TempDir>> {
  validate_runtime(opts.config, opts.rootless)?;
  validate_cache_trust(opts)?;
  fetch_derivation(opts).await?;
  let derivation = load_derivation(opts).await?;
  let structured_attrs = effective_structured_attrs(&derivation)?;
  ensure_effect(&derivation, structured_attrs.as_ref())?;
  let closure_paths = fetch_input_closure(opts, &derivation).await?;

  let prepared = prepare_secrets(
    opts.work_dir,
    &opts.config.secrets_file,
    &derivation.env,
    structured_attrs.as_ref(),
    &derivation.outputs,
    &opts.context,
  )?;
  let fs_root = effect_fs_root(&derivation.env, structured_attrs.as_ref())
    .map(PathBuf::from);
  if fs_root.is_some() && !cfg!(target_os = "linux") {
    bail!(
      "mkEffect and modularEffect assume the Linux effect sandbox (/build, \
       /etc/passwd); this agent only runs plain `isEffect` derivations"
    );
  }
  if let Some(path) = &fs_root {
    validate_effect_fs_root(path, &closure_paths)?;
  }
  let builder = Path::new(&derivation.builder);
  validate_effect_builder(builder, &closure_paths)?;
  let default_env = closure_paths
    .iter()
    .map(|path| Path::new(path).join("bin/env"))
    .find(|path| path.is_file());
  let view = sandbox::effect_view(&prepared.build_dir, &prepared.secrets_file)?;
  let env = effect_environment(
    opts,
    &derivation,
    &opts.context,
    &view,
    structured_attrs.is_some(),
    fs_root.is_some(),
  );
  let command = sandbox::effect_command(
    EffectSandboxOptions {
      rootless:      opts.rootless,
      build_dir:     &prepared.build_dir,
      secrets_file:  &prepared.secrets_file,
      fs_root:       fs_root.as_deref(),
      default_shell: builder,
      default_env:   default_env.as_deref(),
    },
    builder,
    &derivation.args,
    &env,
  )?;

  let PreparedEffect {
    task_dir,
    redactions,
    ..
  } = prepared;
  Ok(PreparedCommand {
    command,
    redactions,
    resources: task_dir,
  })
}

async fn execute_prepared<R>(
  prepared: PreparedCommand<R>,
  opts: &RunOptions<'_>,
  build_timeout: Duration,
  log_sink: log_sink::Client,
  cancel: CancellationToken,
) -> color_eyre::Result<LocalResult> {
  #![expect(
    clippy::future_not_send,
    reason = "capnp futures are not Send; agent uses a single-threaded runtime"
  )]
  let PreparedCommand {
    command,
    redactions,
    resources,
  } = prepared;
  let result = build::run_command(
    command,
    &BuildOptions {
      drv_path: opts.drv_path,
      max_log_size: opts.max_log_size,
      max_silent_time: opts.max_silent_time,
      build_timeout,
      cores: opts.cores,
      extra_args: Vec::new(),
      cache_substituter: opts.cache_substituter.clone(),
      cache_public_key: opts.cache_public_key.clone(),
      rootless: opts.rootless,
      collect_outputs: false,
      nix_internal_json: false,
      redactions,
    },
    Tunables::default(),
    log_sink,
    cancel,
  )
  .await;
  drop(resources);
  result
}

async fn supervise_preparation<F, T>(
  preparation: F,
  cancel: CancellationToken,
  timeout: Duration,
) -> color_eyre::Result<PreparationOutcome<T>>
where
  F: Future<Output = color_eyre::Result<T>>,
{
  tokio::pin!(preparation);
  if timeout.is_zero() {
    tokio::select! {
      biased;
      () = cancel.cancelled() => Ok(PreparationOutcome::Interrupted(
        interrupted_effect(
          circus_proto::BuildOutcome::Aborted,
          "aborted by runner",
        ),
      )),
      result = &mut preparation => result.map(PreparationOutcome::Ready),
    }
  } else {
    tokio::select! {
      biased;
      () = cancel.cancelled() => Ok(PreparationOutcome::Interrupted(
        interrupted_effect(
          circus_proto::BuildOutcome::Aborted,
          "aborted by runner",
        ),
      )),
      () = tokio::time::sleep(timeout) => Ok(PreparationOutcome::Interrupted(
        interrupted_effect(
          circus_proto::BuildOutcome::TimedOut,
          "build-timeout exceeded",
        ),
      )),
      result = &mut preparation => result.map(PreparationOutcome::Ready),
    }
  }
}

fn interrupted_effect(
  outcome: circus_proto::BuildOutcome,
  error_message: &str,
) -> LocalResult {
  LocalResult {
    outcome,
    exit_code: -1,
    build_time_ms: 0,
    upload_time_ms: 0,
    outputs: Vec::new(),
    error_message: error_message.into(),
  }
}

async fn fetch_derivation(opts: &RunOptions<'_>) -> color_eyre::Result<()> {
  if opts.cache_substituter.is_empty() {
    return Ok(());
  }
  nix_copy(
    opts.rootless,
    &opts.cache_substituter,
    &opts.cache_public_key,
    true,
    &[opts.drv_path.to_owned()],
  )
  .await
  .context("fetch effect derivation")
}

async fn load_derivation(
  opts: &RunOptions<'_>,
) -> color_eyre::Result<Derivation> {
  let mut cmd = sandbox::nix_command(opts.rootless, NixTool::Nix)?;
  cmd
    .args([
      "--extra-experimental-features",
      "nix-command",
      "derivation",
      "show",
      opts.drv_path,
    ])
    .kill_on_drop(true);
  let mut cmd = sandbox::wrap_command(opts.rootless, cmd)?;
  let output = cmd
    .stdin(Stdio::null())
    .output()
    .await
    .context("inspect effect derivation")?;
  if !output.status.success() {
    bail!("could not inspect effect derivation");
  }
  parse_derivation_json(&output.stdout, opts.drv_path)
}

fn parse_derivation_json(
  json: &[u8],
  drv_path: &str,
) -> color_eyre::Result<Derivation> {
  let show: DerivationShow =
    serde_json::from_slice(json).context("parse effect derivation JSON")?;
  let derivations = match show {
    DerivationShow::Wrapped { derivations }
    | DerivationShow::Legacy(derivations) => derivations,
  };
  let mut derivations = derivations
    .into_iter()
    .map(|(path, derivation)| {
      (
        normalize_store_reference(&path),
        normalize_derivation(derivation),
      )
    })
    .collect::<BTreeMap<_, _>>();
  if let Some(derivation) = derivations.remove(drv_path) {
    return Ok(derivation);
  }
  if derivations.len() == 1
    && let Some((_, derivation)) = derivations.pop_first()
  {
    return Ok(derivation);
  }
  bail!("Nix did not return the assigned effect derivation")
}

fn normalize_derivation(mut derivation: Derivation) -> Derivation {
  derivation
    .input_drvs
    .extend(std::mem::take(&mut derivation.inputs.drvs));
  derivation.input_drvs = derivation
    .input_drvs
    .into_iter()
    .map(|(path, input)| (normalize_store_reference(&path), input))
    .collect();
  derivation
    .input_srcs
    .extend(std::mem::take(&mut derivation.inputs.srcs));
  derivation.input_srcs = derivation
    .input_srcs
    .into_iter()
    .map(|path| normalize_store_reference(&path))
    .collect();
  derivation.builder = normalize_store_reference(&derivation.builder);
  for output in derivation.outputs.values_mut() {
    if let Some(path) = &mut output.path {
      *path = normalize_store_reference(path);
    }
  }
  derivation
}

fn normalize_store_reference(value: &str) -> String {
  if value.starts_with(STORE_PREFIX) || !looks_like_store_reference(value) {
    value.to_owned()
  } else {
    format!("{STORE_PREFIX}{value}")
  }
}

fn looks_like_store_reference(value: &str) -> bool {
  let name = value.split('/').next().unwrap_or_default();
  name.len() > 33
    && name.as_bytes().get(32) == Some(&b'-')
    && name.as_bytes()[..32]
      .iter()
      .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit())
}

fn effective_structured_attrs(
  derivation: &Derivation,
) -> color_eyre::Result<Option<Map<String, Value>>> {
  let mut attrs = if let Some(json) = derivation.env.get("__json") {
    let value: Value =
      serde_json::from_str(json).context("parse effect structured attrs")?;
    let object = value
      .as_object()
      .ok_or_else(|| eyre!("effect structured attrs must be a JSON object"))?;
    object.clone()
  } else {
    Map::new()
  };
  let present = derivation.env.contains_key("__json")
    || !derivation.structured_attrs.is_empty();
  if !derivation.structured_attrs.is_empty() {
    attrs.extend(derivation.structured_attrs.clone());
  }
  Ok(present.then_some(attrs))
}

fn ensure_effect(
  derivation: &Derivation,
  structured_attrs: Option<&Map<String, Value>>,
) -> color_eyre::Result<()> {
  if derivation
    .env
    .get("isEffect")
    .is_some_and(|value| matches!(value.as_str(), "1" | "true"))
    || structured_attrs
      .and_then(|attrs| attrs.get("isEffect"))
      .is_some_and(effect_marker)
  {
    Ok(())
  } else {
    bail!("assigned derivation is not marked as an effect")
  }
}

fn effect_fs_root(
  env: &BTreeMap<String, String>,
  structured_attrs: Option<&Map<String, Value>>,
) -> Option<String> {
  env
    .get("__hci_effect_fsroot_copy")
    .filter(|path| !path.is_empty())
    .cloned()
    .or_else(|| {
      structured_attrs
        .and_then(|attrs| attrs.get("__hci_effect_fsroot_copy"))
        .and_then(Value::as_str)
        .filter(|path| !path.is_empty())
        .map(str::to_owned)
    })
}

fn effect_marker(value: &Value) -> bool {
  match value {
    Value::Bool(value) => *value,
    Value::Number(value) => value.as_i64() == Some(1),
    Value::String(value) => matches!(value.as_str(), "1" | "true"),
    _ => false,
  }
}

async fn fetch_input_closure(
  opts: &RunOptions<'_>,
  derivation: &Derivation,
) -> color_eyre::Result<BTreeSet<String>> {
  let input_drvs = derivation.input_drvs.keys().cloned().collect::<Vec<_>>();
  if !opts.cache_substituter.is_empty() {
    for chunk in input_drvs.chunks(128) {
      nix_copy(
        opts.rootless,
        &opts.cache_substituter,
        &opts.cache_public_key,
        true,
        chunk,
      )
      .await?;
    }
  }

  let mut paths = referenced_store_paths(derivation);
  paths
    .extend(query_input_outputs(opts.rootless, &derivation.input_drvs).await?);
  if !opts.cache_substituter.is_empty() {
    let copy_paths = paths.iter().cloned().collect::<Vec<_>>();
    for chunk in copy_paths.chunks(128) {
      nix_copy(
        opts.rootless,
        &opts.cache_substituter,
        &opts.cache_public_key,
        false,
        chunk,
      )
      .await?;
    }
  }
  Ok(paths)
}

async fn nix_copy(
  rootless: bool,
  substituter: &str,
  public_key: &str,
  derivation: bool,
  paths: &[String],
) -> color_eyre::Result<()> {
  if paths.is_empty() {
    return Ok(());
  }
  let cmd =
    nix_copy_command(rootless, substituter, public_key, derivation, paths)?;
  let mut cmd = sandbox::wrap_command(rootless, cmd)?;
  let output = cmd
    .stdin(Stdio::null())
    .output()
    .await
    .context("copy effect inputs from cache")?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    let detail = stderr
      .trim()
      .replace(substituter, "<cache>")
      .chars()
      .take(4096)
      .collect::<String>();
    if detail.is_empty() {
      bail!("effect input closure is unavailable from the configured cache");
    }
    bail!(
      "effect input closure is unavailable from the configured cache: {detail}"
    );
  }
  Ok(())
}

fn nix_copy_command(
  rootless: bool,
  substituter: &str,
  public_key: &str,
  derivation: bool,
  paths: &[String],
) -> color_eyre::Result<tokio::process::Command> {
  if substituter.trim().is_empty() {
    bail!("effects require a configured cache_substituter");
  }
  if public_key.trim().is_empty() {
    bail!(
      "effects require cache_public_key when cache_substituter is configured"
    );
  }
  let mut cmd = sandbox::nix_command(rootless, NixTool::Nix)?;
  cmd.args([
    "--extra-experimental-features",
    "nix-command",
    "--option",
    "extra-trusted-public-keys",
    public_key,
    "copy",
  ]);
  if derivation {
    cmd.arg("--derivation");
  }
  cmd
    .args(["--from", substituter])
    .args(paths)
    .kill_on_drop(true);
  Ok(cmd)
}

async fn query_input_outputs(
  rootless: bool,
  input_drvs: &BTreeMap<String, InputDerivation>,
) -> color_eyre::Result<Vec<String>> {
  if input_drvs.is_empty() {
    return Ok(Vec::new());
  }
  let mut by_output = BTreeMap::<&str, Vec<&str>>::new();
  for (drv, input) in input_drvs {
    for output in &input.outputs {
      by_output
        .entry(output.as_str())
        .or_default()
        .push(drv.as_str());
    }
  }
  let mut paths = Vec::new();
  for (output_name, drvs) in by_output {
    for chunk in drvs.chunks(128) {
      let mut cmd = sandbox::nix_command(rootless, NixTool::NixStore)?;
      cmd
        .args(["--query", "--binding", output_name])
        .args(chunk)
        .kill_on_drop(true);
      let mut cmd = sandbox::wrap_command(rootless, cmd)?;
      let output = cmd
        .stdin(Stdio::null())
        .output()
        .await
        .context("query effect input outputs")?;
      if !output.status.success() {
        bail!("could not resolve effect input outputs");
      }
      paths.extend(
        String::from_utf8(output.stdout)
          .context("effect input output paths were not UTF-8")?
          .lines()
          .filter(|path| path.starts_with(STORE_PREFIX))
          .map(str::to_owned),
      );
    }
  }
  Ok(paths)
}

fn validate_effect_fs_root(
  path: &Path,
  closure_paths: &BTreeSet<String>,
) -> color_eyre::Result<()> {
  let root = store_root(path, true).ok_or_else(|| {
    eyre!("effect fs root must be one canonical Nix store path")
  })?;
  let root_string = root.to_string_lossy();
  if !closure_paths.contains(root_string.as_ref()) {
    bail!("effect fs root is not part of the effect input closure");
  }
  let canonical =
    fs::canonicalize(path).context("canonicalize effect fs root")?;
  if canonical != root || !canonical.is_dir() {
    bail!("effect fs root must be one canonical Nix store directory");
  }
  Ok(())
}

fn validate_effect_builder(
  path: &Path,
  closure_paths: &BTreeSet<String>,
) -> color_eyre::Result<()> {
  let root = store_root(path, false)
    .ok_or_else(|| eyre!("effect builder must be inside one Nix store path"))?;
  if !closure_paths.contains(root.to_string_lossy().as_ref()) {
    bail!("effect builder is not part of the effect input closure");
  }
  let canonical =
    fs::canonicalize(path).context("canonicalize effect builder")?;
  if store_root(&canonical, false).is_none() || !canonical.is_file() {
    bail!("effect builder must resolve to a file in the Nix store");
  }
  Ok(())
}

fn store_root(path: &Path, exact: bool) -> Option<PathBuf> {
  let mut components = path.components();
  if components.next() != Some(Component::RootDir)
    || components.next() != Some(Component::Normal(OsStr::new("nix")))
    || components.next() != Some(Component::Normal(OsStr::new("store")))
  {
    return None;
  }
  let Component::Normal(name) = components.next()? else {
    return None;
  };
  let name = name.to_str()?;
  if !looks_like_store_reference(name) {
    return None;
  }
  let rest = components.collect::<Vec<_>>();
  if (exact && !rest.is_empty())
    || rest
      .iter()
      .any(|component| !matches!(component, Component::Normal(_)))
  {
    return None;
  }
  Some(Path::new(STORE_PREFIX).join(name))
}

fn referenced_store_paths(derivation: &Derivation) -> BTreeSet<String> {
  let mut paths = BTreeSet::new();
  collect_store_paths(&derivation.builder, &mut paths);
  for arg in &derivation.args {
    collect_store_paths(arg, &mut paths);
  }
  for (name, value) in &derivation.env {
    if name != "__hci_effect_fsroot_copy" {
      collect_store_paths(value, &mut paths);
    }
  }
  paths.extend(derivation.input_srcs.iter().cloned());
  for output in derivation
    .outputs
    .values()
    .filter_map(|output| output.path.as_ref())
  {
    paths.remove(output);
  }
  paths
}

fn collect_store_paths(text: &str, paths: &mut BTreeSet<String>) {
  let mut tail = text;
  while let Some(offset) = tail.find(STORE_PREFIX) {
    let candidate = &tail[offset..];
    let end = candidate
      .bytes()
      .position(|byte| {
        !byte.is_ascii_alphanumeric()
          && !matches!(byte, b'+' | b'-' | b'.' | b'_' | b'?' | b'=' | b'/')
      })
      .unwrap_or(candidate.len());
    let with_suffix = &candidate[..end];
    let store_path_end = with_suffix[STORE_PREFIX.len()..]
      .find('/')
      .map_or(with_suffix.len(), |slash| STORE_PREFIX.len() + slash);
    let store_path = &with_suffix[..store_path_end];
    let name = &store_path[STORE_PREFIX.len()..];
    if name.len() > 33 && name.as_bytes().get(32) == Some(&b'-') {
      paths.insert(store_path.to_owned());
    }
    tail = &candidate[end.max(STORE_PREFIX.len())..];
  }
}

fn prepare_secrets(
  work_dir: &Path,
  source_file: &Path,
  drv_env: &BTreeMap<String, String>,
  structured_attrs: Option<&Map<String, Value>>,
  outputs: &BTreeMap<String, DerivationOutput>,
  context: &EffectContext,
) -> color_eyre::Result<PreparedEffect> {
  fs::create_dir_all(work_dir).context("create agent work directory")?;
  let task_dir = tempfile::Builder::new()
    .prefix("effect-")
    .tempdir_in(work_dir)
    .context("create effect work directory")?;
  let build_dir = task_dir.path().join("build");
  fs::create_dir(&build_dir).context("create effect build directory")?;
  fs::create_dir(build_dir.join("home"))
    .context("create effect home directory")?;
  if let Some(attrs) = structured_attrs {
    materialize_structured_attrs(&build_dir, attrs, outputs)?;
  }

  let definitions = load_secret_definitions(source_file)?;
  let requested = parse_secrets_map(drv_env, structured_attrs)?;
  let mut resolved = BTreeMap::<String, ResolvedSecret>::new();
  let mut redactions = BTreeSet::<String>::new();
  for (alias, reference) in requested {
    if alias == TASK_TOKEN_SECRET {
      bail!(
        "secret alias '{TASK_TOKEN_SECRET}' is reserved for the task token"
      );
    }
    let SecretReference::Local(source_name) = reference else {
      bail!("secret requested as '{alias}' uses an unsupported provider");
    };
    let Some(secret) = definitions.get(&source_name) else {
      bail!("secret requested as '{alias}' is unavailable or denied");
    };
    if secret.kind != "Secret" {
      bail!("secret requested as '{alias}' is unavailable or denied");
    }
    let Some(condition) = secret.condition.as_ref() else {
      bail!("secret requested as '{alias}' is unavailable or denied");
    };
    let condition = parse_condition(condition, 0).map_err(|_| {
      eyre!("secret requested as '{alias}' has an invalid condition")
    })?;
    if !condition.evaluate(context) {
      bail!("secret requested as '{alias}' is unavailable or denied");
    }
    for value in secret.data.values() {
      collect_secret_strings(value, &mut redactions);
    }
    resolved.insert(alias, ResolvedSecret {
      kind: "Secret",
      data: secret.data.clone(),
    });
  }
  if !context.task_token.is_empty() {
    redactions.insert(context.task_token.clone());
    resolved.insert(TASK_TOKEN_SECRET.to_owned(), ResolvedSecret {
      kind: "Secret",
      data: Map::from_iter([(
        "token".to_owned(),
        Value::String(context.task_token.clone()),
      )]),
    });
  }

  let secrets_file = task_dir.path().join("secrets.json");
  let file = OpenOptions::new()
    .write(true)
    .create_new(true)
    .mode(0o600)
    .open(&secrets_file)
    .context("create resolved effects secrets file")?;
  let mut writer = BufWriter::new(file);
  serde_json::to_writer(&mut writer, &resolved)
    .context("write resolved effects secrets")?;
  writer.flush().context("flush resolved effects secrets")?;

  let mut redactions = redactions.into_iter().collect::<Vec<_>>();
  redactions.sort_by(|left, right| {
    right.len().cmp(&left.len()).then_with(|| left.cmp(right))
  });
  Ok(PreparedEffect {
    task_dir,
    build_dir,
    secrets_file,
    redactions,
  })
}

fn materialize_structured_attrs(
  build_dir: &Path,
  attrs: &Map<String, Value>,
  outputs: &BTreeMap<String, DerivationOutput>,
) -> color_eyre::Result<()> {
  let mut prepared = attrs.clone();
  prepared.insert(
    "outputs".into(),
    Value::Object(
      outputs
        .keys()
        .map(|name| {
          (
            name.clone(),
            Value::String(output_placeholder(name.as_str())),
          )
        })
        .collect(),
    ),
  );
  let json =
    serde_json::to_vec(&prepared).context("encode structured attrs")?;
  write_private_file(&build_dir.join(".attrs.json"), &json)
    .context("write .attrs.json")?;
  write_private_file(
    &build_dir.join(".attrs.sh"),
    structured_attrs_shell(&prepared).as_bytes(),
  )
  .context("write .attrs.sh")
}

fn output_placeholder(name: &str) -> String {
  let digest = Sha256::digest(format!("nix-output:{name}"));
  format!("/{}", circus_nix::base32::encode_sha256(&digest))
}

fn write_private_file(path: &Path, contents: &[u8]) -> io::Result<()> {
  let mut file = OpenOptions::new()
    .write(true)
    .create_new(true)
    .mode(0o600)
    .open(path)?;
  file.write_all(contents)
}

fn structured_attrs_shell(attrs: &Map<String, Value>) -> String {
  let mut shell = String::new();
  for (key, value) in attrs {
    if !valid_shell_name(key) {
      continue;
    }
    if let Some(value) = simple_shell_value(value) {
      shell.push_str("declare ");
      shell.push_str(key);
      shell.push('=');
      shell.push_str(&value);
      shell.push('\n');
    } else if let Value::Array(values) = value {
      let Some(values) = values
        .iter()
        .map(simple_shell_value)
        .collect::<Option<Vec<_>>>()
      else {
        continue;
      };
      shell.push_str("declare -a ");
      shell.push_str(key);
      shell.push_str("=(");
      for value in values {
        shell.push_str(&value);
        shell.push(' ');
      }
      shell.push_str(")\n");
    } else if let Value::Object(values) = value {
      let Some(values) = values
        .iter()
        .map(|(key, value)| {
          simple_shell_value(value).map(|value| (shell_quote(key), value))
        })
        .collect::<Option<Vec<_>>>()
      else {
        continue;
      };
      shell.push_str("declare -A ");
      shell.push_str(key);
      shell.push_str("=(");
      for (key, value) in values {
        shell.push('[');
        shell.push_str(&key);
        shell.push_str("]=");
        shell.push_str(&value);
        shell.push(' ');
      }
      shell.push_str(")\n");
    }
  }
  shell
}

fn valid_shell_name(name: &str) -> bool {
  let mut bytes = name.bytes();
  bytes
    .next()
    .is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
    && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}

fn simple_shell_value(value: &Value) -> Option<String> {
  match value {
    Value::String(value) => Some(shell_quote(value)),
    Value::Number(value) if value.is_i64() || value.is_u64() => {
      Some(value.to_string())
    },
    Value::Null => Some("''".into()),
    Value::Bool(true) => Some("1".into()),
    Value::Bool(false) => Some(String::new()),
    _ => None,
  }
}

fn shell_quote(value: &str) -> String {
  let mut quoted = String::with_capacity(value.len() + 2);
  quoted.push('\'');
  quoted.push_str(&value.replace('\'', "'\\''"));
  quoted.push('\'');
  quoted
}

fn load_secret_definitions(
  path: &Path,
) -> color_eyre::Result<BTreeMap<String, SecretDefinition>> {
  let bytes = fs::read(path).context("read effects secrets file")?;
  serde_json::from_slice(&bytes).context("parse effects secrets file")
}

fn parse_secrets_map(
  drv_env: &BTreeMap<String, String>,
  structured_attrs: Option<&Map<String, Value>>,
) -> color_eyre::Result<BTreeMap<String, SecretReference>> {
  let value = if let Some(raw) = drv_env
    .get("secretsToUse")
    .or_else(|| drv_env.get("secretsMap"))
  {
    serde_json::from_str(raw).context("parse effect secretsMap")?
  } else {
    let Some(value) = structured_attrs.and_then(|attrs| {
      attrs
        .get("secretsToUse")
        .or_else(|| attrs.get("secretsMap"))
    }) else {
      return Ok(BTreeMap::new());
    };
    value.clone()
  };
  let values: BTreeMap<String, Value> =
    serde_json::from_value(value).context("parse effect secretsMap")?;
  values
    .into_iter()
    .map(|(alias, value)| {
      let reference = match value {
        Value::String(name) => SecretReference::Local(name),
        Value::Object(object)
          if object.get("type").and_then(Value::as_str) == Some("GitToken") =>
        {
          SecretReference::Unsupported
        },
        _ => bail!("effect secretsMap entry '{alias}' is invalid"),
      };
      Ok((alias, reference))
    })
    .collect()
}

fn parse_condition(
  value: &Value,
  depth: usize,
) -> Result<Condition, &'static str> {
  if depth >= MAX_CONDITION_DEPTH {
    return Err("condition nesting is too deep");
  }
  match value {
    Value::Bool(value) => Ok(Condition::Const(*value)),
    Value::String(value) if value == "isDefaultBranch" => {
      Ok(Condition::IsDefaultBranch)
    },
    Value::String(value) if value == "isTag" => Ok(Condition::IsTag),
    Value::String(_) => Err("unknown condition keyword"),
    Value::Object(object) if object.len() == 1 => {
      let (operator, operand) =
        object.iter().next().ok_or("empty condition")?;
      match operator.as_str() {
        "and" | "or" => {
          let operands = operand
            .as_array()
            .ok_or("boolean operator requires an array")?
            .iter()
            .map(|value| parse_condition(value, depth + 1))
            .collect::<Result<Vec<_>, _>>()?;
          if operator == "and" {
            Ok(Condition::And(operands))
          } else {
            Ok(Condition::Or(operands))
          }
        },
        "isBranch" => {
          operand
            .as_str()
            .map(|value| Condition::IsBranch(value.to_owned()))
            .ok_or("isBranch requires a string")
        },
        "isRepo" => {
          operand
            .as_str()
            .map(|value| Condition::IsRepo(value.to_owned()))
            .ok_or("isRepo requires a string")
        },
        "isOwner" => {
          operand
            .as_str()
            .map(|value| Condition::IsOwner(value.to_owned()))
            .ok_or("isOwner requires a string")
        },
        _ => Err("unknown condition operator"),
      }
    },
    Value::Object(_) => Err("condition object must contain exactly one field"),
    _ => Err("condition must be a boolean, keyword, or object"),
  }
}

impl Condition {
  fn evaluate(&self, context: &EffectContext) -> bool {
    match self {
      Self::Or(conditions) => {
        conditions
          .iter()
          .any(|condition| condition.evaluate(context))
      },
      Self::And(conditions) => {
        conditions
          .iter()
          .all(|condition| condition.evaluate(context))
      },
      Self::IsDefaultBranch => context.is_default_branch,
      Self::IsBranch(branch) => context.branch == *branch,
      Self::IsTag => !context.tag.is_empty(),
      Self::IsRepo(repo) => context.repo == *repo,
      Self::IsOwner(owner) => context.owner == *owner,
      Self::Const(value) => *value,
    }
  }
}

fn collect_secret_strings(value: &Value, values: &mut BTreeSet<String>) {
  match value {
    Value::String(value) => {
      if value.len() >= MIN_REDACTION_LEN {
        values.insert(value.clone());
        values.extend(
          value
            .lines()
            .filter(|line| line.len() >= MIN_REDACTION_LEN)
            .map(str::to_owned),
        );
        if let Ok(encoded) = serde_json::to_string(value)
          && let Some(interior) = encoded
            .strip_prefix('"')
            .and_then(|value| value.strip_suffix('"'))
            .filter(|value| value.len() >= MIN_REDACTION_LEN)
        {
          values.insert(interior.to_owned());
        }
      }
    },
    Value::Array(array) => {
      for value in array {
        collect_secret_strings(value, values);
      }
    },
    Value::Object(object) => {
      for value in object.values() {
        collect_secret_strings(value, values);
      }
    },
    Value::Bool(_) | Value::Number(_) | Value::Null => {},
  }
}

fn effect_environment(
  opts: &RunOptions<'_>,
  derivation: &Derivation,
  context: &EffectContext,
  view: &EffectView,
  structured_attrs: bool,
  fs_root: bool,
) -> BTreeMap<String, String> {
  let build = view.build.as_str();
  let mut env = derivation.env.clone();
  env.remove("__json");
  env.remove("__structuredAttrs");
  env.insert("PATH".into(), "/path-not-set".into());
  env.insert("HOME".into(), format!("{build}/home"));
  env.insert("USER".into(), view.user.clone());
  env.insert("LOGNAME".into(), view.user.clone());
  env.insert("NIX_STORE".into(), "/nix/store".into());
  env.insert("NIX_BUILD_CORES".into(), opts.cores.max(1).to_string());
  env.insert(
    "NIX_REMOTE".into(),
    if opts.rootless {
      "local"
    } else {
      "local?read-only=true"
    }
    .into(),
  );
  if opts.rootless {
    env.insert("NIX_CONF_DIR".into(), "/nix/etc/nix".into());
  } else {
    env.insert(
      "NIX_CONFIG".into(),
      "extra-experimental-features = nix-command flakes read-only-local-store"
        .into(),
    );
  }
  for name in ["NIX_BUILD_TOP", "TMPDIR", "TEMPDIR", "TMP", "TEMP"] {
    env.insert(name.into(), build.to_owned());
  }
  env.insert("NIX_LOG_FD".into(), "2".into());
  env.insert("TERM".into(), "xterm-256color".into());
  env.insert("IN_HERCULES_CI_EFFECT".into(), "true".into());
  env.insert("IN_CIRCUS_EFFECT".into(), "true".into());
  if structured_attrs {
    env.insert("NIX_ATTRS_JSON_FILE".into(), format!("{build}/.attrs.json"));
    env.insert("NIX_ATTRS_SH_FILE".into(), format!("{build}/.attrs.sh"));
  }
  if fs_root {
    env.insert("__hci_effect_fsroot_copied".into(), "1".into());
  }
  if let Some(ca_file) = view.ca_file {
    for name in ["SSL_CERT_FILE", "NIX_SSL_CERT_FILE"] {
      env.entry(name.into()).or_insert_with(|| ca_file.into());
    }
  }
  set_context_env(&mut env, "API_BASE_URL", &view.secrets, context);
  env
}

fn set_context_env(
  env: &mut BTreeMap<String, String>,
  api_name: &str,
  secrets_file: &str,
  context: &EffectContext,
) {
  for prefix in ["HERCULES_CI", "CIRCUS"] {
    env.insert(format!("{prefix}_{api_name}"), context.api_base_url.clone());
    env.insert(format!("{prefix}_SECRETS_JSON"), secrets_file.to_owned());
    env.insert(format!("{prefix}_PROJECT_ID"), context.project_id.clone());
    env.insert(
      format!("{prefix}_PROJECT_PATH"),
      context.project_path.clone(),
    );
  }
}

#[cfg(test)]
mod tests {
  use capnp::capability::Promise;

  use super::*;

  struct TestLogSink;

  #[allow(refining_impl_trait_internal)]
  impl log_sink::Server for TestLogSink {
    fn write(
      self: capnp::capability::Rc<Self>,
      _params: log_sink::WriteParams,
      _results: log_sink::WriteResults,
    ) -> Promise<(), capnp::Error> {
      Promise::ok(())
    }

    fn close(
      self: capnp::capability::Rc<Self>,
      _params: log_sink::CloseParams,
      _results: log_sink::CloseResults,
    ) -> Promise<(), capnp::Error> {
      Promise::ok(())
    }
  }

  fn write_secret_file(dir: &Path, contents: &str) -> PathBuf {
    let path = dir.join("secrets.json");
    let mut file = OpenOptions::new()
      .write(true)
      .create_new(true)
      .mode(0o600)
      .open(&path)
      .expect("create secrets");
    file.write_all(contents.as_bytes()).expect("write secrets");
    path
  }

  fn context() -> EffectContext {
    EffectContext {
      project_id:        "00000000-0000-0000-0000-000000000000".into(),
      project_path:      "github/acme/infra".into(),
      api_base_url:      "https://ci.example".into(),
      owner:             "acme".into(),
      repo:              "infra".into(),
      branch:            "main".into(),
      tag:               String::new(),
      is_default_branch: true,
      task_token:        String::new(),
    }
  }

  fn evaluate(json: &str, context: &EffectContext) -> bool {
    let value: Value = serde_json::from_str(json).expect("condition JSON");
    parse_condition(&value, 0)
      .expect("condition")
      .evaluate(context)
  }

  #[test]
  fn condition_language_matches_hercules_semantics() {
    let context = context();
    assert!(evaluate("true", &context));
    assert!(!evaluate("false", &context));
    assert!(evaluate(r#"{"and":[]}"#, &context));
    assert!(!evaluate(r#"{"or":[]}"#, &context));
    assert!(evaluate(r#""isDefaultBranch""#, &context));
    assert!(!evaluate(r#""isTag""#, &context));
    assert!(evaluate(r#"{"isBranch":"main"}"#, &context));
    assert!(evaluate(r#"{"isRepo":"infra"}"#, &context));
    assert!(evaluate(r#"{"isOwner":"acme"}"#, &context));
  }

  #[test]
  fn tag_and_nested_conditions_are_evaluated() {
    let mut context = context();
    context.branch.clear();
    context.tag = "v1.2.3".into();
    assert!(evaluate(
      r#"{"and":["isTag",{"or":[{"isRepo":"other"},{"isRepo":"infra"}]}]}"#,
      &context
    ));
  }

  #[tokio::test(flavor = "current_thread")]
  async fn cancellation_during_preparation_never_starts_the_builder() {
    let builder_started = std::cell::Cell::new(false);
    let stalled_preparation = async {
      std::future::pending::<()>().await;
      builder_started.set(true);
      Ok(())
    };
    let cancel = CancellationToken::new();
    cancel.cancel();
    let result =
      supervise_preparation(stalled_preparation, cancel, Duration::ZERO)
        .await
        .expect("cancelled effect result");
    assert!(matches!(result, PreparationOutcome::Interrupted(_)));
    let PreparationOutcome::Interrupted(result) = result else {
      return;
    };

    assert_eq!(result.outcome, circus_proto::BuildOutcome::Aborted);
    assert!(result.error_message.contains("aborted"));
    assert!(!builder_started.get());
  }

  #[tokio::test(flavor = "current_thread")]
  async fn running_effect_helper_is_reaped_before_abort_is_reported() {
    let dir = tempfile::tempdir().expect("tempdir");
    let secrets_file = write_secret_file(dir.path(), "{}");
    let config = EffectsConfig {
      secrets_file,
      allow_insecure_transport: false,
    };
    let options = RunOptions {
      drv_path:          "/nix/store/00000000000000000000000000000000-effect.\
                          drv",
      max_log_size:      u64::MAX,
      max_silent_time:   Duration::ZERO,
      build_timeout:     Duration::from_secs(10),
      cores:             1,
      cache_substituter: String::new(),
      cache_public_key:  String::new(),
      rootless:          false,
      work_dir:          dir.path(),
      config:            &config,
      context:           context(),
    };
    let pid_file = dir.path().join("effect-helper.pid");
    let mut command = Command::new("sh");
    command
      .args([
        "-c",
        "echo $$ > \"$1\"; exec sleep 300",
        "sh",
        pid_file.to_str().expect("pid path"),
      ])
      .stdout(Stdio::piped())
      .stderr(Stdio::piped())
      .kill_on_drop(true);
    let prepared = PreparedCommand {
      command,
      redactions: Vec::new(),
      resources: (),
    };
    let cancel = CancellationToken::new();
    let watcher_cancel = cancel.clone();
    let watcher_pid_file = pid_file.clone();
    let watcher = tokio::spawn(async move {
      tokio::time::timeout(Duration::from_secs(5), async {
        loop {
          if let Ok(raw) = tokio::fs::read_to_string(&watcher_pid_file).await
            && let Ok(pid) = raw.trim().parse::<i32>()
          {
            watcher_cancel.cancel();
            return pid;
          }
          tokio::time::sleep(Duration::from_millis(5)).await;
        }
      })
      .await
      .expect("effect helper started")
    });
    let sink = capnp_rpc::new_client(TestLogSink);

    let result =
      execute_prepared(prepared, &options, options.build_timeout, sink, cancel)
        .await
        .expect("effect execution result");
    let pid = watcher.await.expect("pid watcher");

    assert_eq!(result.outcome, circus_proto::BuildOutcome::Aborted);
    // SAFETY: signal 0 only probes whether the child PID still exists.
    assert_eq!(unsafe { nix::libc::kill(pid, 0) }, -1);
    assert_eq!(
      io::Error::last_os_error().raw_os_error(),
      Some(nix::libc::ESRCH)
    );
  }

  #[test]
  fn invalid_and_excessively_nested_conditions_are_rejected() {
    let invalid: Value =
      serde_json::from_str(r#"{"isTag":true}"#).expect("JSON");
    assert!(parse_condition(&invalid, 0).is_err());
    let mut nested = Value::Bool(true);
    for _ in 0..=MAX_CONDITION_DEPTH {
      nested = serde_json::json!({ "and": [nested] });
    }
    assert!(parse_condition(&nested, 0).is_err());
  }

  #[test]
  fn resolved_secrets_use_aliases_and_hide_conditions() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write_secret_file(
      dir.path(),
      r#"{
        "deploy-ssh": {
          "kind": "Secret",
          "data": {"privateKey": "do-not-print-this"},
          "condition": {"and": [
            {"isOwner": "acme"},
            {"isRepo": "infra"},
            "isDefaultBranch"
          ]}
        }
      }"#,
    );
    let mut env = BTreeMap::new();
    env.insert("secretsMap".into(), r#"{"ssh":"deploy-ssh"}"#.into());
    let prepared = prepare_secrets(
      dir.path(),
      &source,
      &env,
      None,
      &BTreeMap::new(),
      &context(),
    )
    .expect("resolve secrets");
    let resolved: Value = serde_json::from_slice(
      &fs::read(prepared.secrets_file).expect("read resolved secrets"),
    )
    .expect("resolved JSON");
    assert!(resolved.get("ssh").is_some());
    assert!(resolved.get("deploy-ssh").is_none());
    assert_eq!(resolved["ssh"]["kind"], "Secret");
    assert_eq!(resolved["ssh"]["data"]["privateKey"], "do-not-print-this");
    assert!(resolved["ssh"].get("condition").is_none());
  }

  #[test]
  fn structured_attrs_materialize_builder_files_and_resolve_secrets() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write_secret_file(
      dir.path(),
      r#"{
        "deploy-ssh": {
          "kind": "Secret",
          "data": {"privateKey": "structured-secret"},
          "condition": true
        }
      }"#,
    );
    let attrs = serde_json::json!({
      "isEffect": true,
      "secretsMap": {"ssh": "deploy-ssh"},
      "__hci_effect_fsroot_copy":
        "/nix/store/55555555555555555555555555555555-root",
      "message": "it's structured"
    })
    .as_object()
    .expect("object")
    .clone();
    let encoded = serde_json::to_string(&attrs).expect("encode attrs");
    let derivation = Derivation {
      args:             Vec::new(),
      builder:          "/nix/store/11111111111111111111111111111111-bash/bin/\
                         bash"
        .into(),
      env:              BTreeMap::from([("__json".into(), encoded)]),
      structured_attrs: Map::new(),
      inputs:           DerivationInputs::default(),
      input_drvs:       BTreeMap::new(),
      input_srcs:       Vec::new(),
      outputs:          BTreeMap::from([("out".into(), DerivationOutput {
        path: None,
      })]),
    };
    let structured =
      effective_structured_attrs(&derivation).expect("structured attrs");
    let prepared = prepare_secrets(
      dir.path(),
      &source,
      &derivation.env,
      structured.as_ref(),
      &derivation.outputs,
      &context(),
    )
    .expect("prepare structured effect");

    let attrs_json = prepared.build_dir.join(".attrs.json");
    let attrs_sh = prepared.build_dir.join(".attrs.sh");
    assert_eq!(
      fs::metadata(&attrs_json)
        .expect("attrs JSON metadata")
        .permissions()
        .mode()
        & 0o777,
      0o600
    );
    assert_eq!(
      fs::metadata(&attrs_sh)
        .expect("attrs shell metadata")
        .permissions()
        .mode()
        & 0o777,
      0o600
    );
    let materialized: Value =
      serde_json::from_slice(&fs::read(&attrs_json).expect("attrs JSON"))
        .expect("parse attrs JSON");
    assert_eq!(
      materialized["outputs"]["out"],
      "/1rz4g4znpzjwh1xymhjpm42vipw92pr73vdgl6xs1hycac8kf2n9"
    );
    let shell = fs::read_to_string(attrs_sh).expect("attrs shell");
    assert!(shell.contains("declare isEffect=1"));
    assert!(shell.contains("declare -A secretsMap="));
    assert!(shell.contains("'it'\\''s structured'"));

    let resolved: Value = serde_json::from_slice(
      &fs::read(&prepared.secrets_file).expect("resolved secrets"),
    )
    .expect("resolved JSON");
    assert_eq!(resolved["ssh"]["data"]["privateKey"], "structured-secret");
    assert_eq!(
      effect_fs_root(&derivation.env, structured.as_ref()).as_deref(),
      Some("/nix/store/55555555555555555555555555555555-root")
    );

    let config = EffectsConfig {
      secrets_file:             source,
      allow_insecure_transport: false,
    };
    let options = RunOptions {
      drv_path:          "/nix/store/00000000000000000000000000000000-effect.\
                          drv",
      max_log_size:      1,
      max_silent_time:   Duration::ZERO,
      build_timeout:     Duration::ZERO,
      cores:             1,
      cache_substituter: String::new(),
      cache_public_key:  String::new(),
      rootless:          false,
      work_dir:          dir.path(),
      config:            &config,
      context:           context(),
    };
    let view = EffectView {
      build:   "/build".into(),
      secrets: "/secrets/secrets.json".into(),
      user:    "root".into(),
      ca_file: None,
    };
    let env =
      effect_environment(&options, &derivation, &context(), &view, true, true);
    assert_eq!(env["NIX_ATTRS_JSON_FILE"], "/build/.attrs.json");
    assert_eq!(env["NIX_ATTRS_SH_FILE"], "/build/.attrs.sh");
    assert_eq!(env["__hci_effect_fsroot_copied"], "1");
    assert_eq!(env["NIX_REMOTE"], "local?read-only=true");
    assert!(env["NIX_CONFIG"].contains("read-only-local-store"));
    assert!(!env.contains_key("__json"));
  }

  #[test]
  fn redactions_only_include_meaningful_secret_strings() {
    let mut redactions = BTreeSet::new();
    collect_secret_strings(
      &serde_json::json!({
        "enabled": true,
        "attempt": 1,
        "short": "yes",
        "token": "long-secret-value"
      }),
      &mut redactions,
    );

    assert!(redactions.contains("long-secret-value"));
    assert!(!redactions.contains("true"));
    assert!(!redactions.contains("1"));
    assert!(!redactions.contains("yes"));
  }

  #[test]
  fn missing_condition_denies_without_leaking_secret_data() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write_secret_file(
      dir.path(),
      r#"{
        "deploy-ssh": {
          "kind": "Secret",
          "data": {"privateKey": "sensitive-value"}
        }
      }"#,
    );
    let env =
      BTreeMap::from([("secretsMap".into(), r#"{"ssh":"deploy-ssh"}"#.into())]);
    let error = prepare_secrets(
      dir.path(),
      &source,
      &env,
      None,
      &BTreeMap::new(),
      &context(),
    )
    .err()
    .expect("missing condition must fail")
    .to_string();
    assert!(error.contains("unavailable or denied"));
    assert!(!error.contains("sensitive-value"));
    assert!(!error.contains("deploy-ssh"));
  }

  #[test]
  fn startup_validation_accepts_private_file_and_rejects_unsafe_mode() {
    let dir = tempfile::tempdir().expect("tempdir");
    let source = write_secret_file(
      dir.path(),
      r#"{"x":{"kind":"Secret","data":{},"condition":true}}"#,
    );
    let config = EffectsConfig {
      secrets_file:             source.clone(),
      allow_insecure_transport: false,
    };
    validate_config(&config).expect("private valid secrets file");
    fs::set_permissions(&source, fs::Permissions::from_mode(0o644))
      .expect("chmod");
    assert!(validate_config(&config).is_err());
  }

  #[test]
  fn rootless_effects_are_rejected_before_store_exposure() {
    validate_mode(false).expect("non-rootless effects");
    let error = validate_mode(true)
      .expect_err("rootless effects must fail closed")
      .to_string();
    assert!(error.contains("persistent direct store"));
  }

  #[test]
  fn derivation_json_preserves_builder_args_and_environment() {
    let drv_path = "/nix/store/00000000000000000000000000000000-effect.drv";
    let json = format!(
      r#"{{
        "{drv_path}": {{
          "args": ["-e", "/nix/store/11111111111111111111111111111111-setup"],
          "builder": "/nix/store/22222222222222222222222222222222-bash/bin/bash",
          "env": {{"isEffect":"1","effectScript":"deploy"}},
          "inputDrvs": {{
            "/nix/store/33333333333333333333333333333333-input.drv": ["out"]
          }},
          "inputSrcs": [],
          "outputs": {{"out":{{"path":"/nix/store/44444444444444444444444444444444-effect"}}}}
        }}
      }}"#
    );
    let drv =
      parse_derivation_json(json.as_bytes(), drv_path).expect("legacy JSON");
    assert_eq!(
      drv.builder,
      "/nix/store/22222222222222222222222222222222-bash/bin/bash"
    );
    assert_eq!(drv.args[0], "-e");
    assert_eq!(drv.env["effectScript"], "deploy");
    assert_eq!(
      drv.input_drvs["/nix/store/33333333333333333333333333333333-input.drv"]
        .outputs,
      ["out"]
    );
  }

  #[test]
  fn wrapped_derivation_json_and_env_json_effect_marker_are_supported() {
    let drv_name = "00000000000000000000000000000000-effect.drv";
    let drv_path = format!("{STORE_PREFIX}{drv_name}");
    let json = format!(
      r#"{{
        "version": 3,
        "derivations": {{
          "{drv_name}": {{
            "args": [],
            "builder": "11111111111111111111111111111111-bash/bin/bash",
            "env": {{"__json":"{{\"isEffect\":true}}"}},
            "inputs": {{
              "drvs": {{
                "22222222222222222222222222222222-input.drv": {{
                  "outputs": ["out"]
                }}
              }},
              "srcs": ["33333333333333333333333333333333-source"]
            }},
            "outputs": {{
              "out": {{"path":"44444444444444444444444444444444-output"}}
            }}
          }}
        }}
      }}"#
    );
    let derivation =
      parse_derivation_json(json.as_bytes(), &drv_path).expect("wrapped JSON");
    let attrs =
      effective_structured_attrs(&derivation).expect("structured attrs");
    ensure_effect(&derivation, attrs.as_ref())
      .expect("env __json isEffect marker");
    assert_eq!(
      derivation.builder,
      "/nix/store/11111111111111111111111111111111-bash/bin/bash"
    );
    assert!(
      derivation
        .input_drvs
        .contains_key("/nix/store/22222222222222222222222222222222-input.drv")
    );
    assert_eq!(derivation.input_srcs, [
      "/nix/store/33333333333333333333333333333333-source"
    ]);
    assert_eq!(
      derivation.outputs["out"].path.as_deref(),
      Some("/nix/store/44444444444444444444444444444444-output")
    );
  }

  #[test]
  fn cache_copy_requires_and_uses_the_configured_signing_key() {
    let paths =
      vec!["/nix/store/00000000000000000000000000000000-input".into()];
    let key = "cache.invalid-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=";
    assert!(validate_cache_settings("", "").is_err());
    assert!(validate_cache_settings("https://cache.invalid", "").is_err());
    validate_cache_settings("https://cache.invalid", key)
      .expect("complete signed cache settings");
    let error = nix_copy_command(false, "", key, false, &paths)
      .expect_err("missing substituter must fail")
      .to_string();
    assert!(error.contains("cache_substituter"));
    let error =
      nix_copy_command(false, "https://cache.invalid", "", false, &paths)
        .expect_err("missing key must fail")
        .to_string();
    assert!(error.contains("cache_public_key"));

    let cmd =
      nix_copy_command(false, "https://cache.invalid", key, true, &paths)
        .expect("signed copy command");
    let args = cmd
      .as_std()
      .get_args()
      .map(|arg| arg.to_string_lossy().into_owned())
      .collect::<Vec<_>>();
    assert!(args.windows(3).any(|args| {
      args
        == [
          "--option",
          "extra-trusted-public-keys",
          "cache.invalid-1:AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=",
        ]
    }));
    assert!(args.iter().any(|arg| arg == "--derivation"));
    assert!(!args.iter().any(|arg| arg == "--no-check-sigs"));
  }

  #[test]
  fn store_path_scanner_returns_roots_not_files() {
    let mut paths = BTreeSet::new();
    collect_store_paths(
      "x=/nix/store/00000000000000000000000000000000-tool/bin/tool:y",
      &mut paths,
    );
    assert_eq!(paths.into_iter().collect::<Vec<_>>(), vec![
      "/nix/store/00000000000000000000000000000000-tool"
    ]);
  }

  #[test]
  fn host_paths_and_store_traversal_cannot_be_used_as_effect_roots() {
    let closure = BTreeSet::new();
    for path in [
      "/var/lib/circus-agent",
      "/nix/store/00000000000000000000000000000000-root/../../var/lib",
      "/nix/store/00000000000000000000000000000000-unreferenced",
    ] {
      assert!(
        validate_effect_fs_root(Path::new(path), &closure).is_err(),
        "accepted unsafe fs root {path}"
      );
    }
    assert!(
      validate_effect_builder(
        Path::new(
          "/nix/store/00000000000000000000000000000000-builder/../../etc/\
           shadow"
        ),
        &closure,
      )
      .is_err()
    );
  }
}
