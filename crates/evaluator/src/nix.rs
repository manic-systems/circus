use std::{collections::HashMap, path::Path, time::Duration};

use circus_common::{CiError, InputType, error::Result, models::JobsetInput};
use circus_config::EvaluatorConfig;
use tokio::process::Command;
use tokio_util::sync::CancellationToken;
use url::Url;

mod eval_command;
mod flake_lock;

use eval_command::NixEvalPolicy;

/// Per-worker memory ceiling in MB; evix restarts a worker once it is exceeded.
const EVAL_MAX_MEMORY_MB: usize = 4096;

#[derive(Debug, Clone, Default)]
pub struct NixMeta {
  pub description: Option<String>,
  pub license:     Option<String>,
  pub homepage:    Option<String>,
  pub maintainers: Option<String>,
}

#[derive(Debug, Clone)]
pub struct NixJob {
  pub name:         String,
  pub drv_path:     String,
  pub system:       Option<String>,
  pub outputs:      Option<HashMap<String, String>>,
  pub input_drvs:   Option<HashMap<String, serde_json::Value>>,
  pub constituents: Option<Vec<String>>,
  pub meta:         NixMeta,
}

/// Convert an [`evix::Derivation`] event into a [`NixJob`].
///
/// evix exposes the attribute path as `attr` and the derivation name as `name`;
/// `attr` is preferred as the job identifier. Empty `outputs`/`input_drvs`/
/// `constituents` collapse to `None` so ordinary builds are not misclassified
/// as aggregates and absent data is represented uniformly.
pub(crate) fn nix_job_from_derivation(drv: &evix::Derivation) -> NixJob {
  let name = if drv.attr.is_empty() {
    drv.name.clone()
  } else {
    drv.attr.clone()
  };

  let system = (!drv.system.is_empty()).then(|| drv.system.clone());

  let outputs: HashMap<String, String> = drv
    .outputs
    .iter()
    .filter_map(|(key, path)| path.clone().map(|p| (key.clone(), p)))
    .collect();
  let outputs = (!outputs.is_empty()).then_some(outputs);

  let input_drvs = if drv.input_drvs.is_empty() {
    None
  } else {
    Some(
      drv
        .input_drvs
        .iter()
        .map(|(key, value)| {
          (key.clone(), serde_json::Value::from(value.clone()))
        })
        .collect(),
    )
  };

  let constituents = drv.constituents.clone().filter(|c| !c.is_empty());

  NixJob {
    name,
    drv_path: drv.drv_path.clone(),
    system,
    outputs,
    input_drvs,
    constituents,
    meta: parse_meta(drv.meta.as_ref()),
  }
}

/// Flatten a single `meta.license` JSON value to a display string. nixpkgs
/// licenses can be a string, an object with `fullName`/`spdxId`/`shortName`,
/// or a list of either. The channel tarball and `nix-env -qa --description`
/// expect a single string, so we pick the first sensible label.
fn flatten_license(v: &serde_json::Value) -> Option<String> {
  match v {
    serde_json::Value::String(s) => Some(s.clone()),
    serde_json::Value::Object(map) => {
      map
        .get("fullName")
        .or_else(|| map.get("spdxId"))
        .or_else(|| map.get("shortName"))
        .and_then(|x| x.as_str())
        .map(str::to_owned)
    },
    serde_json::Value::Array(arr) => {
      let parts: Vec<String> = arr.iter().filter_map(flatten_license).collect();
      if parts.is_empty() {
        None
      } else {
        Some(parts.join(", "))
      }
    },
    _ => None,
  }
}

/// Flatten `meta.maintainers` to a comma-separated list. nixpkgs entries
/// are either bare strings or objects carrying `github`/`name`/`email`.
fn flatten_maintainers(v: &serde_json::Value) -> Option<String> {
  let arr = v.as_array()?;
  let parts: Vec<String> = arr
    .iter()
    .filter_map(|m| {
      match m {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Object(map) => {
          map
            .get("github")
            .or_else(|| map.get("name"))
            .or_else(|| map.get("email"))
            .and_then(|x| x.as_str())
            .map(str::to_owned)
        },
        _ => None,
      }
    })
    .collect();
  if parts.is_empty() {
    None
  } else {
    Some(parts.join(", "))
  }
}

fn parse_meta(v: Option<&serde_json::Value>) -> NixMeta {
  let Some(serde_json::Value::Object(map)) = v else {
    return NixMeta::default();
  };
  NixMeta {
    description: map
      .get("description")
      .and_then(|x| x.as_str())
      .map(str::to_owned),
    license:     map.get("license").and_then(flatten_license),
    homepage:    map.get("homepage").and_then(|x| {
      match x {
        serde_json::Value::String(s) => Some(s.clone()),
        serde_json::Value::Array(arr) => {
          arr.iter().find_map(|v| v.as_str()).map(str::to_owned)
        },
        _ => None,
      }
    }),
    maintainers: map.get("maintainers").and_then(flatten_maintainers),
  }
}

/// Result of evaluating nix expressions.
#[derive(Debug)]
pub struct EvalResult {
  pub jobs:        Vec<NixJob>,
  pub error_count: usize,
}

/// Evaluate nix expressions and return discovered jobs.
/// If `flake_mode` is true, evaluates a flake output via evix.
/// If `flake_mode` is false, evaluates a legacy expression file via evix.
///
/// # Errors
///
/// Returns error if nix evaluation command fails or times out.
#[tracing::instrument(skip(config, inputs), fields(flake_mode, nix_expression))]
pub async fn evaluate(
  repo_path: &Path,
  commit_hash: &str,
  nix_expression: &str,
  flake_mode: bool,
  timeout: Duration,
  config: &EvaluatorConfig,
  inputs: &[JobsetInput],
  cancel: &CancellationToken,
) -> Result<EvalResult> {
  // Validate nix expression before constructing any commands
  circus_nix::validate::validate_nix_expression(nix_expression)
    .map_err(|e| CiError::NixEval(format!("Invalid nix expression: {e}")))?;

  // Strip a flake-style attribute prefix the user may have typed (".#packages"
  // or "#packages"). The flake ref already adds the '#' separator, so leaving
  // it in produces an attribute path like "#packages".
  let normalized = nix_expression
    .strip_prefix(".#")
    .or_else(|| nix_expression.strip_prefix('#'))
    .unwrap_or(nix_expression);
  let nix_expression = if normalized.is_empty() {
    nix_expression
  } else {
    normalized
  };

  if flake_mode {
    evaluate_flake(
      repo_path,
      commit_hash,
      nix_expression,
      timeout,
      config,
      inputs,
      cancel,
    )
    .await
  } else {
    evaluate_legacy(repo_path, nix_expression, timeout, config, inputs, cancel)
      .await
  }
}

/// Evaluating a raw `nixosConfigurations.<name>` does not yield a buildable
/// derivation, so drill into the toplevel derivation.
fn rewrite_nixos_config_expr(expr: &str) -> Option<String> {
  let parts = expr.split('.').collect::<Vec<&str>>();
  match parts.as_slice() {
    ["nixosConfigurations", name] => {
      Some(format!(
        "nixosConfigurations.{name}.config.system.build.toplevel"
      ))
    },
    _ => None,
  }
}

/// Derive `allowed-uris` from the project's `flake.lock`.
fn lock_derived_allowed_uris(
  repo_path: &Path,
  config: &EvaluatorConfig,
) -> Result<Vec<String>> {
  if !config.restrict_eval || !config.auto_allowed_uris {
    return Ok(Vec::new());
  }

  let lock_path = repo_path.join("flake.lock");
  match std::fs::read_to_string(&lock_path) {
    Ok(contents) => {
      let uris = flake_lock::allowed_uris_from_lock(&contents);
      tracing::info!(
        count = uris.len(),
        "Derived allowed-uris from flake.lock"
      );
      Ok(uris)
    },
    Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
      if config.require_locked_flake {
        Err(CiError::NixEval(format!(
          "No flake.lock at {} but require_locked_flake is enabled. Commit a \
           lock file or disable require_locked_flake",
          lock_path.display()
        )))
      } else {
        tracing::warn!(
          path = %lock_path.display(),
          "No flake.lock found, deriving no allowed-uris (set \
           evaluator.allowed_uris or restrict_eval = false if inputs are blocked)"
        );
        Ok(Vec::new())
      }
    },
    Err(e) => {
      Err(CiError::NixEval(format!(
        "Failed to read {}: {e}",
        lock_path.display()
      )))
    },
  }
}

#[tracing::instrument(skip(config, inputs))]
async fn evaluate_flake(
  repo_path: &Path,
  commit_hash: &str,
  nix_expression: &str,
  timeout: Duration,
  config: &EvaluatorConfig,
  inputs: &[JobsetInput],
  cancel: &CancellationToken,
) -> Result<EvalResult> {
  if nix_expression == "nixosConfigurations" {
    return evaluate_all_nixos_configs(
      repo_path, timeout, config, inputs, cancel,
    )
    .await;
  }

  let effective_expr = rewrite_nixos_config_expr(nix_expression)
    .unwrap_or_else(|| nix_expression.to_string());

  if effective_expr != nix_expression {
    tracing::info!(
      original = %nix_expression,
      rewritten = %effective_expr,
      "Rewrote nixosConfigurations to target toplevel derivation"
    );
  }

  let flake_ref = git_flake_ref(repo_path, commit_hash, &effective_expr)?;

  tracing::debug!(flake_ref = %flake_ref, "Running evix evaluation");

  let mut override_inputs = Vec::new();
  for input in inputs {
    if input.input_type == InputType::Git {
      circus_nix::validate::validate_jobset_input(
        &input.name,
        input.input_type,
        &input.value,
        input.revision.as_deref(),
      )
      .map_err(|e| CiError::NixEval(format!("Invalid jobset input: {e}")))?;
      override_inputs.push((input.name.clone(), input.value.clone()));
    }
  }

  let derived_uris = lock_derived_allowed_uris(repo_path, config)?;
  let policy =
    NixEvalPolicy::from(config).with_extra_allowed_uris(derived_uris);

  let evix_config = evix::Config {
    input: evix::Input::Flake(flake_ref),
    auto_args: Vec::new(),
    force_recurse: true,
    gc_roots_dir: None,
    workers: config.eval_workers,
    max_memory_size: EVAL_MAX_MEMORY_MB,
    meta: true,
    show_input_drvs: true,
    override_inputs,
    nix_options: policy.nix_options(),
    ..evix::Config::default()
  };

  eval_command::run_eval(evix_config, timeout, "flake", cancel).await
}

fn git_flake_ref(
  repo_path: &Path,
  commit_hash: &str,
  expression: &str,
) -> Result<String> {
  let mut url = Url::from_file_path(repo_path).map_err(|()| {
    CiError::NixEval(format!(
      "Failed to convert repository path to a file URL: {}",
      repo_path.display()
    ))
  })?;
  url.query_pairs_mut().append_pair("rev", commit_hash);
  Ok(format!("git+{url}#{expression}"))
}

/// Resolve all toplevels in one nix eval.
async fn evaluate_all_nixos_configs(
  repo_path: &Path,
  _timeout: Duration,
  config: &EvaluatorConfig,
  _inputs: &[JobsetInput],
  cancel: &CancellationToken,
) -> Result<EvalResult> {
  let flake_ref = format!("{}#nixosConfigurations", repo_path.display());

  let expr = "builtins.mapAttrs (_: v: v.config.system.build.toplevel)";
  let mut cmd = Command::new("nix");
  cmd
    .args([
      "eval",
      "--json",
      &flake_ref,
      "--apply",
      expr,
      "--no-write-lock-file",
    ])
    .kill_on_drop(true);
  let derived_uris = lock_derived_allowed_uris(repo_path, config)?;
  NixEvalPolicy::from(config)
    .with_extra_allowed_uris(derived_uris)
    .apply_to(&mut cmd);
  let output = tokio::select! {
    output = cmd.output() => output,
    () = cancel.cancelled() => return Err(CiError::NixEval("Nix evaluation was cancelled".to_string())),
  }.map_err(|e| {
    CiError::NixEval(format!("Failed to evaluate nixosConfigurations: {e}"))
  })?;

  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    return Err(CiError::NixEval(format!(
      "Failed to evaluate nixosConfigurations: {stderr}"
    )));
  }

  let value = serde_json::from_slice::<serde_json::Value>(&output.stdout)
    .map_err(|e| {
      CiError::NixEval(format!(
        "Failed to parse nixosConfigurations output: {e}"
      ))
    })?;

  let entries = flatten_attrs("", &value);

  tracing::info!(
    count = entries.len(),
    "Discovered nixosConfigurations toplevels"
  );

  let mut jobs = Vec::new();
  for (name, eval_path) in entries {
    let drv_ref = format!(
      "{}#nixosConfigurations.{name}.config.system.build.toplevel",
      repo_path.display()
    );
    let shown = resolve_drv(&drv_ref).await;

    let (drv_path, system, outputs, input_drvs) = if let Some(shown) = shown {
      (
        shown.drv_path,
        shown.system,
        shown.outputs,
        shown.input_drvs,
      )
    } else if is_store_drv_path(&eval_path) {
      (eval_path, None, None, None)
    } else {
      tracing::warn!(
        attr = %name,
        "Skipping nixosConfiguration: could not resolve drv path"
      );
      continue;
    };

    jobs.push(NixJob {
      name,
      drv_path,
      system,
      outputs,
      input_drvs,
      constituents: None,
      meta: NixMeta::default(),
    });
  }

  Ok(EvalResult {
    jobs,
    error_count: 0,
  })
}

async fn resolve_drv(flake_ref: &str) -> Option<ShownDerivation> {
  let out = Command::new("nix")
    .args(["derivation", "show", flake_ref])
    .kill_on_drop(true)
    .output()
    .await
    .ok()?;

  if !out.status.success() {
    return None;
  }

  let json = serde_json::from_slice::<serde_json::Value>(&out.stdout).ok()?;
  parse_derivation_show(&json)
}

/// Legacy (non-flake) evaluation: import the nix expression file and evaluate
/// it.
#[tracing::instrument(skip(config, inputs))]
async fn evaluate_legacy(
  repo_path: &Path,
  nix_expression: &str,
  timeout: Duration,
  config: &EvaluatorConfig,
  inputs: &[JobsetInput],
  cancel: &CancellationToken,
) -> Result<EvalResult> {
  let repo_path = repo_path.canonicalize().map_err(|e| {
    CiError::NixEval(format!("Failed to canonicalize repository path: {e}"))
  })?;
  let expr_path = repo_path.join(nix_expression);
  let expr_path = expr_path.canonicalize().map_err(|e| {
    CiError::NixEval(format!("Failed to canonicalize nix expression path: {e}"))
  })?;
  if !expr_path.starts_with(&repo_path) {
    return Err(CiError::NixEval(
      "Nix expression path escapes repository checkout".to_string(),
    ));
  }

  // Map jobset inputs to evix auto-args: string inputs become `--argstr`
  // equivalents (`AutoArg::Str`), boolean/build inputs become `--arg`
  // equivalents (`AutoArg::Expr`).
  let mut auto_args = Vec::new();
  for input in inputs {
    circus_nix::validate::validate_jobset_input(
      &input.name,
      input.input_type,
      &input.value,
      input.revision.as_deref(),
    )
    .map_err(|e| CiError::NixEval(format!("Invalid jobset input: {e}")))?;
    match input.input_type {
      InputType::String | InputType::Git => {
        auto_args
          .push((input.name.clone(), evix::AutoArg::Str(input.value.clone())));
      },
      InputType::Boolean => {
        if input.value == "true" || input.value == "false" {
          auto_args.push((
            input.name.clone(),
            evix::AutoArg::Expr(input.value.clone()),
          ));
        } else {
          return Err(CiError::NixEval(format!(
            "Invalid boolean input '{}': expected true or false",
            input.name
          )));
        }
      },
      InputType::Build => {
        auto_args
          .push((input.name.clone(), evix::AutoArg::Expr(input.value.clone())));
      },
    }
  }

  let evix_config = evix::Config {
    input: evix::Input::File(expr_path),
    auto_args,
    force_recurse: true,
    gc_roots_dir: None,
    workers: config.eval_workers,
    max_memory_size: EVAL_MAX_MEMORY_MB,
    meta: true,
    show_input_drvs: true,
    override_inputs: Vec::new(),
    nix_options: NixEvalPolicy::from(config).nix_options(),
    ..evix::Config::default()
  };

  eval_command::run_eval(evix_config, timeout, "legacy", cancel).await
}

/// Recursively flatten a nix eval --json value into (`attr_path`, `drv_path`)
/// pairs. Structured derivation objects are emitted as a single job; plain
/// attrsets are recursed into. Bare strings are accepted only when they look
/// like Nix store paths; `nix eval --json` stringifies derivations to output
/// paths, and the caller resolves those attrs back to buildable `.drv` paths.
/// Non-store metadata such as `type` or `name` never becomes a build by
/// accident.
fn flatten_attrs(
  prefix: &str,
  value: &serde_json::Value,
) -> Vec<(String, String)> {
  match value {
    serde_json::Value::String(s) if is_store_path(s) => {
      vec![(prefix.to_string(), s.clone())]
    },
    serde_json::Value::Object(map) => {
      if map
        .get("type")
        .and_then(serde_json::Value::as_str)
        .is_some_and(|t| t == "derivation")
        && let Some(drv_path) = map
          .get("drvPath")
          .or_else(|| map.get("drv_path"))
          .and_then(serde_json::Value::as_str)
      {
        return vec![(prefix.to_string(), drv_path.to_string())];
      }

      let mut result = Vec::new();
      for (key, val) in map {
        let child_prefix = if prefix.is_empty() {
          key.clone()
        } else {
          format!("{prefix}.{key}")
        };
        result.extend(flatten_attrs(&child_prefix, val));
      }
      result
    },
    _ => Vec::new(),
  }
}

fn is_store_drv_path(value: &str) -> bool {
  is_store_path(value)
    && std::path::Path::new(value)
      .extension()
      .is_some_and(|ext| ext.eq_ignore_ascii_case("drv"))
}

fn is_store_path(value: &str) -> bool {
  value.starts_with("/nix/store/")
}

struct ShownDerivation {
  drv_path:   String,
  system:     Option<String>,
  outputs:    Option<HashMap<String, String>>,
  input_drvs: Option<HashMap<String, serde_json::Value>>,
}

fn parse_derivation_show(value: &serde_json::Value) -> Option<ShownDerivation> {
  // Newer nix derivation show wraps output as
  // {"derivations": {<drv>: {...}}, "version": N}; older nix keys the
  // drv paths directly at the top level.
  let derivations = value
    .get("derivations")
    .and_then(serde_json::Value::as_object)
    .or_else(|| value.as_object())?;
  let (drv_path, drv_val) = derivations.iter().next()?;

  let system = drv_val
    .get("system")
    .and_then(|v| v.as_str())
    .map(std::string::ToString::to_string);

  let outputs = drv_val
    .get("outputs")
    .and_then(serde_json::Value::as_object)
    .map(|map| {
      map
        .iter()
        .filter_map(|(name, output)| {
          output
            .get("path")
            .or_else(|| output.get("outPath"))
            .and_then(|v| v.as_str())
            .map(|path| (name.clone(), path.to_string()))
        })
        .collect::<HashMap<_, _>>()
    })
    .filter(|map| !map.is_empty());

  let input_drvs = drv_val.get("inputDrvs").and_then(|v| {
    serde_json::from_value::<HashMap<String, serde_json::Value>>(v.clone()).ok()
  });

  let drv_path = if drv_path.starts_with("/nix/store/") {
    drv_path.clone()
  } else {
    format!("/nix/store/{drv_path}")
  };

  Some(ShownDerivation {
    drv_path,
    system,
    outputs,
    input_drvs,
  })
}

#[cfg(test)]
mod meta_tests {
  #![expect(clippy::unwrap_used, reason = "fine in tests")]
  use super::*;

  #[test]
  fn license_string() {
    let v = serde_json::json!("MIT");
    assert_eq!(flatten_license(&v).as_deref(), Some("MIT"));
  }

  #[test]
  fn license_object_prefers_full_name() {
    let v = serde_json::json!({
      "fullName": "MIT License",
      "spdxId": "MIT",
      "shortName": "mit",
    });
    assert_eq!(flatten_license(&v).as_deref(), Some("MIT License"));
  }

  #[test]
  fn license_object_falls_back_to_spdx_then_short_name() {
    assert_eq!(
      flatten_license(&serde_json::json!({"spdxId": "MIT"})).as_deref(),
      Some("MIT"),
    );
    assert_eq!(
      flatten_license(&serde_json::json!({"shortName": "mit"})).as_deref(),
      Some("mit"),
    );
  }

  #[test]
  fn license_list_joins() {
    let v = serde_json::json!([
      {"fullName": "MIT License"},
      "Apache-2.0",
    ]);
    assert_eq!(
      flatten_license(&v).as_deref(),
      Some("MIT License, Apache-2.0"),
    );
  }

  #[test]
  fn maintainers_handles_string_and_object_entries() {
    let v = serde_json::json!([
      "alice",
      {"github": "bob"},
      {"name": "Carol", "email": "carol@example.com"},
      {"email": "dave@example.com"},
    ]);
    assert_eq!(
      flatten_maintainers(&v).as_deref(),
      Some("alice, bob, Carol, dave@example.com"),
    );
  }

  #[test]
  fn parse_meta_full() {
    let v = serde_json::json!({
      "description": "hello",
      "license": {"fullName": "MIT"},
      "homepage": "https://example.com",
      "maintainers": [{"github": "alice"}],
    });
    let m = parse_meta(Some(&v));
    assert_eq!(m.description.as_deref(), Some("hello"));
    assert_eq!(m.license.as_deref(), Some("MIT"));
    assert_eq!(m.homepage.as_deref(), Some("https://example.com"));
    assert_eq!(m.maintainers.as_deref(), Some("alice"));
  }

  #[test]
  fn parse_meta_absent() {
    let m = parse_meta(None);
    assert!(m.description.is_none());
    assert!(m.license.is_none());
    assert!(m.homepage.is_none());
    assert!(m.maintainers.is_none());
  }

  #[test]
  fn parse_derivation_show_wrapped_uses_drv_key() {
    let v = serde_json::json!({
      "derivations": {
        "/nix/store/abc-hello.drv": {
          "system": "x86_64-linux",
          "outputs": {
            "out": {
              "path": "/nix/store/def-hello"
            }
          },
          "inputDrvs": {
            "/nix/store/bash.drv": ["out"]
          }
        }
      },
      "version": 3
    });

    let shown = parse_derivation_show(&v);
    assert!(shown.is_some());
    let Some(shown) = shown else {
      return;
    };
    assert_eq!(shown.drv_path, "/nix/store/abc-hello.drv");
    assert_eq!(shown.system.as_deref(), Some("x86_64-linux"));
    assert_eq!(
      shown
        .outputs
        .as_ref()
        .and_then(|o| o.get("out"))
        .map(String::as_str),
      Some("/nix/store/def-hello"),
    );
    assert!(
      shown
        .input_drvs
        .as_ref()
        .is_some_and(|i| i.contains_key("/nix/store/bash.drv"))
    );
  }

  #[test]
  fn flatten_attrs_emits_one_job_per_structured_derivation() {
    let value = serde_json::json!({
      "x86_64-linux": {
        "default": {
          "type": "derivation",
          "name": "hello",
          "system": "x86_64-linux",
          "drvPath": "/nix/store/abc-hello.drv",
          "outPath": "/nix/store/def-hello",
          "outputs": ["out"]
        },
        "nested": {
          "world": {
            "type": "derivation",
            "drvPath": "/nix/store/ghi-world.drv"
          }
        }
      }
    });

    let jobs = flatten_attrs("", &value);

    assert_eq!(jobs.len(), 2);
    assert_eq!(
      jobs[0],
      (
        "x86_64-linux.default".to_string(),
        "/nix/store/abc-hello.drv".to_string(),
      ),
    );
    assert_eq!(
      jobs[1],
      (
        "x86_64-linux.nested.world".to_string(),
        "/nix/store/ghi-world.drv".to_string(),
      ),
    );
  }

  #[test]
  fn flatten_attrs_ignores_derivation_metadata_strings() {
    let value = serde_json::json!({
      "default": {
        "type": "derivation",
        "name": "hello",
        "system": "x86_64-linux",
        "drvPath": "/nix/store/abc-hello.drv"
      },
      "metadata": {
        "type": "derivations",
        "name": "not-a-job"
      },
      "legacy": "/nix/store/def-legacy.drv"
    });

    let jobs = flatten_attrs("", &value);

    assert_eq!(jobs.len(), 2);
    assert!(jobs.contains(&(
      "default".to_string(),
      "/nix/store/abc-hello.drv".to_string(),
    )));
    assert!(jobs.contains(&(
      "legacy".to_string(),
      "/nix/store/def-legacy.drv".to_string(),
    )));
  }

  #[test]
  fn flatten_attrs_emits_stringified_derivation_out_paths() {
    let value = serde_json::json!({
      "x86_64-linux": {
        "hello": "/nix/store/def-hello"
      },
      "metadata": {
        "type": "derivations",
        "name": "not-a-job"
      }
    });

    let jobs = flatten_attrs("", &value);

    assert_eq!(jobs.len(), 1);
    assert_eq!(
      jobs[0],
      (
        "x86_64-linux.hello".to_string(),
        "/nix/store/def-hello".to_string(),
      ),
    );
  }

  #[test]
  fn parse_derivation_show_wrapped_normalizes_store_basename() {
    let v = serde_json::json!({
      "derivations": {
        "abc-hello.drv": {
          "system": "x86_64-linux",
          "outputs": {
            "out": {
              "path": "/nix/store/def-hello"
            }
          }
        }
      },
      "version": 3
    });

    let shown = parse_derivation_show(&v);
    assert!(shown.is_some());
    let Some(shown) = shown else {
      return;
    };
    assert_eq!(shown.drv_path, "/nix/store/abc-hello.drv");
  }

  #[test]
  fn rewrite_nixos_config_expr_only_rewrites_bare_config_name() {
    assert_eq!(
      rewrite_nixos_config_expr("nixosConfigurations.main").as_deref(),
      Some("nixosConfigurations.main.config.system.build.toplevel"),
    );
    assert!(
      rewrite_nixos_config_expr(
        "nixosConfigurations.main.config.system.build.toplevel"
      )
      .is_none()
    );
    assert!(rewrite_nixos_config_expr("packages").is_none());
  }

  #[test]
  fn git_flake_ref_pins_the_checked_out_commit() {
    assert_eq!(
      git_flake_ref(
        Path::new("/tmp/circus fixture"),
        "0123456789abcdef",
        "packages",
      )
      .unwrap(),
      "git+file:///tmp/circus%20fixture?rev=0123456789abcdef#packages",
    );
  }

  #[test]
  fn parse_derivation_show_legacy_uses_top_level_drv_key() {
    let v = serde_json::json!({
      "/nix/store/abc-hello.drv": {
        "system": "x86_64-linux",
        "outputs": {
          "out": {
            "outPath": "/nix/store/def-hello"
          }
        }
      }
    });

    let shown = parse_derivation_show(&v);
    assert!(shown.is_some());
    let Some(shown) = shown else {
      return;
    };
    assert_eq!(shown.drv_path, "/nix/store/abc-hello.drv");
    assert_eq!(
      shown
        .outputs
        .as_ref()
        .and_then(|o| o.get("out"))
        .map(String::as_str),
      Some("/nix/store/def-hello"),
    );
  }

  fn derivation(attr: &str, name: &str) -> evix::Derivation {
    evix::Derivation {
      attr:          attr.to_string(),
      attr_path:     attr.split('.').map(str::to_owned).collect(),
      name:          name.to_string(),
      system:        "x86_64-linux".to_string(),
      drv_path:      "/nix/store/abc-hello.drv".to_string(),
      outputs:       std::collections::BTreeMap::new(),
      meta:          None,
      input_drvs:    std::collections::BTreeMap::new(),
      constituents:  None,
      gc_root_error: None,
    }
  }

  #[test]
  fn nix_job_prefers_attr_over_name() {
    let drv = derivation("x86_64-linux.hello", "hello-2.12.3");
    let job = nix_job_from_derivation(&drv);
    assert_eq!(job.name, "x86_64-linux.hello");
    assert_eq!(job.system.as_deref(), Some("x86_64-linux"));
  }

  #[test]
  fn nix_job_falls_back_to_name_when_attr_empty() {
    let drv = derivation("", "hello-2.12.3");
    assert_eq!(nix_job_from_derivation(&drv).name, "hello-2.12.3");
  }

  #[test]
  fn nix_job_maps_outputs_dropping_unresolved() {
    let mut drv = derivation("hello", "hello");
    drv
      .outputs
      .insert("out".into(), Some("/nix/store/def-hello".into()));
    drv.outputs.insert("dev".into(), None);
    let outputs = nix_job_from_derivation(&drv).outputs.unwrap();
    assert_eq!(
      outputs.get("out").map(String::as_str),
      Some("/nix/store/def-hello")
    );
    assert!(!outputs.contains_key("dev"));
  }

  #[test]
  fn nix_job_empty_outputs_is_none() {
    assert!(
      nix_job_from_derivation(&derivation("hello", "hello"))
        .outputs
        .is_none()
    );
  }

  #[test]
  fn nix_job_maps_input_drvs() {
    let mut drv = derivation("hello", "hello");
    drv
      .input_drvs
      .insert("/nix/store/dep.drv".into(), vec!["out".to_string()]);
    let input_drvs = nix_job_from_derivation(&drv).input_drvs.unwrap();
    assert!(input_drvs.contains_key("/nix/store/dep.drv"));
  }

  #[test]
  fn nix_job_empty_constituents_is_none() {
    let mut drv = derivation("agg", "agg");
    drv.constituents = Some(vec![]);
    assert!(nix_job_from_derivation(&drv).constituents.is_none());
  }

  #[test]
  fn nix_job_nonempty_constituents_marks_aggregate() {
    let mut drv = derivation("agg", "agg");
    drv.constituents = Some(vec!["hello".into(), "world".into()]);
    let constituents = nix_job_from_derivation(&drv).constituents.unwrap();
    assert_eq!(constituents, vec!["hello".to_string(), "world".to_string()]);
  }

  #[test]
  fn nix_job_parses_meta() {
    let mut drv = derivation("hello", "hello");
    drv.meta = Some(serde_json::json!({
      "description": "greeting",
      "license": { "spdxId": "GPL-3.0-or-later" },
    }));
    let meta = nix_job_from_derivation(&drv).meta;
    assert_eq!(meta.description.as_deref(), Some("greeting"));
    assert_eq!(meta.license.as_deref(), Some("GPL-3.0-or-later"));
  }
}
