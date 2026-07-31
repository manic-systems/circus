use std::{
  collections::{HashMap, HashSet, VecDeque},
  path::Path,
};

use circus_common::{
  PgPool,
  models::{CreateBuild, EvaluationStatus, JobsetInput},
  repo,
};
use tokio::process::Command;
use uuid::Uuid;

use crate::memory::MemoryLimit;

async fn read_required_features(
  drv_path: &str,
  memory_limit: MemoryLimit,
) -> Vec<String> {
  let mut command =
    circus_nix::derivation::required_features_command(&[drv_path.to_owned()]);
  command.kill_on_drop(true);
  if memory_limit.apply_to(&mut command).is_err() {
    return Vec::new();
  }
  let Ok(output) = command.output().await else {
    return Vec::new();
  };
  if !output.status.success() {
    return Vec::new();
  }
  serde_json::from_slice(&output.stdout)
    .map(|value| circus_nix::derivation::union_required_features(&value))
    .unwrap_or_default()
}

#[derive(Debug, Clone)]
struct DerivationInfo {
  system:            Option<String>,
  outputs:           Option<HashMap<String, String>>,
  input_drvs:        Option<HashMap<String, serde_json::Value>>,
  required_features: Vec<String>,
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
      let required_features =
        circus_nix::derivation::drv_required_features(drv_val);

      (drv_path, DerivationInfo {
        system,
        outputs,
        input_drvs,
        required_features,
      })
    })
    .collect()
}

async fn show_recursive_derivations(
  drv_paths: &[String],
  memory_limit: MemoryLimit,
) -> HashMap<String, DerivationInfo> {
  if drv_paths.is_empty() {
    return HashMap::new();
  }
  let mut command = Command::new("nix");
  command
    .arg("derivation")
    .arg("show")
    .arg("--recursive")
    .args(drv_paths)
    .kill_on_drop(true);
  if let Err(error) = memory_limit.apply_to(&mut command) {
    tracing::warn!(%error, "failed to apply evaluator memory limit");
    return HashMap::new();
  }
  let output = command.output().await;
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

/// Paths the command cannot vouch for are treated as invalid so the
/// derivation still gets enqueued.
async fn invalid_output_paths(
  derivations: &HashMap<String, DerivationInfo>,
  memory_limit: MemoryLimit,
) -> HashSet<String> {
  let paths = derivations
    .values()
    .filter_map(|info| info.outputs.as_ref())
    .flat_map(|outputs| outputs.values().cloned())
    .collect::<HashSet<String>>()
    .into_iter()
    .collect::<Vec<String>>();

  let mut invalid = HashSet::new();
  for chunk in paths.chunks(1024) {
    let mut command = Command::new("nix-store");
    command
      .args(["--check-validity", "--print-invalid"])
      .args(chunk)
      .kill_on_drop(true);
    let output = if memory_limit.apply_to(&mut command).is_ok() {
      command.output().await.ok()
    } else {
      None
    };
    match output {
      Some(output) if output.status.success() => {
        invalid.extend(
          String::from_utf8_lossy(&output.stdout)
            .lines()
            .map(str::to_owned),
        );
      },
      _ => invalid.extend(chunk.iter().cloned()),
    }
  }
  invalid
}

fn should_enqueue_derivation(
  info: &DerivationInfo,
  invalid_outputs: &HashSet<String>,
) -> bool {
  let Some(outputs) = &info.outputs else {
    return true;
  };
  outputs
    .values()
    .any(|output| invalid_outputs.contains(output))
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
  memory_limit: MemoryLimit,
) -> (Vec<crate::nix::NixJob>, HashMap<String, DerivationInfo>) {
  let top_level_drvs = jobs
    .iter()
    .map(|job| job.drv_path.clone())
    .collect::<Vec<_>>();
  let derivations =
    show_recursive_derivations(&top_level_drvs, memory_limit).await;
  if derivations.is_empty() {
    return (jobs.to_vec(), derivations);
  }
  let invalid_outputs = invalid_output_paths(&derivations, memory_limit).await;

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
    if !should_enqueue_derivation(info, &invalid_outputs) {
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

  (expanded, derivations)
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

/// Resolve the dependency edges to insert for `jobs`, deduplicated. Jobs can
/// alias one drv path (`packages.default`), so edges are anchored to each
/// job's own build id rather than the drv-keyed map, which only retains the
/// last build per drv.
fn resolve_dependency_pairs(
  jobs: &[crate::nix::NixJob],
  build_ids: &[Uuid],
  drv_to_build: &HashMap<String, Uuid>,
  name_to_build: &HashMap<String, Uuid>,
) -> Vec<(Uuid, Uuid)> {
  let mut seen = HashSet::new();
  let mut pairs = Vec::new();
  for (job, &build_id) in jobs.iter().zip(build_ids) {
    if let Some(input_drvs) = &job.input_drvs {
      for dep_drv in input_drvs.keys() {
        if let Some(&dep_build_id) = drv_to_build.get(dep_drv)
          && dep_build_id != build_id
          && seen.insert((build_id, dep_build_id))
        {
          pairs.push((build_id, dep_build_id));
        }
      }
    }

    if let Some(constituents) = &job.constituents {
      for constituent_name in constituents {
        if let Some(&dep_build_id) = name_to_build.get(constituent_name)
          && dep_build_id != build_id
          && seen.insert((build_id, dep_build_id))
        {
          pairs.push((build_id, dep_build_id));
        }
      }
    }
  }
  pairs
}

/// Create build records and finish the evaluation while it is still running.
pub(crate) async fn create_builds_from_eval(
  pool: &PgPool,
  eval_id: Uuid,
  eval_result: &crate::nix::EvalResult,
  memory_limit: MemoryLimit,
) -> color_eyre::Result<bool> {
  let mut drv_to_build: HashMap<String, Uuid> = HashMap::new();
  let mut name_to_build: HashMap<String, Uuid> = HashMap::new();
  let (jobs, derivations) =
    expand_derivation_graph(&eval_result.jobs, memory_limit).await;
  let mut builds = Vec::with_capacity(jobs.len());

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
    let required_features = match derivations.get(&job.drv_path) {
      Some(info) => info.required_features.clone(),
      None => read_required_features(&job.drv_path, memory_limit).await,
    };
    builds.push(CreateBuild {
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
    });
  }

  let mut client = pool.get().await?;
  let tx = client.transaction().await?;
  if !repo::evaluations::lock_running(&tx, eval_id).await? {
    return Ok(false);
  }

  let mut build_ids = Vec::with_capacity(jobs.len());
  for build in builds {
    let drv_path = build.drv_path.clone();
    let job_name = build.job_name.clone();
    let id = repo::builds::create_in_transaction(&tx, build).await?.id;

    drv_to_build.insert(drv_path, id);
    name_to_build.insert(job_name, id);
    build_ids.push(id);
  }

  for (build_id, dep_build_id) in
    resolve_dependency_pairs(&jobs, &build_ids, &drv_to_build, &name_to_build)
  {
    repo::build_dependencies::create_in_transaction(
      &tx,
      build_id,
      dep_build_id,
    )
    .await?;
  }

  if !repo::evaluations::finish_running_in_transaction(
    &tx,
    eval_id,
    EvaluationStatus::Completed,
    None,
  )
  .await?
  {
    return Err(color_eyre::eyre::eyre!(
      "evaluation {eval_id} lost its running state while locked"
    ));
  }
  tx.commit().await?;
  Ok(true)
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

#[cfg(test)]
mod tests {
  use std::collections::HashMap;

  use uuid::Uuid;

  use super::resolve_dependency_pairs;
  use crate::nix::{NixJob, NixMeta};

  fn job(name: &str, drv_path: &str, input_drvs: &[&str]) -> NixJob {
    NixJob {
      name:         name.to_owned(),
      drv_path:     drv_path.to_owned(),
      system:       None,
      outputs:      None,
      input_drvs:   (!input_drvs.is_empty()).then(|| {
        input_drvs
          .iter()
          .map(|drv| ((*drv).to_owned(), serde_json::Value::Null))
          .collect()
      }),
      constituents: None,
      meta:         NixMeta::default(),
    }
  }

  #[test]
  fn aliased_jobs_get_unique_edges_on_their_own_builds() {
    let jobs = [
      job("packages.x86_64-linux.default", "/drv/pkg.drv", &[
        "/drv/dep.drv",
      ]),
      job("packages.x86_64-linux.pkg", "/drv/pkg.drv", &[
        "/drv/dep.drv",
      ]),
      job("drv:dep", "/drv/dep.drv", &[]),
    ];
    let build_ids = [Uuid::new_v4(), Uuid::new_v4(), Uuid::new_v4()];
    let drv_to_build = HashMap::from([
      ("/drv/pkg.drv".to_owned(), build_ids[1]),
      ("/drv/dep.drv".to_owned(), build_ids[2]),
    ]);
    let name_to_build = jobs
      .iter()
      .zip(build_ids)
      .map(|(job, id)| (job.name.clone(), id))
      .collect();

    let pairs = resolve_dependency_pairs(
      &jobs,
      &build_ids,
      &drv_to_build,
      &name_to_build,
    );

    assert_eq!(pairs, vec![
      (build_ids[0], build_ids[2]),
      (build_ids[1], build_ids[2]),
    ]);
  }
}
