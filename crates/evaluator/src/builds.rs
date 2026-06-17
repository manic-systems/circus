use std::collections::HashMap;

use circus_common::{
  models::{CreateBuild, JobsetInput},
  repo,
};
use sqlx::PgPool;
use uuid::Uuid;

async fn read_required_features(drv_path: &str) -> Vec<String> {
  circus_common::nix::derivation::show_required_features(&[drv_path.to_owned()])
    .await
    .unwrap_or_default()
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

  for job in &eval_result.jobs {
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
  for job in &eval_result.jobs {
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
