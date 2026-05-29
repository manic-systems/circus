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
          arr
            .iter()
            .filter_map(|v| v.as_str())
            .next()
            .map(str::to_owned)
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
  circus_common::validate::validate_nix_expression(nix_expression)
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

#[tracing::instrument(skip(config, inputs))]
async fn evaluate_flake(
  repo_path: &Path,
  nix_expression: &str,
  timeout: Duration,
  config: &EvaluatorConfig,
  inputs: &[JobsetInput],
) -> Result<EvalResult> {
  let flake_ref = format!("{}#{}", repo_path.display(), nix_expression);

  tracing::debug!(flake_ref = %flake_ref, "Running nix-eval-jobs");

  tokio::time::timeout(timeout, async {
    let mut cmd = tokio::process::Command::new("nix-eval-jobs");
    cmd.arg("--flake").arg(&flake_ref).arg("--force-recurse");
    // Surface meta.{description, license, homepage, maintainers} so the
    // channel tarball can carry them through to nix-env / nix search.
    cmd.arg("--meta");
    // Evaluation must never mutate the consumer's flake.lock. Without this,
    // nix-eval-jobs can race against the checked-out working tree and write a
    // refreshed lock that the next eval then disagrees with.
    cmd.arg("--no-write-lock-file");
    cmd.kill_on_drop(true);

    if config.restrict_eval {
      cmd.args(["--option", "restrict-eval", "true"]);
    }
    if !config.allow_ifd {
      cmd.args(["--option", "allow-import-from-derivation", "false"]);
    }
    for input in inputs {
      if input.input_type == "git" {
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
      _ => {
        tracing::info!("nix-eval-jobs unavailable, falling back to nix eval");
        let jobs = evaluate_with_nix_eval(repo_path, nix_expression).await?;
        Ok(EvalResult {
          jobs,
          error_count: 0,
        })
      },
    }
  })
  .await
  .map_err(|_| {
    CiError::Timeout(format!("Nix evaluation timed out after {timeout:?}"))
  })?
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
  let expr_path = repo_path.join(nix_expression);

  tokio::time::timeout(timeout, async {
    // Try nix-eval-jobs without --flake for legacy expressions
    let mut cmd = tokio::process::Command::new("nix-eval-jobs");
    cmd.arg(&expr_path).arg("--force-recurse");
    cmd.arg("--meta");
    cmd.kill_on_drop(true);

    if config.restrict_eval {
      cmd.args(["--option", "restrict-eval", "true"]);
    }
    if !config.allow_ifd {
      cmd.args(["--option", "allow-import-from-derivation", "false"]);
    }
    for input in inputs {
      match input.input_type.as_str() {
        "string" | "path" => {
          cmd.args(["--arg", &input.name, &input.value]);
        },
        // Legacy expressions can't take a git override the way flakes do,
        // but the input value (a fetched path) is meaningful as a path
        // argument. Threading it through as --arg preserves the input's
        // effect on evaluation instead of silently dropping it.
        "git" => {
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
      _ => {
        // Degraded path: nix-eval-jobs unavailable. The fabricated jobs below
        // have no name, system, outputs, input_drvs, or constituents, which
        // breaks per-system scheduling, dep wiring, FOD detection, and dedup.
        // This should be rare; log loudly so the deployment notices.
        tracing::error!(
          "nix-eval-jobs unavailable for legacy expr; falling back to \
           nix-instantiate. Scheduling, dependency wiring, and dedup will be \
           degraded for this evaluation."
        );
        let output = tokio::process::Command::new("nix-instantiate")
          .arg(&expr_path)
          .arg("--strict")
          .arg("--json")
          .kill_on_drop(true)
          .output()
          .await
          .map_err(|e| {
            CiError::NixEval(format!("nix-instantiate failed: {e}"))
          })?;

        if !output.status.success() {
          let stderr = String::from_utf8_lossy(&output.stderr);
          return Err(CiError::NixEval(format!(
            "nix-instantiate failed: {stderr}"
          )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        // nix-instantiate --json outputs the derivation path(s)
        let drv_paths: Vec<String> =
          serde_json::from_str(&stdout).map_err(|e| {
            CiError::NixEval(format!(
              "Failed to parse nix-instantiate output: {e}"
            ))
          })?;
        let jobs: Vec<NixJob> = drv_paths
          .into_iter()
          .enumerate()
          .map(|(i, drv_path)| {
            NixJob {
              name: format!("job-{i}"),
              drv_path,
              system: None,
              outputs: None,
              input_drvs: None,
              constituents: None,
              meta: NixMeta::default(),
            }
          })
          .collect();

        Ok(EvalResult {
          jobs,
          error_count: 0,
        })
      },
    }
  })
  .await
  .map_err(|_| {
    CiError::Timeout(format!("Nix evaluation timed out after {timeout:?}"))
  })?
}

/// Recursively flatten a nix eval --json value into (attr_path, drv_path)
/// pairs. String values are treated as derivation paths. Objects are recursed
/// into. Other types are skipped.
fn flatten_attrs(
  prefix: &str,
  value: &serde_json::Value,
) -> Vec<(String, String)> {
  match value {
    serde_json::Value::String(s) => {
      // Store path - likely a derivation
      vec![(prefix.to_string(), s.clone())]
    },
    serde_json::Value::Object(map) => {
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

async fn evaluate_with_nix_eval(
  repo_path: &Path,
  nix_expression: &str,
) -> Result<Vec<NixJob>> {
  let flake_ref = format!("{}#{}", repo_path.display(), nix_expression);

  let output = tokio::process::Command::new("nix")
    .args(["eval", "--json", &flake_ref])
    .kill_on_drop(true)
    .output()
    .await
    .map_err(|e| CiError::NixEval(format!("Failed to run nix eval: {e}")))?;

  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    return Err(CiError::NixEval(format!("nix eval failed: {stderr}")));
  }

  // Parse the JSON output - expecting an attrset of name -> derivation
  let stdout = String::from_utf8_lossy(&output.stdout);
  let value: serde_json::Value =
    serde_json::from_str(&stdout).map_err(|e| {
      CiError::NixEval(format!("Failed to parse nix eval output: {e}"))
    })?;

  let entries = flatten_attrs("", &value);

  let mut jobs = Vec::new();
  for (name, drv_path) in entries {
    // Fetch system via nix derivation show
    let drv_ref =
      format!("{}#{}.{}", repo_path.display(), nix_expression, name);
    let drv_output = tokio::process::Command::new("nix")
      .args(["derivation", "show", &drv_ref])
      .kill_on_drop(true)
      .output()
      .await;

    let system = match drv_output {
      Ok(out) if out.status.success() => {
        let drv_stdout = String::from_utf8_lossy(&out.stdout);
        if let Ok(drv_json) =
          serde_json::from_str::<serde_json::Value>(&drv_stdout)
          // Newer `nix derivation show` wraps output as
          // `{"derivations": {<drv>: {...}}, "version": N}`; older nix keys the
          // drv paths directly at the top level. Without unwrapping the
          // "derivations" key, the first top-level key parsed as the drv_path
          // becomes the literal string "derivations".
          && let Some((_, drv_val)) = drv_json
            .get("derivations")
            .and_then(serde_json::Value::as_object)
            .or_else(|| drv_json.as_object())
            .and_then(|o| o.iter().next())
        {
          drv_val
            .get("system")
            .and_then(|v| v.as_str())
            .map(std::string::ToString::to_string)
        } else {
          None
        }
      },
      _ => None,
    };

    jobs.push(NixJob {
      name: name.clone(),
      drv_path,
      system,
      outputs: None,
      input_drvs: None,
      constituents: None,
      meta: NixMeta::default(),
    });
  }

  Ok(jobs)
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
}
