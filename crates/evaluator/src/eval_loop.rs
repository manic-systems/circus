use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Context;
use chrono::Utc;
use circus_common::{
  config::EvaluatorConfig,
  error::{CiError, check_disk_space},
  models::{
    ActiveJobset,
    CreateBuild,
    CreateEvaluation,
    CreateJobset,
    Evaluation,
    EvaluationStatus,
    JobsetInput,
    JobsetState,
  },
  repo,
};
use futures::stream::{self, StreamExt};
use sqlx::PgPool;
use tokio::sync::Notify;
use tracing::info;
use uuid::Uuid;

/// Main evaluator loop. Polls jobsets and runs nix evaluations.
///
/// # Errors
///
/// Returns error if evaluation cycle fails and `strict_errors` is enabled.
pub async fn run(
  pool: PgPool,
  config: EvaluatorConfig,
  notifications_config: circus_common::config::NotificationsConfig,
  wakeup: Arc<Notify>,
) -> anyhow::Result<()> {
  let poll_interval = Duration::from_secs(config.poll_interval);
  let nix_timeout = Duration::from_secs(config.nix_timeout);
  let git_timeout = Duration::from_secs(config.git_timeout);

  let strict = config.strict_errors;

  loop {
    if let Err(e) = run_cycle(
      &pool,
      &config,
      &notifications_config,
      nix_timeout,
      git_timeout,
    )
    .await
    {
      if strict {
        return Err(e);
      }
      tracing::error!("Evaluation cycle failed: {e}");
    }
    // Wake on NOTIFY or fall back to regular poll interval
    let _ = tokio::time::timeout(poll_interval, wakeup.notified()).await;
  }
}

async fn run_cycle(
  pool: &PgPool,
  config: &EvaluatorConfig,
  notifications_config: &circus_common::config::NotificationsConfig,
  nix_timeout: Duration,
  git_timeout: Duration,
) -> anyhow::Result<()> {
  let active = repo::jobsets::list_active(pool).await?;
  let active_by_id: HashMap<Uuid, ActiveJobset> =
    active.iter().cloned().map(|j| (j.id, j)).collect();

  let max_concurrent = config.max_concurrent_evals;

  // First drain push-driven work. Webhooks and /evaluations/trigger
  // insert pending eval rows with a specific commit_hash (push commit,
  // PR head). Each one is evaluated at exactly that commit, independent
  // of jobset check_interval and independent of the branch tip.
  let pending =
    repo::evaluations::list_pending(pool)
      .await
      .unwrap_or_else(|e| {
        tracing::warn!("Failed to list pending evaluations: {e}");
        Vec::new()
      });

  let pending_jobset_ids: std::collections::HashSet<Uuid> =
    pending.iter().map(|e| e.jobset_id).collect();

  let pending_tasks: Vec<(Evaluation, ActiveJobset)> = pending
    .into_iter()
    .filter_map(|eval| {
      active_by_id
        .get(&eval.jobset_id)
        .map(|js| (eval, js.clone()))
    })
    .collect();

  if !pending_tasks.is_empty() {
    tracing::info!("Draining {} pending evaluation(s)", pending_tasks.len());
  }

  stream::iter(pending_tasks)
    .for_each_concurrent(max_concurrent, |(eval, jobset)| {
      async move {
        if let Err(e) = evaluate_pending_eval(
          pool,
          &eval,
          &jobset,
          config,
          notifications_config,
          nix_timeout,
          git_timeout,
        )
        .await
        {
          tracing::error!(
              jobset_id = %jobset.id,
              jobset_name = %jobset.name,
              eval_id = %eval.id,
              commit = %eval.commit_hash,
              "Failed to process pending evaluation: {e}"
          );

          let msg = e.to_string();
          if let Err(mark_err) = repo::evaluations::update_status(
            pool,
            eval.id,
            EvaluationStatus::Failed,
            Some(&msg),
          )
          .await
          {
            tracing::warn!(
              eval_id = %eval.id,
              "Failed to record evaluation failure status: {mark_err}"
            );
          }

          warn_on_disk_pressure(&msg);
        }
      }
    })
    .await;

  // Then, git polling for jobsets due by check_interval, excluding any
  // we just handled via the push path (their work is already current).
  let now = Utc::now();
  let ready: Vec<_> = active
    .into_iter()
    .filter(|js| {
      if pending_jobset_ids.contains(&js.id) {
        return false;
      }
      js.last_checked_at.is_none_or(|last| {
        let elapsed = (now - last).num_seconds();
        elapsed >= i64::from(js.check_interval)
      })
    })
    .collect();

  tracing::info!("Found {} jobsets due for evaluation", ready.len());

  stream::iter(ready)
    .for_each_concurrent(max_concurrent, |jobset| {
      async move {
        if let Err(e) = evaluate_jobset(
          pool,
          &jobset,
          config,
          notifications_config,
          nix_timeout,
          git_timeout,
        )
        .await
        {
          tracing::error!(
              jobset_id = %jobset.id,
              jobset_name = %jobset.name,
              "Failed to evaluate jobset: {e}"
          );
          warn_on_disk_pressure(&e.to_string());
        }
      }
    })
    .await;

  // Clone projects with no active jobsets to discover their in-repo config.
  // This handles the case where a project is declared in the server config
  // without any jobsets and relies solely on .circus.toml to define them.
  discover_projects_without_jobsets(pool, config, git_timeout).await;

  Ok(())
}

fn warn_on_disk_pressure(msg: &str) {
  let lower = msg.to_lowercase();
  if lower.contains("no space left on device")
    || lower.contains("disk full")
    || lower.contains("enospc")
    || lower.contains("cannot create")
    || lower.contains("sqlite")
  {
    tracing::error!(
      "Evaluation failed due to disk space problems. Please free up space on \
       the server:\n- Run `nix-collect-garbage -d` to clean the Nix store\n- \
       Clear /tmp/circus-evaluator directory\n- Check build logs directory if \
       configured"
    );
  }
}

/// Process a single push-driven pending evaluation by checking out its
/// declared `commit_hash` (which may be a PR head not on any branch tip)
/// and running nix evaluation against it. Idempotent: skips silently if
/// another worker already claimed the row.
///
/// `last_checked_at` is intentionally not touched here. The push path
/// evaluates a specific commit; the branch-tip poll runs against
/// whatever the configured branch points at now. If the push commit was
/// the branch tip the next branch poll is absorbed by the eval-cache
/// check on the inputs hash; if it wasn't, the branch poll still needs
/// to fire on its own cadence.
async fn evaluate_pending_eval(
  pool: &PgPool,
  eval: &Evaluation,
  jobset: &ActiveJobset,
  config: &EvaluatorConfig,
  notifications_config: &circus_common::config::NotificationsConfig,
  nix_timeout: Duration,
  git_timeout: Duration,
) -> anyhow::Result<()> {
  // Claim the row atomically. If the row is no longer pending another
  // wakeup already handled it; bail out without doing the git fetch.
  let Some(claimed) =
    repo::evaluations::try_claim_pending(pool, eval.id).await?
  else {
    tracing::debug!(
      eval_id = %eval.id,
      "Pending evaluation already claimed, skipping"
    );
    return Ok(());
  };

  tracing::info!(
      jobset = %jobset.name,
      eval_id = %claimed.id,
      commit = %claimed.commit_hash,
      pr_number = ?claimed.pr_number,
      "Processing pending evaluation"
  );

  log_disk_space(&config.work_dir, &jobset.name);

  let url = jobset.repository_url.clone();
  let work_dir = config.work_dir.clone();
  let project_name = jobset.project_name.clone();
  let target_commit = claimed.commit_hash.clone();

  let repo_path = tokio::time::timeout(
    git_timeout,
    tokio::task::spawn_blocking(move || {
      crate::git::fetch_and_checkout_commit(
        &url,
        &work_dir,
        &project_name,
        &target_commit,
      )
    }),
  )
  .await
  .map_err(|_| {
    anyhow::anyhow!("Git operation timed out after {git_timeout:?}")
  })???;

  let inputs = repo::jobset_inputs::list_for_jobset(pool, jobset.id)
    .await
    .unwrap_or_default();
  let inputs_hash = compute_inputs_hash(&claimed.commit_hash, &inputs);

  if let Err(e) =
    repo::evaluations::set_inputs_hash(pool, claimed.id, &inputs_hash).await
  {
    tracing::warn!(eval_id = %claimed.id, "Failed to set inputs hash: {e}");
  }

  sync_repo_declarative_config(pool, &repo_path, jobset.project_id).await;

  run_nix_and_record_builds(
    pool,
    jobset,
    &claimed,
    &repo_path,
    &inputs,
    config,
    notifications_config,
    nix_timeout,
  )
  .await?;

  if jobset.state == JobsetState::OneShot {
    tracing::info!(
      jobset = %jobset.name,
      "One-shot evaluation complete, disabling jobset"
    );
    if let Err(e) = repo::jobsets::mark_one_shot_complete(pool, jobset.id).await
    {
      tracing::error!(
        jobset = %jobset.name,
        "Failed to mark one-shot complete: {e}"
      );
    }
  }

  Ok(())
}

fn log_disk_space(work_dir: &std::path::Path, jobset_name: &str) {
  match check_disk_space(work_dir) {
    Ok(info) => {
      if info.is_critical() {
        tracing::error!(
          jobset = jobset_name,
          "Less than 1GB disk space available. {}",
          info.summary()
        );
      } else if info.is_low() {
        tracing::warn!(
          jobset = jobset_name,
          "Less than 5GB disk space available. {}",
          info.summary()
        );
      }
    },
    Err(e) => {
      tracing::warn!(
        jobset = jobset_name,
        "Disk space check failed: {}. Proceeding anyway...",
        e
      );
    },
  }
}

/// Shared back-half: invoke nix, persist builds, dispatch notifications,
/// and mark the eval row Completed or Failed. Used by both the branch-tip
/// path (`evaluate_jobset`) and the push-driven path
/// (`evaluate_pending_eval`).
#[allow(clippy::too_many_arguments)]
async fn run_nix_and_record_builds(
  pool: &PgPool,
  jobset: &ActiveJobset,
  eval: &Evaluation,
  repo_path: &std::path::Path,
  inputs: &[JobsetInput],
  config: &EvaluatorConfig,
  notifications_config: &circus_common::config::NotificationsConfig,
  nix_timeout: Duration,
) -> anyhow::Result<()> {
  match crate::nix::evaluate(
    repo_path,
    &jobset.nix_expression,
    jobset.flake_mode,
    nix_timeout,
    config,
    inputs,
  )
  .await
  {
    Ok(eval_result) => {
      tracing::debug!(jobset = %jobset.name, job_count = eval_result.jobs.len(), "Nix evaluation returned");
      tracing::info!(
          jobset = %jobset.name,
          count = eval_result.jobs.len(),
          errors = eval_result.error_count,
          "Evaluation discovered jobs"
      );

      create_builds_from_eval(pool, eval.id, &eval_result).await?;

      if notifications_config.enable_retry_queue {
        if let Ok(project) = repo::projects::get(pool, jobset.project_id).await
        {
          if let Ok(builds) =
            repo::builds::list_for_evaluation(pool, eval.id).await
          {
            for build in builds {
              if !build.is_aggregate {
                circus_common::notifications::dispatch_build_created(
                  pool,
                  &build,
                  &project,
                  &eval.commit_hash,
                  notifications_config,
                )
                .await;
              }
            }
          } else {
            tracing::warn!(
              eval_id = %eval.id,
              "Failed to fetch builds for pending notifications"
            );
          }
        } else {
          tracing::warn!(
            project_id = %jobset.project_id,
            "Failed to fetch project for pending notifications"
          );
        }
      }

      repo::evaluations::update_status(
        pool,
        eval.id,
        EvaluationStatus::Completed,
        None,
      )
      .await?;
    },
    Err(e) => {
      let msg = e.to_string();
      tracing::error!(jobset = %jobset.name, "Evaluation failed: {msg}");
      repo::evaluations::update_status(
        pool,
        eval.id,
        EvaluationStatus::Failed,
        Some(&msg),
      )
      .await?;
    },
  }

  Ok(())
}

async fn evaluate_jobset(
  pool: &PgPool,
  jobset: &circus_common::models::ActiveJobset,
  config: &EvaluatorConfig,
  notifications_config: &circus_common::config::NotificationsConfig,
  nix_timeout: Duration,
  git_timeout: Duration,
) -> anyhow::Result<()> {
  let url = jobset.repository_url.clone();
  let work_dir = config.work_dir.clone();
  let project_name = jobset.project_name.clone();
  let branch = jobset.branch.clone();

  tracing::info!(
      jobset = %jobset.name,
      project = %project_name,
      "Starting evaluation cycle"
  );

  log_disk_space(&work_dir, &jobset.name);

  // Clone/fetch in a blocking task (git2 is sync) with timeout
  let (repo_path, commit_hash) = tokio::time::timeout(
    git_timeout,
    tokio::task::spawn_blocking(move || {
      crate::git::clone_or_fetch(
        &url,
        &work_dir,
        &project_name,
        branch.as_deref(),
      )
    }),
  )
  .await
  .map_err(|_| {
    anyhow::anyhow!("Git operation timed out after {git_timeout:?}")
  })???;

  // Query jobset inputs
  let inputs = repo::jobset_inputs::list_for_jobset(pool, jobset.id)
    .await
    .unwrap_or_default();

  // Compute inputs hash for eval caching (commit + all input values/revisions)
  let inputs_hash = compute_inputs_hash(&commit_hash, &inputs);

  // Skip re-evaluation when the same (commit, inputs) hash already
  // produced a completed evaluation.
  if let Ok(Some(cached)) =
    repo::evaluations::get_by_inputs_hash(pool, jobset.id, &inputs_hash).await
  {
    tracing::debug!(
        jobset = %jobset.name,
        commit = %commit_hash,
        cached_eval = %cached.id,
        "Inputs unchanged (hash: {}), skipping evaluation",
        &inputs_hash[..16],
    );
    repo::jobsets::update_last_checked(pool, jobset.id).await?;
    return Ok(());
  }

  tracing::info!(
      jobset = %jobset.name,
      commit = %commit_hash,
      "Starting evaluation"
  );

  // Create evaluation record. If it already exists (race condition), fetch the
  // existing one and continue. Only update status if it's still pending.
  let eval = match repo::evaluations::create(pool, CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    commit_hash.clone(),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  })
  .await
  {
    Ok(eval) => eval,
    Err(CiError::Conflict(_)) => {
      tracing::info!(
          jobset = %jobset.name,
          commit = %commit_hash,
          "Evaluation already exists (conflict), fetching existing record"
      );
      let existing = repo::evaluations::get_by_jobset_and_commit(
        pool,
        jobset.id,
        &commit_hash,
      )
      .await?
      .ok_or_else(|| {
        anyhow::anyhow!(
          "Evaluation conflict but not found: {}/{}",
          jobset.id,
          commit_hash
        )
      })?;

      if existing.status == EvaluationStatus::Pending {
        repo::evaluations::update_status(
          pool,
          existing.id,
          EvaluationStatus::Running,
          None,
        )
        .await?;
      } else if existing.status == EvaluationStatus::Completed {
        let build_count = repo::builds::count_filtered(
          pool,
          Some(existing.id),
          None,
          None,
          None,
        )
        .await?;

        if build_count > 0 {
          info!(
            "Evaluation already completed with {} builds, skipping nix \
             evaluation jobset={} commit={}",
            build_count, jobset.name, commit_hash
          );
          if let Err(e) =
            repo::jobsets::update_last_checked(pool, jobset.id).await
          {
            tracing::warn!(
              jobset = %jobset.name,
              "Failed to update last_checked_at: {e}"
            );
          }
          return Ok(());
        }
        info!(
          "Evaluation completed but has 0 builds, re-running nix evaluation \
           jobset={} commit={}",
          jobset.name, commit_hash
        );
      }
      existing
    },
    Err(e) => {
      return Err(anyhow::anyhow!(e)).with_context(|| {
        format!("failed to create evaluation for jobset {}", jobset.name)
      });
    },
  };

  // Set inputs hash (only needed for new evaluations, not existing ones)
  if let Err(e) =
    repo::evaluations::set_inputs_hash(pool, eval.id, &inputs_hash).await
  {
    tracing::warn!(eval_id = %eval.id, "Failed to set evaluation inputs hash: {e}");
  }

  // Sync any jobsets declared in the repo's .circus.toml
  sync_repo_declarative_config(pool, &repo_path, jobset.project_id).await;

  run_nix_and_record_builds(
    pool,
    jobset,
    &eval,
    &repo_path,
    &inputs,
    config,
    notifications_config,
    nix_timeout,
  )
  .await?;

  // Update last_checked_at timestamp for per-jobset interval tracking
  if let Err(e) = repo::jobsets::update_last_checked(pool, jobset.id).await {
    tracing::warn!(
      jobset = %jobset.name,
      "Failed to update last_checked_at: {e}"
    );
  }

  // Mark one-shot jobsets as complete (disabled) after evaluation
  if jobset.state == JobsetState::OneShot {
    tracing::info!(
      jobset = %jobset.name,
      "One-shot evaluation complete, disabling jobset"
    );
    if let Err(e) = repo::jobsets::mark_one_shot_complete(pool, jobset.id).await
    {
      tracing::error!(
        jobset = %jobset.name,
        "Failed to mark one-shot complete: {e}"
      );
    }
  }

  Ok(())
}

/// Detect whether a derivation is a fixed-output derivation by reading the
/// `.drv` file and checking for `outputHash` in its env vars.
/// Returns `(is_fod, fod_hash)`.
/// Read the derivation's `requiredSystemFeatures` env var via
/// `nix derivation show`. The Nix derivation format encodes this attribute
/// as a colon- or space-separated string in `env.requiredSystemFeatures`
/// (a function of how nixpkgs builds the drv); modern Nix exposes a
/// structured `requiredSystemFeatures` key on the derivation too.
///
/// We accept either: parse a JSON list if present, otherwise split the
/// env string on whitespace. Failure (drv not on disk, nix not on PATH,
/// malformed JSON) returns an empty list, mirroring the SSH path which
/// treats absence as "no constraint".
async fn read_required_features(drv_path: &str) -> Vec<String> {
  let Ok(output) = tokio::process::Command::new("nix")
    .args([
      "--extra-experimental-features",
      "nix-command",
      "derivation",
      "show",
      drv_path,
    ])
    .output()
    .await
  else {
    return Vec::new();
  };
  if !output.status.success() {
    return Vec::new();
  }
  let Ok(parsed) = serde_json::from_slice::<serde_json::Value>(&output.stdout)
  else {
    return Vec::new();
  };
  let Some(drv) = parsed.as_object().and_then(|o| o.values().next()) else {
    return Vec::new();
  };
  let drv = drv.as_object();

  // 1. Top-level `requiredSystemFeatures` list (modern Nix).
  if let Some(arr) = drv
    .and_then(|d| d.get("requiredSystemFeatures"))
    .and_then(|v| v.as_array())
  {
    return arr
      .iter()
      .filter_map(|v| v.as_str().map(str::to_owned))
      .filter(|s| !s.is_empty())
      .collect();
  }
  // 2. `env.requiredSystemFeatures`: space-separated string.
  if let Some(env_str) = drv
    .and_then(|d| d.get("env"))
    .and_then(|v| v.as_object())
    .and_then(|env| env.get("requiredSystemFeatures"))
    .and_then(|v| v.as_str())
  {
    return env_str.split_whitespace().map(str::to_owned).collect();
  }
  Vec::new()
}

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
async fn create_builds_from_eval(
  pool: &PgPool,
  eval_id: Uuid,
  eval_result: &crate::nix::EvalResult,
) -> anyhow::Result<()> {
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
fn compute_inputs_hash(commit_hash: &str, inputs: &[JobsetInput]) -> String {
  use sha2::{Digest, Sha256};

  let mut hasher = Sha256::new();
  hasher.update(commit_hash.as_bytes());

  // Sort inputs by name for deterministic hashing
  let mut sorted_inputs: Vec<&JobsetInput> = inputs.iter().collect();
  sorted_inputs.sort_by_key(|i| &i.name);

  for input in sorted_inputs {
    hasher.update(input.name.as_bytes());
    hasher.update(input.input_type.as_bytes());
    hasher.update(input.value.as_bytes());
    if let Some(ref rev) = input.revision {
      hasher.update(rev.as_bytes());
    }
  }

  hex::encode(hasher.finalize())
}

/// Sync jobsets declared in a repo's `.circus.toml` (or `.circus/config.toml`)
/// into the database for the given project.
///
/// This is called both after cloning during a normal evaluation and during the
/// project-discovery pass for projects that have no active jobsets yet.
async fn sync_repo_declarative_config(
  pool: &PgPool,
  repo_path: &std::path::Path,
  project_id: Uuid,
) {
  #[derive(serde::Deserialize)]
  struct RepoConfig {
    #[serde(default)]
    jobsets: Vec<circus_common::config::DeclarativeJobset>,
  }

  let config_path = repo_path.join(".circus.toml");
  let alt_config_path = repo_path.join(".circus/config.toml");

  let path = if config_path.exists() {
    config_path
  } else if alt_config_path.exists() {
    alt_config_path
  } else {
    return;
  };

  let content = match std::fs::read_to_string(&path) {
    Ok(c) => c,
    Err(e) => {
      tracing::warn!("Failed to read repo config {}: {e}", path.display());
      return;
    },
  };

  let config: RepoConfig = match toml::from_str(&content) {
    Ok(c) => c,
    Err(e) => {
      tracing::warn!("Failed to parse repo config {}: {e}", path.display());
      return;
    },
  };

  for js in &config.jobsets {
    let state = js.state.as_deref().map(JobsetState::from_config_str);

    let input = CreateJobset {
      project_id,
      name: js.name.clone(),
      nix_expression: js.nix_expression.clone(),
      enabled: Some(js.enabled),
      flake_mode: Some(js.flake_mode),
      check_interval: Some(js.check_interval),
      branch: js.branch.clone(),
      scheduling_shares: Some(js.scheduling_shares),
      state,
      keep_nr: js.keep_nr,
    };

    match repo::jobsets::upsert(pool, input).await {
      Ok(jobset) => {
        if !js.inputs.is_empty()
          && let Err(e) =
            repo::jobset_inputs::sync_for_jobset(pool, jobset.id, &js.inputs)
              .await
        {
          tracing::warn!(
            jobset = %jobset.name,
            "Failed to sync inputs from repo config: {e}"
          );
        }
        tracing::debug!(
          jobset = %js.name,
          "Synced jobset from repo config"
        );
      },
      Err(e) => {
        tracing::warn!(
          jobset = %js.name,
          "Failed to upsert jobset from repo config: {e}"
        );
      },
    }
  }
}

/// Clone each project that has no active jobsets and look for a `.circus.toml`.
///
/// This handles the bootstrap case where a project is declared in the server
/// config without any jobsets, relying entirely on the in-repo config to define
/// them. Without this pass the repo would never be cloned and the in-repo
/// config would never be discovered.
async fn discover_projects_without_jobsets(
  pool: &PgPool,
  config: &EvaluatorConfig,
  git_timeout: Duration,
) {
  let projects = match repo::projects::list_without_active_jobsets(pool).await {
    Ok(p) => p,
    Err(e) => {
      tracing::warn!("Failed to list projects without active jobsets: {e}");
      return;
    },
  };

  for project in projects {
    let url = project.repository_url.clone();
    let work_dir = config.work_dir.clone();
    let project_name = project.name.clone();

    let clone_result = tokio::time::timeout(
      git_timeout,
      tokio::task::spawn_blocking(move || {
        crate::git::clone_or_fetch(&url, &work_dir, &project_name, None)
      }),
    )
    .await;

    let repo_path = match clone_result {
      Ok(Ok(Ok((path, _commit)))) => path,
      Ok(Ok(Err(e))) => {
        tracing::warn!(
          project = %project.name,
          "Failed to clone for discovery: {e}"
        );
        continue;
      },
      Ok(Err(e)) => {
        tracing::warn!(
          project = %project.name,
          "Spawn error during discovery clone: {e}"
        );
        continue;
      },
      Err(_) => {
        tracing::warn!(
          project = %project.name,
          "Git clone timed out during discovery"
        );
        continue;
      },
    };

    sync_repo_declarative_config(pool, &repo_path, project.id).await;
  }
}
