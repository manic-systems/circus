use std::{collections::HashMap, sync::Arc, time::Duration};

use chrono::Utc;
use circus_common::{
  PgPool,
  error::{CiError, check_disk_space},
  models::{
    ActiveJobset,
    CreateEvaluation,
    CreateJobset,
    Evaluation,
    EvaluationStatus,
    EvaluationTriggerKind,
    JobsetInput,
    JobsetState,
    JobsetTriggerMode,
  },
  repo,
};
use circus_config::{DeclarativeJobset, EvaluatorConfig, NotificationsConfig};
use color_eyre::eyre::Context;
use futures::stream::{self, StreamExt};
use tokio::sync::Notify;
use tokio_util::sync::CancellationToken;
use tracing::info;
use uuid::Uuid;

use crate::{
  builds::{compute_inputs_hash, create_builds_from_eval},
  evaluation_state::{ExistingEvaluationClaim, claim_existing},
};

/// Main evaluator loop. Polls jobsets and runs nix evaluations.
///
/// # Errors
///
/// Returns error if evaluation cycle fails and `strict_errors` is enabled.
pub async fn run(
  pool: PgPool,
  config: EvaluatorConfig,
  notifications_config: NotificationsConfig,
  notification_secret_key: Option<String>,
  wakeup: Arc<Notify>,
) -> color_eyre::Result<()> {
  let poll_interval = Duration::from_secs(config.poll_interval);
  let nix_timeout = Duration::from_secs(config.nix_timeout);
  let git_timeout = Duration::from_secs(config.git_timeout);

  let strict = config.strict_errors;

  loop {
    if let Err(e) = run_cycle(
      &pool,
      &config,
      &notifications_config,
      notification_secret_key.as_deref(),
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

/// Grace period past the worst-case evaluation time before a running row is
/// considered orphaned.
const ORPHAN_SWEEP_SLACK_SECS: f64 = 300.0;

/// Requeue evaluations stranded in running state by an evaluator that died
/// after claiming them. No live evaluation can outlast the git plus nix
/// timeouts, so anything older is unowned. Also unwedges the duplicate-poll
/// skip that a stranded row otherwise holds forever.
async fn sweep_orphaned_evaluations(pool: &PgPool, worst_case: Duration) {
  let deadline_secs = worst_case.as_secs_f64() + ORPHAN_SWEEP_SLACK_SECS;
  match repo::evaluations::sweep_orphaned(pool, deadline_secs).await {
    Ok(swept) => {
      for eval in swept {
        tracing::warn!(
          eval_id = %eval.id,
          jobset_id = %eval.jobset_id,
          commit = %eval.commit_hash,
          status = eval.status.as_db_str(),
          "Recovered orphaned evaluation"
        );
      }
    },
    Err(e) => {
      tracing::warn!("Failed to sweep orphaned evaluations: {e}");
    },
  }
}

async fn run_cycle(
  pool: &PgPool,
  config: &EvaluatorConfig,
  notifications_config: &NotificationsConfig,
  notification_secret_key: Option<&str>,
  nix_timeout: Duration,
  git_timeout: Duration,
) -> color_eyre::Result<()> {
  sweep_orphaned_evaluations(pool, git_timeout + nix_timeout).await;

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

  let mut pending_jobset_ids: std::collections::HashSet<Uuid> =
    std::collections::HashSet::new();
  let mut pending_tasks: Vec<(Evaluation, ActiveJobset)> = Vec::new();

  for eval in pending {
    let Some(jobset) = active_by_id.get(&eval.jobset_id) else {
      continue;
    };

    if accepts_pending_evaluation(jobset.trigger_mode, eval.trigger_kind) {
      pending_jobset_ids.insert(eval.jobset_id);
      pending_tasks.push((eval, jobset.clone()));
    } else {
      let msg = "jobset trigger_mode is interval; source/manual pending \
                 evaluations are disabled";
      if let Err(e) = repo::evaluations::update_status(
        pool,
        eval.id,
        EvaluationStatus::Failed,
        Some(msg),
      )
      .await
      {
        tracing::warn!(eval_id = %eval.id, "Failed to reject pending evaluation: {e}");
      }
    }
  }

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
          notification_secret_key,
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
          notification_secret_key,
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

fn accepts_pending_evaluation(
  trigger_mode: JobsetTriggerMode,
  trigger_kind: EvaluationTriggerKind,
) -> bool {
  trigger_mode.accepts_source_triggers()
    || trigger_kind == EvaluationTriggerKind::Interval
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
  notifications_config: &NotificationsConfig,
  notification_secret_key: Option<&str>,
  nix_timeout: Duration,
  git_timeout: Duration,
) -> color_eyre::Result<()> {
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
    color_eyre::eyre::eyre!("Git operation timed out after {git_timeout:?}")
  })???;

  if repo::evaluations::is_cancelled(pool, claimed.id).await? {
    tracing::info!(eval_id = %claimed.id, "Evaluation cancelled during checkout");
    return Ok(());
  }

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
    notification_secret_key,
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
#[expect(
  clippy::too_many_arguments,
  reason = "shared evaluation back-half needs both persisted eval state and \
            runtime configs"
)]
async fn run_nix_and_record_builds(
  pool: &PgPool,
  jobset: &ActiveJobset,
  eval: &Evaluation,
  repo_path: &std::path::Path,
  inputs: &[JobsetInput],
  config: &EvaluatorConfig,
  notifications_config: &NotificationsConfig,
  notification_secret_key: Option<&str>,
  nix_timeout: Duration,
) -> color_eyre::Result<()> {
  let max_eval_time = config.max_eval_time.map(Duration::from_secs);
  let nix_timeout = max_eval_time.map_or(nix_timeout, |limit| {
    let elapsed = (Utc::now() - eval.evaluation_time)
      .to_std()
      .unwrap_or_default();
    limit.saturating_sub(elapsed).min(nix_timeout)
  });
  if max_eval_time.is_some_and(|_| nix_timeout.is_zero()) {
    repo::evaluations::finish_running(
      pool,
      eval.id,
      EvaluationStatus::TimedOut,
      Some("Evaluation exceeded max_eval_time"),
    )
    .await?;
    return Ok(());
  }

  let cancel = CancellationToken::new();
  let watcher = tokio::spawn(watch_evaluation_cancellation(
    pool.clone(),
    eval.id,
    cancel.clone(),
  ));
  let result = crate::nix::evaluate(
    repo_path,
    &jobset.repository_url,
    &eval.commit_hash,
    &jobset.nix_expression,
    jobset.flake_mode,
    nix_timeout,
    config,
    inputs,
    &cancel,
    None,
  )
  .await;
  cancel.cancel();
  if let Err(error) = watcher.await {
    tracing::warn!(eval_id = %eval.id, "Evaluation cancellation watcher stopped: {error}");
  }

  match result {
    Ok(eval_result) => {
      tracing::debug!(jobset = %jobset.name, job_count = eval_result.jobs.len(), "Nix evaluation returned");
      tracing::info!(
          jobset = %jobset.name,
          count = eval_result.jobs.len(),
          errors = eval_result.error_count,
          "Evaluation discovered jobs"
      );

      let allowed_systems = circus_common::systems::restrict_to_jobset(
        circus_common::systems::resolve_allowed_systems(pool, config).await,
        jobset.systems.as_deref(),
      );
      if !create_builds_from_eval(
        pool,
        eval.id,
        &eval_result,
        crate::memory::MemoryLimit::from(config),
        allowed_systems.as_ref(),
      )
      .await?
      {
        tracing::info!(eval_id = %eval.id, "Evaluation was cancelled");
        return Ok(());
      }

      if notifications_config.enable_retry_queue {
        if let Ok(project) = repo::projects::get(pool, jobset.project_id).await
        {
          if let Ok(builds) =
            repo::builds::list_for_evaluation(pool, eval.id).await
          {
            for build in builds {
              if !build.is_aggregate {
                circus_notification::dispatch_build_created(
                  pool,
                  &build,
                  &project,
                  &eval.commit_hash,
                  notifications_config,
                  notification_secret_key,
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
    },
    Err(e) => {
      let msg = e.to_string();
      tracing::error!(jobset = %jobset.name, "Evaluation failed: {msg}");
      let status = if max_eval_time.is_some_and(|limit| {
        (Utc::now() - eval.evaluation_time)
          .to_std()
          .is_ok_and(|elapsed| elapsed >= limit)
      }) {
        EvaluationStatus::TimedOut
      } else {
        EvaluationStatus::Failed
      };
      repo::evaluations::finish_running(pool, eval.id, status, Some(&msg))
        .await?;
    },
  }

  Ok(())
}

async fn watch_evaluation_cancellation(
  pool: PgPool,
  evaluation_id: Uuid,
  cancel: CancellationToken,
) {
  let mut interval = tokio::time::interval(Duration::from_millis(500));
  loop {
    tokio::select! {
      () = cancel.cancelled() => return,
      _ = interval.tick() => match repo::evaluations::is_cancelled(&pool, evaluation_id).await {
        Ok(true) => {
          cancel.cancel();
          return;
        },
        Ok(false) => {},
        Err(error) => tracing::warn!(%evaluation_id, "Failed to check evaluation cancellation: {error}"),
      },
    }
  }
}

async fn evaluate_jobset(
  pool: &PgPool,
  jobset: &circus_common::models::ActiveJobset,
  config: &EvaluatorConfig,
  notifications_config: &NotificationsConfig,
  notification_secret_key: Option<&str>,
  nix_timeout: Duration,
  git_timeout: Duration,
) -> color_eyre::Result<()> {
  if jobset.trigger_mode == JobsetTriggerMode::Interval
    && repo::jobsets::has_unfinished_work(pool, jobset.id).await?
  {
    tracing::debug!(
      jobset = %jobset.name,
      "Skipping interval evaluation while previous work is unfinished"
    );
    repo::jobsets::update_last_checked(pool, jobset.id).await?;
    return Ok(());
  }

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

  if jobset.branch_pattern.is_some() || jobset.tag_pattern.is_some() {
    let branch_pattern = jobset.branch_pattern.clone();
    let tag_pattern = jobset.tag_pattern.clone();
    let discover_url = url.clone();
    let discover_work_dir = work_dir.clone();
    let discover_project_name = project_name.clone();
    let refs = tokio::time::timeout(
      git_timeout,
      tokio::task::spawn_blocking(move || {
        crate::git::list_matching_refs(
          &discover_url,
          &discover_work_dir,
          &discover_project_name,
          branch_pattern.as_deref(),
          tag_pattern.as_deref(),
        )
      }),
    )
    .await
    .map_err(|_| {
      color_eyre::eyre::eyre!("Git operation timed out after {git_timeout:?}")
    })???;

    tracing::info!(
      jobset = %jobset.name,
      refs = refs.len(),
      "Discovered matching repository refs"
    );

    for git_ref in refs {
      let create_eval = CreateEvaluation {
        jobset_id:      jobset.id,
        commit_hash:    git_ref.commit_hash.clone(),
        pr_number:      None,
        pr_head_branch: match git_ref.kind {
          crate::git::RefKind::Branch => Some(git_ref.name.clone()),
          crate::git::RefKind::Tag => None,
        },
        pr_base_branch: None,
        pr_action:      match git_ref.kind {
          crate::git::RefKind::Branch => None,
          crate::git::RefKind::Tag => Some(format!("tag:{}", git_ref.name)),
        },
      };

      match repo::evaluations::create(pool, create_eval).await {
        Ok(eval) => {
          evaluate_pending_eval(
            pool,
            &eval,
            jobset,
            config,
            notifications_config,
            notification_secret_key,
            nix_timeout,
            git_timeout,
          )
          .await?;
        },
        Err(CiError::Conflict(_)) => {
          tracing::debug!(
            jobset = %jobset.name,
            commit = %git_ref.commit_hash,
            "Evaluation already exists for matched ref"
          );
        },
        Err(e) => return Err(color_eyre::eyre::eyre!(e)),
      }
    }

    repo::jobsets::update_last_checked(pool, jobset.id).await?;
    return Ok(());
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
    color_eyre::eyre::eyre!("Git operation timed out after {git_timeout:?}")
  })???;

  // Query jobset inputs
  let inputs = repo::jobset_inputs::list_for_jobset(pool, jobset.id)
    .await
    .unwrap_or_default();

  // Compute inputs hash for eval caching (commit + all input values/revisions)
  let inputs_hash = compute_inputs_hash(&commit_hash, &inputs);

  // Source-change jobsets only rebuild when source/input state changes.
  // Interval jobsets intentionally create a fresh run every due tick.
  if jobset.trigger_mode == JobsetTriggerMode::SourceChange
    && let Ok(Some(cached)) =
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
  let create_eval = CreateEvaluation {
    jobset_id:      jobset.id,
    commit_hash:    commit_hash.clone(),
    pr_number:      None,
    pr_head_branch: None,
    pr_base_branch: None,
    pr_action:      None,
  };

  let eval_result = if jobset.trigger_mode == JobsetTriggerMode::Interval {
    repo::evaluations::create_interval(pool, create_eval).await
  } else {
    repo::evaluations::create_running_source_change(pool, create_eval).await
  };

  let eval = match eval_result {
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
        color_eyre::eyre::eyre!(
          "Evaluation conflict but not found: {}/{}",
          jobset.id,
          commit_hash
        )
      })?;

      match claim_existing(pool, existing).await? {
        ExistingEvaluationClaim::Claimed(eval) => eval,
        ExistingEvaluationClaim::Completed { build_count } => {
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
        },
        ExistingEvaluationClaim::Running => {
          tracing::info!(
            jobset = %jobset.name,
            commit = %commit_hash,
            "Evaluation is already running, skipping duplicate poll"
          );
          if let Err(e) =
            repo::jobsets::update_last_checked(pool, jobset.id).await
          {
            tracing::warn!(jobset = %jobset.name, "Failed to update last_checked_at: {e}");
          }
          return Ok(());
        },
        ExistingEvaluationClaim::Cancelled => {
          tracing::info!(
            jobset = %jobset.name,
            commit = %commit_hash,
            "Evaluation was cancelled, skipping duplicate poll"
          );
          return Ok(());
        },
      }
    },
    Err(e) => {
      return Err(color_eyre::eyre::eyre!(e)).with_context(|| {
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
    notification_secret_key,
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

/// Read the derivation's `requiredSystemFeatures` via `nix derivation show`.
/// Failure returns an empty list, mirroring the SSH path which treats absence
/// as "no constraint".
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
    jobsets: Vec<DeclarativeJobset>,
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
    let trigger_mode = js
      .trigger_mode
      .as_deref()
      .map(JobsetTriggerMode::from_config_str);

    let input = CreateJobset {
      project_id,
      name: js.name.clone(),
      nix_expression: js.nix_expression.clone(),
      enabled: Some(js.enabled),
      flake_mode: Some(js.flake_mode),
      check_interval: Some(js.check_interval),
      trigger_mode,
      branch: js.branch.clone(),
      branch_pattern: js.branch_pattern.clone(),
      tag_pattern: js.tag_pattern.clone(),
      scheduling_shares: Some(js.scheduling_shares),
      state,
      keep_nr: js.keep_nr,
      systems: js.systems.clone(),
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

#[cfg(test)]
mod tests {
  use circus_common::models::{EvaluationTriggerKind, JobsetTriggerMode};

  use super::accepts_pending_evaluation;

  #[test]
  fn interval_restarts_are_accepted_by_pending_queue() {
    assert!(accepts_pending_evaluation(
      JobsetTriggerMode::Interval,
      EvaluationTriggerKind::Interval,
    ));
    assert!(!accepts_pending_evaluation(
      JobsetTriggerMode::Interval,
      EvaluationTriggerKind::Manual,
    ));
  }
}
