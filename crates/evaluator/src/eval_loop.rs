use std::{collections::HashMap, sync::Arc, time::Duration};

use anyhow::Context;
use chrono::Utc;
use circus_common::{
  config::EvaluatorConfig,
  error::{CiError, check_disk_space},
  models::{
    CreateBuild,
    CreateEvaluation,
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

  // Filter to jobsets that are due for evaluation based on their
  // check_interval and last_checked_at
  let now = Utc::now();
  let ready: Vec<_> = active
    .into_iter()
    .filter(|js| {
      js.last_checked_at.is_none_or(|last| {
        let elapsed = (now - last).num_seconds();
        elapsed >= i64::from(js.check_interval)
      })
    })
    .collect();

  tracing::info!("Found {} jobsets due for evaluation", ready.len());

  let max_concurrent = config.max_concurrent_evals;

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

          let msg = e.to_string().to_lowercase();
          if msg.contains("no space left on device")
            || msg.contains("disk full")
            || msg.contains("enospc")
            || msg.contains("cannot create")
            || msg.contains("sqlite")
          {
            tracing::error!(
              "Evaluation failed due to disk space problems. Please free up \
               space on the server:\n- Run `nix-collect-garbage -d` to clean \
               the Nix store\n- Clear /tmp/circus-evaluator directory\n- \
               Check build logs directory if configured"
            );
          }
        }
      }
    })
    .await;

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

  match check_disk_space(&work_dir) {
    Ok(info) => {
      if info.is_critical() {
        tracing::error!(
          jobset = %jobset.name,
          "Less than 1GB disk space available. {}",
          info.summary()
        );
      } else if info.is_low() {
        tracing::warn!(
          jobset = %jobset.name,
          "Less than 5GB disk space available. {}",
          info.summary()
        );
      }
    },
    Err(e) => {
      tracing::warn!(
        jobset = %jobset.name,
        "Disk space check failed: {}. Proceeding anyway...",
        e
      );
    },
  }

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

  // Check if this exact combination was already evaluated (eval caching)
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

  // Also skip if commit hasn't changed and inputs_hash matches (backward
  // compat for evaluations created before inputs_hash was indexed)
  if let Some(latest) = repo::evaluations::get_latest(pool, jobset.id).await?
    && latest.commit_hash == commit_hash
    && latest.inputs_hash.as_deref() == Some(&inputs_hash)
  {
    tracing::debug!(
        jobset = %jobset.name,
        commit = %commit_hash,
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

  // Check for declarative config in repo
  check_declarative_config(pool, &repo_path, jobset.project_id).await;

  // Run nix evaluation
  match crate::nix::evaluate(
    &repo_path,
    &jobset.nix_expression,
    jobset.flake_mode,
    nix_timeout,
    config,
    &inputs,
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

      // Dispatch pending notifications for created builds
      if notifications_config.enable_retry_queue {
        if let Ok(project) = repo::projects::get(pool, jobset.project_id).await
        {
          if let Ok(builds) =
            repo::builds::list_for_evaluation(pool, eval.id).await
          {
            for build in builds {
              // Skip aggregate builds (they complete later when constituents
              // finish)
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

    let build = repo::builds::create(pool, CreateBuild {
      evaluation_id: eval_id,
      job_name:      job.name.clone(),
      drv_path:      job.drv_path.clone(),
      system:        job.system.clone(),
      outputs:       outputs_json,
      is_aggregate:  Some(is_aggregate),
      constituents:  constituents_json,
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

/// Check for declarative project config (.circus.toml or .circus/config.toml)
/// in the repo.
async fn check_declarative_config(
  pool: &PgPool,
  repo_path: &std::path::Path,
  project_id: Uuid,
) {
  #[derive(serde::Deserialize)]
  struct DeclarativeConfig {
    jobsets: Option<Vec<DeclarativeJobset>>,
  }

  #[derive(serde::Deserialize)]
  struct DeclarativeJobset {
    name:           String,
    nix_expression: String,
    flake_mode:     Option<bool>,
    check_interval: Option<i32>,
    enabled:        Option<bool>,
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
      tracing::warn!(
        "Failed to read declarative config {}: {e}",
        path.display()
      );
      return;
    },
  };

  let config: DeclarativeConfig = match toml::from_str(&content) {
    Ok(c) => c,
    Err(e) => {
      tracing::warn!("Failed to parse declarative config: {e}");
      return;
    },
  };

  if let Some(jobsets) = config.jobsets {
    for js in jobsets {
      let input = circus_common::models::CreateJobset {
        project_id,
        name: js.name,
        nix_expression: js.nix_expression,
        enabled: js.enabled,
        flake_mode: js.flake_mode,
        check_interval: js.check_interval,
        branch: None,
        scheduling_shares: None,
        state: None,
        keep_nr: None,
      };
      if let Err(e) = repo::jobsets::upsert(pool, input).await {
        tracing::warn!("Failed to upsert declarative jobset: {e}");
      }
    }
  }
}
