use std::{
  collections::{HashMap, HashSet, VecDeque},
  path::Path,
};

use circus_common::{
  PgPool,
  models::{BuildKind, CreateBuild, EvaluationStatus, JobsetInput},
  repo,
  systems::system_allowed,
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
  is_effect:         bool,
  wants_git_token:   bool,
}

fn effect_marker(value: &serde_json::Value) -> bool {
  match value {
    serde_json::Value::Bool(value) => *value,
    serde_json::Value::Number(value) => value.as_i64() == Some(1),
    serde_json::Value::String(value) => {
      matches!(value.as_str(), "1" | "true")
    },
    _ => false,
  }
}

fn derivation_is_effect(value: &serde_json::Value) -> bool {
  if value
    .get("structuredAttrs")
    .and_then(|attrs| attrs.get("isEffect"))
    .is_some_and(effect_marker)
  {
    return true;
  }

  let Some(env) = value.get("env") else {
    return false;
  };
  if env.get("isEffect").is_some_and(effect_marker) {
    return true;
  }

  env
    .get("__json")
    .and_then(serde_json::Value::as_str)
    .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok())
    .is_some_and(|attrs| attrs.get("isEffect").is_some_and(effect_marker))
}

/// Whether `secretsToUse` or `secretsMap` asks for a `GitToken`.
fn derivation_wants_git_token(value: &serde_json::Value) -> bool {
  let json_attrs = value
    .get("env")
    .and_then(|env| env.get("__json"))
    .and_then(serde_json::Value::as_str)
    .and_then(|json| serde_json::from_str::<serde_json::Value>(json).ok());
  let sources = [
    value.get("env"),
    value.get("structuredAttrs"),
    json_attrs.as_ref(),
  ];
  sources.into_iter().flatten().any(|attrs| {
    ["secretsToUse", "secretsMap"].into_iter().any(|key| {
      let map = match attrs.get(key) {
        Some(serde_json::Value::String(json)) => {
          serde_json::from_str(json).ok()
        },
        Some(map) => Some(map.clone()),
        None => None,
      };
      map
        .as_ref()
        .and_then(serde_json::Value::as_object)
        .is_some_and(|map| {
          map.values().any(|secret| {
            secret.get("type").and_then(serde_json::Value::as_str)
              == Some("GitToken")
          })
        })
    })
  })
}

fn store_path(path: &str) -> String {
  if path.starts_with("/nix/store/") {
    path.to_owned()
  } else {
    format!("/nix/store/{path}")
  }
}

fn derivation_inputs(
  value: &serde_json::Value,
) -> Option<HashMap<String, serde_json::Value>> {
  value
    .get("inputDrvs")
    .or_else(|| value.get("inputs").and_then(|inputs| inputs.get("drvs")))
    .and_then(serde_json::Value::as_object)
    .map(|inputs| {
      inputs
        .iter()
        .map(|(path, input)| (store_path(path), input.clone()))
        .collect()
    })
}

fn parse_derivation_infos(
  value: &serde_json::Value,
) -> color_eyre::Result<HashMap<String, DerivationInfo>> {
  let Some(root) = value.as_object() else {
    return Err(color_eyre::eyre::eyre!(
      "nix derivation show output must be a JSON object"
    ));
  };

  let derivations = match root.get("derivations") {
    Some(value) => {
      value.as_object().ok_or_else(|| {
        color_eyre::eyre::eyre!(
          "nix derivation show output has a non-object derivations field"
        )
      })?
    },
    None => root,
  };

  let mut parsed = HashMap::with_capacity(derivations.len());
  for (drv_path, drv_val) in derivations {
    if !drv_val.is_object() {
      return Err(color_eyre::eyre::eyre!(
        "nix derivation show entry {drv_path:?} must be a JSON object"
      ));
    }

    let drv_path = store_path(drv_path);
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
              .map(|path| (name.clone(), store_path(path)))
          })
          .collect::<HashMap<_, _>>()
      })
      .filter(|map| !map.is_empty());
    let input_drvs = derivation_inputs(drv_val);
    let required_features =
      circus_nix::derivation::drv_required_features(drv_val);
    let is_effect = derivation_is_effect(drv_val);
    let wants_git_token = is_effect && derivation_wants_git_token(drv_val);

    parsed.insert(drv_path, DerivationInfo {
      system,
      outputs,
      input_drvs,
      required_features,
      is_effect,
      wants_git_token,
    });
  }
  Ok(parsed)
}

fn parse_derivation_output(
  output: &[u8],
  requested_drv_paths: &[String],
) -> color_eyre::Result<HashMap<String, DerivationInfo>> {
  let value =
    serde_json::from_slice::<serde_json::Value>(output).map_err(|error| {
      color_eyre::eyre::eyre!(
        "invalid nix derivation show --recursive JSON: {error}"
      )
    })?;
  let derivations = parse_derivation_infos(&value)?;
  let missing = requested_drv_paths
    .iter()
    .map(|path| store_path(path))
    .filter(|path| !derivations.contains_key(path))
    .collect::<Vec<_>>();
  if !missing.is_empty() {
    return Err(color_eyre::eyre::eyre!(
      "nix derivation show --recursive omitted top-level derivation(s): {}",
      missing.join(", ")
    ));
  }
  Ok(derivations)
}

async fn show_recursive_derivations(
  drv_paths: &[String],
  memory_limit: MemoryLimit,
) -> color_eyre::Result<HashMap<String, DerivationInfo>> {
  if drv_paths.is_empty() {
    return Ok(HashMap::new());
  }
  let mut command = Command::new("nix");
  command
    .arg("derivation")
    .arg("show")
    .arg("--recursive")
    .args(drv_paths)
    .kill_on_drop(true);
  memory_limit.apply_to(&mut command).map_err(|error| {
    color_eyre::eyre::eyre!(
      "failed to apply evaluator memory limit while inspecting derivations: \
       {error}"
    )
  })?;
  let output = command.output().await.map_err(|error| {
    color_eyre::eyre::eyre!(
      "failed to run nix derivation show --recursive: {error}"
    )
  })?;
  if !output.status.success() {
    let stderr = String::from_utf8_lossy(&output.stderr);
    return Err(color_eyre::eyre::eyre!(
      "nix derivation show --recursive exited with {}: {}",
      output.status,
      stderr.trim()
    ));
  }
  parse_derivation_output(&output.stdout, drv_paths)
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

fn should_enqueue_outputs(
  outputs: Option<&HashMap<String, String>>,
  invalid_outputs: &HashSet<String>,
) -> bool {
  let Some(outputs) = outputs else {
    return true;
  };
  outputs
    .values()
    .any(|output| invalid_outputs.contains(output))
}

fn selected_derivation_outputs(
  info: &DerivationInfo,
  input: &serde_json::Value,
) -> Option<HashMap<String, String>> {
  let requested = input
    .as_array()
    .or_else(|| input.get("outputs").and_then(serde_json::Value::as_array))
    .map(|outputs| {
      outputs
        .iter()
        .filter_map(serde_json::Value::as_str)
        .collect::<HashSet<_>>()
    })
    .filter(|outputs| !outputs.is_empty());
  match (&info.outputs, requested) {
    (Some(outputs), Some(requested)) => {
      Some(
        outputs
          .iter()
          .filter(|(name, _)| requested.contains(name.as_str()))
          .map(|(name, path)| (name.clone(), path.clone()))
          .collect(),
      )
    },
    (outputs, None) => outputs.clone(),
    (None, Some(_)) => None,
  }
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
  derivations: &HashMap<String, DerivationInfo>,
  memory_limit: MemoryLimit,
) -> Vec<crate::nix::NixJob> {
  if derivations.is_empty() {
    return jobs.to_vec();
  }
  let invalid_outputs = invalid_output_paths(derivations, memory_limit).await;

  let mut expanded = jobs.to_vec();
  let mut included = expanded
    .iter()
    .map(|job| job.drv_path.clone())
    .collect::<HashSet<_>>();
  let mut queued = VecDeque::new();
  for job in jobs {
    if let Some(input_drvs) = &job.input_drvs {
      queued.extend(
        input_drvs
          .iter()
          .map(|(path, input)| (path.clone(), input.clone())),
      );
    }
  }

  while let Some((drv_path, requested_input)) = queued.pop_front() {
    if included.contains(&drv_path) {
      if let Some(info) = derivations.get(&drv_path)
        && let Some(requested) =
          selected_derivation_outputs(info, &requested_input)
        && let Some(existing) =
          expanded.iter_mut().find(|job| job.drv_path == drv_path)
      {
        existing
          .outputs
          .get_or_insert_with(HashMap::new)
          .extend(requested);
      }
      continue;
    }
    let Some(info) = derivations.get(&drv_path) else {
      continue;
    };
    let outputs = selected_derivation_outputs(info, &requested_input);
    if !should_enqueue_outputs(outputs.as_ref(), &invalid_outputs) {
      continue;
    }

    included.insert(drv_path.clone());
    if let Some(input_drvs) = &info.input_drvs {
      queued.extend(
        input_drvs
          .iter()
          .map(|(path, input)| (path.clone(), input.clone())),
      );
    }
    expanded.push(crate::nix::NixJob {
      name: dependency_job_name(&drv_path),
      drv_path,
      system: info.system.clone(),
      outputs,
      input_drvs: info.input_drvs.clone(),
      constituents: None,
      meta: crate::nix::NixMeta::default(),
    });
  }

  expanded
}

fn hydrate_top_level_derivations(
  jobs: &[crate::nix::NixJob],
  derivations: &HashMap<String, DerivationInfo>,
) -> Vec<crate::nix::NixJob> {
  jobs
    .iter()
    .cloned()
    .map(|mut job| {
      if let Some(info) = derivations.get(&store_path(&job.drv_path)) {
        job.system.clone_from(&info.system);
        job.outputs.clone_from(&info.outputs);
        job.input_drvs.clone_from(&info.input_drvs);
      }
      job
    })
    .collect()
}

/// Detect whether a derivation is a fixed-output derivation from the output
/// list of its `.drv`.
///
/// # Returns
///
/// Returns `(is_fod, fod_hash)`.
fn detect_fod(drv_path: &str) -> (bool, Option<String>) {
  let Ok(content) = std::fs::read_to_string(drv_path) else {
    return (false, None);
  };
  // ATerm: Derive([("out","<path>","<algo>","<hash>")],...), one output only.
  let Some((outputs, _)) = content
    .strip_prefix("Derive([(")
    .and_then(|rest| rest.split_once(")],"))
  else {
    return (false, None);
  };
  match outputs.split(',').collect::<Vec<_>>().as_slice() {
    ["\"out\"", _path, algo, hash] if *algo != "\"\"" && *hash != "\"\"" => {
      (true, Some(hash.trim_matches('"').to_string()))
    },
    _ => (false, None),
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
  effect_build_ids: &[Uuid],
  regular_build_ids: &[Uuid],
) -> Vec<(Uuid, Uuid)> {
  let effect_build_ids =
    effect_build_ids.iter().copied().collect::<HashSet<_>>();
  let mut seen = HashSet::new();
  let mut pairs = Vec::new();
  for (job, &build_id) in jobs.iter().zip(build_ids) {
    if let Some(input_drvs) = &job.input_drvs {
      for dep_drv in input_drvs.keys() {
        if let Some(&dep_build_id) = drv_to_build.get(dep_drv)
          && dep_build_id != build_id
          && !effect_build_ids.contains(&dep_build_id)
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
          && !effect_build_ids.contains(&dep_build_id)
          && seen.insert((build_id, dep_build_id))
        {
          pairs.push((build_id, dep_build_id));
        }
      }
    }
  }

  for &effect_id in &effect_build_ids {
    for &regular_id in regular_build_ids {
      if regular_id != effect_id && seen.insert((effect_id, regular_id)) {
        pairs.push((effect_id, regular_id));
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
  allowed_systems: Option<&HashSet<String>>,
) -> color_eyre::Result<bool> {
  let mut drv_to_build: HashMap<String, Uuid> = HashMap::new();
  let mut name_to_build: HashMap<String, Uuid> = HashMap::new();

  let top_level_drvs = eval_result
    .jobs
    .iter()
    .map(|job| job.drv_path.clone())
    .collect::<Vec<_>>();
  let derivations =
    show_recursive_derivations(&top_level_drvs, memory_limit).await?;
  let effect_drvs = eval_result
    .jobs
    .iter()
    .filter(|job| {
      derivations
        .get(&store_path(&job.drv_path))
        .is_some_and(|info| info.is_effect)
    })
    .map(|job| job.drv_path.clone())
    .collect::<HashSet<_>>();

  // Evix job metadata is shallow on some versions, so hydrate from recursive
  // derivation inspection.
  let named_jobs =
    hydrate_top_level_derivations(&eval_result.jobs, &derivations)
      .into_iter()
      .filter(|job| system_allowed(job.system.as_deref(), allowed_systems))
      .collect::<Vec<_>>();
  let expanded =
    expand_derivation_graph(&named_jobs, &derivations, memory_limit).await;
  let expanded_len = expanded.len();
  let jobs = expanded
    .into_iter()
    .filter(|job| system_allowed(job.system.as_deref(), allowed_systems))
    .collect::<Vec<_>>();
  let dropped =
    (eval_result.jobs.len() - named_jobs.len()) + (expanded_len - jobs.len());
  if dropped > 0 {
    tracing::info!(
      dropped,
      "Dropped jobs for systems outside evaluator.systems"
    );
  }
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
    let kind = if effect_drvs.contains(&job.drv_path) {
      BuildKind::Effect
    } else {
      BuildKind::Build
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
      kind,
    });
  }

  let mut client = pool.get().await?;
  let tx = client.transaction().await?;
  if !repo::evaluations::lock_running(&tx, eval_id).await? {
    return Ok(false);
  }

  let git_token_drvs = derivations
    .iter()
    .filter(|(_, info)| info.wants_git_token)
    .map(|(path, _)| path.as_str())
    .collect::<HashSet<_>>();
  let mut build_ids = Vec::with_capacity(jobs.len());
  let mut effect_build_ids = Vec::new();
  let mut regular_build_ids = Vec::new();
  for build in builds {
    let drv_path = build.drv_path.clone();
    let job_name = build.job_name.clone();
    let kind = build.kind;
    let id = repo::builds::create_in_transaction(&tx, build).await?.id;

    name_to_build.insert(job_name, id);
    build_ids.push(id);
    if kind.is_effect() {
      if git_token_drvs.contains(store_path(&drv_path).as_str()) {
        repo::effect_git_token_requests::insert_in_transaction(&tx, id).await?;
      }
      effect_build_ids.push(id);
      drv_to_build.entry(drv_path).or_insert(id);
    } else {
      regular_build_ids.push(id);
      drv_to_build.insert(drv_path, id);
    }
  }

  for (build_id, dep_build_id) in resolve_dependency_pairs(
    &jobs,
    &build_ids,
    &drv_to_build,
    &name_to_build,
    &effect_build_ids,
    &regular_build_ids,
  ) {
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
  use serde_json::json;

  use super::*;

  #[test]
  fn parses_effect_markers_from_derivation_json() {
    for drv in [
      json!({"env": {"isEffect": "1"}}),
      json!({"env": {"isEffect": "true"}}),
      json!({"structuredAttrs": {"isEffect": true}}),
      json!({"env": {"__json": r#"{"isEffect":true}"#}}),
    ] {
      assert!(derivation_is_effect(&drv), "{drv}");
    }
  }

  #[test]
  fn rejects_false_or_unrelated_effect_text() {
    for drv in [
      json!({"env": {"isEffect": "0"}}),
      json!({"structuredAttrs": {"isEffect": false}}),
      json!({"env": {"note": r#"("isEffect","1")"#}}),
      json!({"env": {"__json": r#"{"isEffect":false}"#}}),
    ] {
      assert!(!derivation_is_effect(&drv), "{drv}");
    }
  }

  #[test]
  fn parses_current_wrapped_derivation_output() {
    let requested = vec!["/nix/store/abc-effect.drv".to_owned()];
    let parsed = parse_derivation_output(
      br#"{
        "derivations": {
          "abc-effect.drv": {
            "structuredAttrs": {"isEffect": true},
            "inputs": {
              "drvs": {
                "def-input.drv": {"outputs": ["out"]}
              }
            },
            "outputs": {
              "out": {"path": "ghi-effect"}
            }
          }
        },
        "version": 4
      }"#,
      &requested,
    )
    .expect("valid wrapped derivation output");

    let effect = parsed
      .get("/nix/store/abc-effect.drv")
      .expect("wrapped derivation");
    assert!(effect.is_effect);
    assert!(
      effect
        .input_drvs
        .as_ref()
        .is_some_and(|inputs| inputs.contains_key("/nix/store/def-input.drv"))
    );
    assert_eq!(
      effect
        .outputs
        .as_ref()
        .and_then(|outputs| outputs.get("out"))
        .map(String::as_str),
      Some("/nix/store/ghi-effect")
    );
  }

  #[test]
  fn parses_legacy_direct_derivation_output() {
    let requested = vec!["/nix/store/abc-effect.drv".to_owned()];
    let parsed = parse_derivation_output(
      br#"{
        "/nix/store/abc-effect.drv": {
          "env": {"isEffect": "1"},
          "inputDrvs": {
            "/nix/store/def-input.drv": ["out"]
          },
          "outputs": {
            "out": {"outPath": "/nix/store/ghi-effect"}
          }
        }
      }"#,
      &requested,
    )
    .expect("valid direct derivation output");

    let effect = parsed
      .get("/nix/store/abc-effect.drv")
      .expect("direct derivation");
    assert!(effect.is_effect);
    assert!(
      effect
        .input_drvs
        .as_ref()
        .is_some_and(|inputs| inputs.contains_key("/nix/store/def-input.drv"))
    );
  }

  #[test]
  fn rejects_invalid_derivation_output() {
    let requested = vec!["/nix/store/abc.drv".to_owned()];
    let invalid_json = parse_derivation_output(b"{", &requested)
      .expect_err("invalid JSON must fail");
    assert!(
      invalid_json
        .to_string()
        .contains("invalid nix derivation show --recursive JSON")
    );

    let invalid_wrapper =
      parse_derivation_output(br#"{"derivations":[],"version":4}"#, &requested)
        .expect_err("invalid wrapper must fail");
    assert!(
      invalid_wrapper
        .to_string()
        .contains("non-object derivations field")
    );
  }

  #[test]
  fn rejects_output_missing_a_top_level_derivation() {
    let requested = vec![
      "/nix/store/present.drv".to_owned(),
      "/nix/store/missing-effect.drv".to_owned(),
    ];
    let error = parse_derivation_output(
      br#"{
        "derivations": {
          "present.drv": {"env": {}, "outputs": {}}
        },
        "version": 4
      }"#,
      &requested,
    )
    .expect_err("missing requested derivation must fail");

    assert!(error.to_string().contains("/nix/store/missing-effect.drv"));
  }

  fn job(
    name: &str,
    drv_path: &str,
    input_drvs: &[&str],
  ) -> crate::nix::NixJob {
    crate::nix::NixJob {
      name:         name.to_owned(),
      drv_path:     drv_path.to_owned(),
      system:       None,
      outputs:      None,
      input_drvs:   (!input_drvs.is_empty()).then(|| {
        input_drvs
          .iter()
          .map(|path| ((*path).to_owned(), json!(["out"])))
          .collect()
      }),
      constituents: None,
      meta:         crate::nix::NixMeta::default(),
    }
  }

  #[test]
  fn recursive_inspection_hydrates_shallow_top_level_jobs() {
    let effect_path = "/nix/store/effect.drv";
    let input_path = "/nix/store/effect-input.drv";
    let jobs = vec![job("effect", effect_path, &[])];
    let derivations =
      HashMap::from([(effect_path.to_owned(), DerivationInfo {
        system:            Some("x86_64-linux".to_owned()),
        outputs:           Some(HashMap::from([(
          "out".to_owned(),
          "/nix/store/effect-output".to_owned(),
        )])),
        input_drvs:        Some(HashMap::from([(
          input_path.to_owned(),
          json!(["out"]),
        )])),
        required_features: Vec::new(),
        is_effect:         true,
        wants_git_token:   false,
      })]);

    let hydrated = hydrate_top_level_derivations(&jobs, &derivations);

    assert_eq!(hydrated[0].system.as_deref(), Some("x86_64-linux"));
    assert!(
      hydrated[0]
        .input_drvs
        .as_ref()
        .is_some_and(|inputs| inputs.contains_key(input_path))
    );
    assert_eq!(
      hydrated[0]
        .outputs
        .as_ref()
        .and_then(|outputs| outputs.get("out"))
        .map(String::as_str),
      Some("/nix/store/effect-output")
    );
  }

  #[test]
  fn dependency_jobs_only_track_the_outputs_the_parent_selected() {
    let info = DerivationInfo {
      system:            Some("x86_64-linux".to_owned()),
      outputs:           Some(HashMap::from([
        ("out".to_owned(), "/nix/store/package".to_owned()),
        ("dev".to_owned(), "/nix/store/package-dev".to_owned()),
        ("debug".to_owned(), "/nix/store/package-debug".to_owned()),
      ])),
      input_drvs:        None,
      required_features: Vec::new(),
      is_effect:         false,
      wants_git_token:   false,
    };

    let current =
      selected_derivation_outputs(&info, &json!({"outputs": ["out"]}))
        .expect("selected outputs");
    let legacy = selected_derivation_outputs(&info, &json!(["dev"]))
      .expect("selected outputs");

    assert_eq!(
      current,
      HashMap::from([("out".to_owned(), "/nix/store/package".to_owned())])
    );
    assert_eq!(
      legacy,
      HashMap::from([("dev".to_owned(), "/nix/store/package-dev".to_owned())])
    );
  }

  #[test]
  fn effects_depend_on_every_regular_build_but_not_each_other() {
    let regular = Uuid::from_u128(1);
    let dependency = Uuid::from_u128(2);
    let effect_a = Uuid::from_u128(3);
    let effect_b = Uuid::from_u128(4);
    let aggregate = Uuid::from_u128(5);
    let mut aggregate_job = job("aggregate", "/nix/store/aggregate.drv", &[]);
    aggregate_job.constituents =
      Some(vec!["regular".to_owned(), "effect-a".to_owned()]);
    let jobs = vec![
      job("regular", "/nix/store/regular.drv", &[
        "/nix/store/dependency.drv"
      ]),
      job("drv:dependency", "/nix/store/dependency.drv", &[]),
      job("effect-a", "/nix/store/effect-a.drv", &[]),
      job("effect-b", "/nix/store/effect-b.drv", &[]),
      aggregate_job,
    ];
    let drv_to_build = HashMap::from([
      ("/nix/store/regular.drv".to_owned(), regular),
      ("/nix/store/dependency.drv".to_owned(), dependency),
      ("/nix/store/effect-a.drv".to_owned(), effect_a),
      ("/nix/store/effect-b.drv".to_owned(), effect_b),
      ("/nix/store/aggregate.drv".to_owned(), aggregate),
    ]);
    let name_to_build = HashMap::from([
      ("regular".to_owned(), regular),
      ("drv:dependency".to_owned(), dependency),
      ("effect-a".to_owned(), effect_a),
      ("effect-b".to_owned(), effect_b),
      ("aggregate".to_owned(), aggregate),
    ]);

    let build_ids = [regular, dependency, effect_a, effect_b, aggregate];

    let edges = resolve_dependency_pairs(
      &jobs,
      &build_ids,
      &drv_to_build,
      &name_to_build,
      &[effect_a, effect_b],
      &[regular, dependency, aggregate],
    )
    .into_iter()
    .collect::<HashSet<_>>();

    assert!(edges.contains(&(regular, dependency)));
    assert!(edges.contains(&(aggregate, regular)));
    assert!(!edges.contains(&(aggregate, effect_a)));
    for effect in [effect_a, effect_b] {
      assert!(edges.contains(&(effect, regular)));
      assert!(edges.contains(&(effect, dependency)));
      assert!(edges.contains(&(effect, aggregate)));
      assert!(!edges.contains(&(effect, effect_a)));
      assert!(!edges.contains(&(effect, effect_b)));
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
      &[],
      &build_ids,
    );

    assert_eq!(pairs, vec![
      (build_ids[0], build_ids[2]),
      (build_ids[1], build_ids[2]),
    ]);
  }
}
