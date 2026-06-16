use std::{collections::HashMap, path::Path, time::Duration};

use circus_common::{
  CiError,
  config::EvaluatorConfig,
  error::Result,
  models::JobsetInput,
};
use serde::Deserialize;

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

/// Raw deserialization target for nix-eval-jobs output.
/// nix-eval-jobs emits both `attr` (attribute path) and `name` (derivation
/// name) in the same JSON object. We deserialize them separately and prefer
/// `attr` as the job identifier.
#[derive(Deserialize)]
struct RawNixJob {
  name:         Option<String>,
  attr:         Option<String>,
  #[serde(alias = "drvPath")]
  drv_path:     Option<String>,
  system:       Option<String>,
  outputs:      Option<HashMap<String, String>>,
  #[serde(alias = "inputDrvs")]
  input_drvs:   Option<HashMap<String, serde_json::Value>>,
  constituents: Option<Vec<String>>,
  /// `meta` is freeform in nixpkgs (description, license, maintainers,
  /// homepage, ...). nix-eval-jobs forwards it verbatim when
  /// `--meta` (or the default in newer versions) is set; older
  /// invocations omit it entirely and this stays `None`.
  meta:         Option<serde_json::Value>,
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

/// An error reported by nix-eval-jobs for a single job.
#[derive(Debug, Clone, Deserialize)]
struct NixEvalError {
  attr:  Option<String>,
  name:  Option<String>,
  error: String,
}

/// Result of evaluating nix expressions.
pub struct EvalResult {
  pub jobs:        Vec<NixJob>,
  pub error_count: usize,
}

/// Parse nix-eval-jobs output lines into jobs and error counts.
/// Extracted as a testable function from the inline parsing loops.
pub fn parse_eval_output(stdout: &str) -> EvalResult {
  let mut jobs = Vec::new();
  let mut error_count = 0;

  for line in stdout.lines() {
    if line.trim().is_empty() {
      continue;
    }

    if let Ok(parsed) = serde_json::from_str::<serde_json::Value>(line)
      && parsed.get("error").is_some()
    {
      if let Ok(eval_err) = serde_json::from_str::<NixEvalError>(line) {
        let name = eval_err
          .attr
          .as_deref()
          .or(eval_err.name.as_deref())
          .unwrap_or("<unknown>");
        tracing::warn!(
          job = name,
          "nix-eval-jobs reported error: {}",
          eval_err.error
        );
        error_count += 1;
      }
      continue;
    }

    match serde_json::from_str::<RawNixJob>(line) {
      Ok(raw) => {
        // drv_path is required for a valid job
        if let Some(drv_path) = raw.drv_path {
          let meta = parse_meta(raw.meta.as_ref());
          jobs.push(NixJob {
            name: raw.attr.or(raw.name).unwrap_or_default(),
            drv_path,
            system: raw.system,
            outputs: raw.outputs,
            input_drvs: raw.input_drvs,
            meta,
            // nix-eval-jobs emits `"constituents": []` for ordinary jobs; only
            // a non-empty list denotes an aggregate. Treat empty as None so
            // ordinary builds are not misclassified as aggregates, which the
            // queue runner never builds.
            constituents: raw.constituents.filter(|c| !c.is_empty()),
          });
        }
      },
      Err(e) => {
        tracing::warn!("Failed to parse nix-eval-jobs line: {e}");
      },
    }
  }

  EvalResult { jobs, error_count }
}

/// Evaluate nix expressions and return discovered jobs.
/// If `flake_mode` is true, uses nix-eval-jobs with --flake flag.
/// If `flake_mode` is false, evaluates a legacy expression file.
///
/// # Errors
///
/// Returns error if nix evaluation command fails or times out.
#[tracing::instrument(skip(config, inputs), fields(flake_mode, nix_expression))]
pub async fn evaluate(
  repo_path: &Path,
  nix_expression: &str,
  flake_mode: bool,
  timeout: Duration,
  config: &EvaluatorConfig,
  inputs: &[JobsetInput],
) -> Result<EvalResult> {
  // Validate nix expression before constructing any commands
  circus_common::nix::validate::validate_nix_expression(nix_expression)
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
    evaluate_flake(repo_path, nix_expression, timeout, config, inputs).await
  } else {
    evaluate_legacy(repo_path, nix_expression, timeout, config, inputs).await
  }
}

/// nix-eval-jobs chokes on raw `nixosConfigurations.<name>`, so drill into the
/// toplevel derivation.
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

#[tracing::instrument(skip(config, inputs))]
async fn evaluate_flake(
  repo_path: &Path,
  nix_expression: &str,
  timeout: Duration,
  config: &EvaluatorConfig,
  inputs: &[JobsetInput],
) -> Result<EvalResult> {
  if nix_expression == "nixosConfigurations" {
    return evaluate_all_nixos_configs(repo_path, timeout, config, inputs)
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

  let flake_ref = format!("{}#{effective_expr}", repo_path.display());

  tracing::debug!(flake_ref = %flake_ref, "Running nix-eval-jobs");

  tokio::time::timeout(timeout, async {
    let mut cmd = tokio::process::Command::new("nix-eval-jobs");
    cmd.arg("--flake").arg(&flake_ref).arg("--force-recurse");
    // Surface meta.{description, license, homepage, maintainers} so the
    // channel tarball can carry them through to nix-env / nix search.
    cmd.arg("--meta");
    // `inputDrvs` is what create_builds_from_eval wires into build_dependencies
    cmd.arg("--show-input-drvs");
    // nix-eval-jobs does not support --no-write-lock-file; it does not
    // write lock files during evaluation so the flag is unnecessary here.
    // The nix-native commands (nix eval, nix flake show) DO pass it.
    cmd.kill_on_drop(true);

    if config.restrict_eval {
      cmd.args(["--option", "restrict-eval", "true"]);
    }
    if !config.allow_ifd {
      cmd.args(["--option", "allow-import-from-derivation", "false"]);
    }
    for input in inputs {
      if input.input_type == "git" {
        circus_common::nix::validate::validate_jobset_input(
          &input.name,
          &input.input_type,
          &input.value,
          input.revision.as_deref(),
        )
        .map_err(|e| CiError::NixEval(format!("Invalid jobset input: {e}")))?;
        cmd.args(["--override-input", &input.name, &input.value]);
      }
    }

    let output = cmd.output().await;

    match output {
      Ok(out) if out.status.success() || !out.stdout.is_empty() => {
        let stdout = String::from_utf8_lossy(&out.stdout);
        let result = parse_eval_output(&stdout);

        if result.error_count > 0 {
          tracing::warn!(
            error_count = result.error_count,
            "nix-eval-jobs reported errors for some jobs"
          );
        }

        if result.jobs.is_empty() && result.error_count == 0 {
          let stderr = String::from_utf8_lossy(&out.stderr);
          if !stderr.trim().is_empty() {
            tracing::warn!(
              stderr = %stderr,
              "nix-eval-jobs returned no jobs, stderr output present"
            );
          }
        }

        Ok(result)
      },
      Ok(out) => {
        let stderr = String::from_utf8_lossy(&out.stderr);
        tracing::warn!(stderr = %stderr, "nix-eval-jobs failed");
        Err(CiError::NixEval("Nix evaluation failed".to_string()))
      },
      Err(e) => {
        Err(CiError::NixEval(format!(
          "Failed to run nix-eval-jobs: {e}"
        )))
      },
    }
  })
  .await
  .map_err(|_| {
    CiError::Timeout(format!("Nix evaluation timed out after {timeout:?}"))
  })?
}

/// Resolve all toplevels in one nix eval.
async fn evaluate_all_nixos_configs(
  repo_path: &Path,
  _timeout: Duration,
  config: &EvaluatorConfig,
  _inputs: &[JobsetInput],
) -> Result<EvalResult> {
  let flake_ref = format!("{}#nixosConfigurations", repo_path.display());

  let expr = "builtins.mapAttrs (_: v: v.config.system.build.toplevel)";
  let mut cmd = tokio::process::Command::new("nix");
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
  if config.restrict_eval {
    cmd.args(["--option", "restrict-eval", "true"]);
  }
  if !config.allow_ifd {
    cmd.args(["--option", "allow-import-from-derivation", "false"]);
  }
  let output = cmd.output().await.map_err(|e| {
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
  let out = tokio::process::Command::new("nix")
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

  tokio::time::timeout(timeout, async {
    // Try nix-eval-jobs without --flake for legacy expressions
    let mut cmd = tokio::process::Command::new("nix-eval-jobs");
    cmd.arg(&expr_path).arg("--force-recurse");
    cmd.arg("--meta");
    cmd.arg("--show-input-drvs");
    cmd.kill_on_drop(true);

    if config.restrict_eval {
      cmd.args(["--option", "restrict-eval", "true"]);
    }
    if !config.allow_ifd {
      cmd.args(["--option", "allow-import-from-derivation", "false"]);
    }
    for input in inputs {
      circus_common::nix::validate::validate_jobset_input(
        &input.name,
        &input.input_type,
        &input.value,
        input.revision.as_deref(),
      )
      .map_err(|e| CiError::NixEval(format!("Invalid jobset input: {e}")))?;
      match input.input_type.as_str() {
        "string" | "git" => {
          cmd.args(["--argstr", &input.name, &input.value]);
        },
        "boolean" => {
          if input.value == "true" || input.value == "false" {
            cmd.args(["--arg", &input.name, &input.value]);
          } else {
            return Err(CiError::NixEval(format!(
              "Invalid boolean input '{}': expected true or false",
              input.name
            )));
          }
        },
        "build" => {
          cmd.args(["--arg", &input.name, &input.value]);
        },
        _ => {
          tracing::warn!(
            input_name = %input.name,
            input_type = %input.input_type,
            "Unrecognized jobset input type in legacy mode, skipping"
          );
        },
      }
    }

    let output = cmd.output().await;

    match output {
      Ok(out) if out.status.success() || !out.stdout.is_empty() => {
        let stdout = String::from_utf8_lossy(&out.stdout);
        Ok(parse_eval_output(&stdout))
      },
      Ok(out) => {
        let stderr = String::from_utf8_lossy(&out.stderr);
        tracing::warn!(stderr = %stderr, "legacy nix-eval-jobs failed");
        Err(CiError::NixEval("Nix evaluation failed".to_string()))
      },
      Err(e) => {
        Err(CiError::NixEval(format!(
          "Failed to run nix-eval-jobs: {e}"
        )))
      },
    }
  })
  .await
  .map_err(|_| {
    CiError::Timeout(format!("Nix evaluation timed out after {timeout:?}"))
  })?
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
}
