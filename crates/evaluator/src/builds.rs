use std::{
  collections::{HashMap, HashSet, VecDeque},
  path::Path,
};

use circus_common::{
  models::{CreateBuild, JobsetInput},
  repo,
};
use sqlx::PgPool;
use tokio::process::Command;
use uuid::Uuid;

async fn read_required_features(drv_path: &str) -> Vec<String> {
  circus_nix::derivation::show_required_features(&[drv_path.to_owned()])
    .await
    .unwrap_or_default()
}

#[derive(Debug, Clone)]
struct DerivationInfo {
  system:     Option<String>,
  outputs:    Option<HashMap<String, String>>,
  input_drvs: Option<HashMap<String, serde_json::Value>>,
}

fn parse_derivation_infos(
  value: &serde_json::Value,
) -> HashMap<String, DerivationInfo> {
  let Some(derivations) = value
    .get("derivations")
    .and_then(serde_json::Value::as_object)
    .or_else(|| value.as_object())
  else {
    return HashMap::new();
  };

  derivations
    .iter()
    .map(|(drv_path, drv_val)| {
      let drv_path = if drv_path.starts_with("/nix/store/") {
        drv_path.clone()
      } else {
        format!("/nix/store/{drv_path}")
      };
      let system = drv_val
        .get("system")
        .and_then(serde_json::Value::as_str)
        .map(str::to_owned);
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
                .and_then(serde_json::Value::as_str)
                .map(|path| (name.clone(), path.to_string()))
            })
            .collect::<HashMap<_, _>>()
        })
        .filter(|map| !map.is_empty());
      let input_drvs = drv_val.get("inputDrvs").and_then(|v| {
        serde_json::from_value::<HashMap<String, serde_json::Value>>(v.clone())
          .ok()
      });

      (drv_path, DerivationInfo {
        system,
        outputs,
        input_drvs,
      })
    })
    .collect()
}

async fn show_recursive_derivations(
  drv_paths: &[String],
) -> HashMap<String, DerivationInfo> {
  if drv_paths.is_empty() {
    return HashMap::new();
  }
  let output = Command::new("nix")
    .arg("derivation")
    .arg("show")
    .arg("--recursive")
    .args(drv_paths)
    .kill_on_drop(true)
    .output()
    .await;
  let Ok(output) = output else {
    return HashMap::new();
  };
  if !output.status.success() {
    tracing::warn!(
      stderr = %String::from_utf8_lossy(&output.stderr),
      "nix derivation show --recursive failed"
    );
    return HashMap::new();
  }
  serde_json::from_slice::<serde_json::Value>(&output.stdout)
    .map(|value| parse_derivation_infos(&value))
    .unwrap_or_default()
}

async fn output_available(path: &str) -> bool {
  let valid = Command::new("nix-store")
    .args(["--check-validity", path])
    .kill_on_drop(true)
    .status()
    .await
    .is_ok_and(|status| status.success());
  if valid {
    return true;
  }

  Command::new("nix")
    .args(["path-info", "--json", path])
    .kill_on_drop(true)
    .status()
    .await
    .is_ok_and(|status| status.success())
}

async fn should_enqueue_derivation(info: &DerivationInfo) -> bool {
  let Some(outputs) = &info.outputs else {
    return true;
  };
  for output in outputs.values() {
    if !output_available(output).await {
      return true;
    }
  }
  false
}

fn dependency_job_name(drv_path: &str) -> String {
  let basename = Path::new(drv_path)
    .file_name()
    .and_then(|name| name.to_str())
    .unwrap_or(drv_path)
    .trim_end_matches(".drv");
  format!("{}{basename}", circus_common::models::DEPENDENCY_JOB_PREFIX)
}

async fn expand_derivation_graph(
  jobs: &[crate::nix::NixJob],
) -> Vec<crate::nix::NixJob> {
  let top_level_drvs = jobs
    .iter()
    .map(|job| job.drv_path.clone())
    .collect::<Vec<_>>();
  let derivations = show_recursive_derivations(&top_level_drvs).await;
  if derivations.is_empty() {
    return jobs.to_vec();
  }

  let mut expanded = jobs.to_vec();
  let mut included = expanded
    .iter()
    .map(|job| job.drv_path.clone())
    .collect::<HashSet<_>>();
  let mut queued = VecDeque::new();
  for job in jobs {
    if let Some(input_drvs) = &job.input_drvs {
      queued.extend(input_drvs.keys().cloned());
    }
  }

  while let Some(drv_path) = queued.pop_front() {
    if included.contains(&drv_path) {
      continue;
    }
    let Some(info) = derivations.get(&drv_path) else {
      continue;
    };
    if !should_enqueue_derivation(info).await {
      continue;
    }

    included.insert(drv_path.clone());
    if let Some(input_drvs) = &info.input_drvs {
      queued.extend(input_drvs.keys().cloned());
    }
    expanded.push(crate::nix::NixJob {
      name: dependency_job_name(&drv_path),
      drv_path,
      system: info.system.clone(),
      outputs: info.outputs.clone(),
      input_drvs: info.input_drvs.clone(),
      constituents: None,
      meta: crate::nix::NixMeta::default(),
    });
  }

  expanded
}

/// Detect whether a derivation is a fixed-output derivation by reading the
/// `.drv` file and checking for `outputHash` in its env vars.
///
/// # Returns
///
/// Returns `(is_fod, fod_hash)`.
fn detect_fod(drv_path: &str) -> (bool, Option<String>) {
  let Ok(content) = std::fs::read_to_string(drv_path) else {
    return (false, None);
  };
  // ATerm format: ("outputHash","<hash>")
  let marker = "\"outputHash\",\"";
  let Some(start) = content.find(marker) else {
    return (false, None);
  };
  let rest = &content[start + marker.len()..];
  let Some(end) = rest.find('"') else {
    return (false, None);
  };
  let hash = &rest[..end];
  if hash.is_empty() {
    (false, None)
  } else {
    (true, Some(hash.to_string()))
  }
}

/// Create build records from evaluation results, resolving dependencies.
pub(crate) async fn create_builds_from_eval(
  pool: &PgPool,
  eval_id: Uuid,
  eval_result: &crate::nix::EvalResult,
) -> color_eyre::Result<()> {
  let mut drv_to_build: HashMap<String, Uuid> = HashMap::new();
  let mut name_to_build: HashMap<String, Uuid> = HashMap::new();
  let jobs = expand_derivation_graph(&eval_result.jobs).await;

  for job in &jobs {
    let outputs_json = job
      .outputs
      .as_ref()
      .map(|o| serde_json::to_value(o).unwrap_or_default());
    let constituents_json = job
      .constituents
      .as_ref()
      .map(|c| serde_json::to_value(c).unwrap_or_default());
    let is_aggregate = job.constituents.is_some();

    let (is_fod, fod_hash) = detect_fod(&job.drv_path);
    let required_features = read_required_features(&job.drv_path).await;
    let build = repo::builds::create(pool, CreateBuild {
      evaluation_id: eval_id,
      job_name: job.name.clone(),
      drv_path: job.drv_path.clone(),
      system: job.system.clone(),
      outputs: outputs_json,
      is_aggregate: Some(is_aggregate),
      constituents: constituents_json,
      is_fod: Some(is_fod),
      fod_hash,
      meta_description: job.meta.description.clone(),
      meta_license: job.meta.license.clone(),
      meta_homepage: job.meta.homepage.clone(),
      meta_maintainers: job.meta.maintainers.clone(),
      required_features,
    })
    .await?;

    drv_to_build.insert(job.drv_path.clone(), build.id);
    name_to_build.insert(job.name.clone(), build.id);
  }

  // Resolve dependencies
  for job in &jobs {
    let build_id = match drv_to_build.get(&job.drv_path) {
      Some(id) => *id,
      None => continue,
    };

    // Input derivation dependencies
    if let Some(ref input_drvs) = job.input_drvs {
      for dep_drv in input_drvs.keys() {
        if let Some(&dep_build_id) = drv_to_build.get(dep_drv)
          && dep_build_id != build_id
          && let Err(e) =
            repo::build_dependencies::create(pool, build_id, dep_build_id).await
        {
          tracing::warn!(build_id = %build_id, dep = %dep_build_id, "Failed to create build dependency: {e}");
        }
      }
    }

    // Aggregate constituent dependencies
    if let Some(ref constituents) = job.constituents {
      for constituent_name in constituents {
        if let Some(&dep_build_id) = name_to_build.get(constituent_name)
          && dep_build_id != build_id
          && let Err(e) =
            repo::build_dependencies::create(pool, build_id, dep_build_id).await
        {
          tracing::warn!(build_id = %build_id, dep = %dep_build_id, "Failed to create constituent dependency: {e}");
        }
      }
    }
  }

  Ok(())
}

/// Compute a deterministic hash over the commit and all jobset inputs.
/// Used for evaluation caching, so skip re-eval when inputs haven't changed.
pub(crate) fn compute_inputs_hash(
  commit_hash: &str,
  inputs: &[JobsetInput],
) -> String {
  use sha2::{Digest, Sha256};

  let mut hasher = Sha256::new();
  hasher.update(commit_hash.as_bytes());

  // Sort inputs by name for deterministic hashing
  let mut sorted_inputs: Vec<&JobsetInput> = inputs.iter().collect();
  sorted_inputs.sort_by_key(|i| &i.name);

  for input in sorted_inputs {
    hasher.update(input.name.as_bytes());
    hasher.update(input.input_type.as_str().as_bytes());
    hasher.update(input.value.as_bytes());
    if let Some(ref rev) = input.revision {
      hasher.update(rev.as_bytes());
    }
  }

  hex::encode(hasher.finalize())
}
