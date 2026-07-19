use std::collections::{HashMap, HashSet};

use circus_codegen::queries::builds as q;
use uuid::Uuid;

use crate::{
  db::{DbTransaction, GenericClient, PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{Build, BuildStats, BuildStatus, CreateBuild},
};

impl TryFrom<q::BuildRow> for Build {
  type Error = CiError;

  fn try_from(r: q::BuildRow) -> Result<Self> {
    let status = r.status.parse::<BuildStatus>().map_err(|e| {
      CiError::Internal(format!("build {} in the database has {e}", r.id))
    })?;
    Ok(Self {
      id: r.id,
      evaluation_id: r.evaluation_id,
      job_name: r.job_name,
      drv_path: r.drv_path,
      status,
      started_at: r.started_at,
      completed_at: r.completed_at,
      log_path: r.log_path,
      build_output_path: r.build_output_path,
      error_message: r.error_message,
      system: r.system,
      priority: r.priority,
      retry_count: r.retry_count,
      max_retries: r.max_retries,
      notification_pending_since: r.notification_pending_since,
      created_at: r.created_at,
      outputs: r.outputs,
      is_aggregate: r.is_aggregate,
      constituents: r.constituents,
      builder_id: r.builder_id,
      agent_machine_id: r.agent_machine_id,
      signed: r.signed,
      keep: r.keep,
      is_fod: r.is_fod,
      fod_hash: r.fod_hash,
      meta_description: r.meta_description,
      meta_license: r.meta_license,
      meta_homepage: r.meta_homepage,
      meta_maintainers: r.meta_maintainers,
      required_features: r.required_features,
      effective_features: r.effective_features,
    })
  }
}

impl From<q::GetStats> for BuildStats {
  fn from(r: q::GetStats) -> Self {
    Self {
      total_builds:         r.total_builds,
      completed_builds:     r.completed_builds,
      failed_builds:        r.failed_builds,
      running_builds:       r.running_builds,
      pending_builds:       r.pending_builds,
      avg_duration_seconds: r.avg_duration_seconds,
    }
  }
}

/// Create a new build record in pending state.
///
/// # Errors
///
/// Returns error if database insert fails or job already exists.
pub async fn create(pool: &PgPool, input: CreateBuild) -> Result<Build> {
  let client = pool.get().await?;
  create_with(&client, input).await
}

/// Create a new build record within an existing transaction.
///
/// # Errors
///
/// Returns an error if database insert fails or job already exists.
pub async fn create_in_transaction(
  tx: &DbTransaction<'_>,
  input: CreateBuild,
) -> Result<Build> {
  create_with(tx, input).await
}

async fn create_with<C: GenericClient>(
  client: &C,
  input: CreateBuild,
) -> Result<Build> {
  let is_aggregate = input.is_aggregate.unwrap_or(false);
  let is_fod = input.is_fod.unwrap_or(false);
  let row = q::create()
    .bind(
      client,
      &input.evaluation_id,
      &input.job_name,
      &input.drv_path,
      &input.system,
      &input.outputs,
      &is_aggregate,
      &input.constituents,
      &is_fod,
      &input.fod_hash,
      &input.meta_description,
      &input.meta_license,
      &input.meta_homepage,
      &input.meta_maintainers,
      &input.required_features,
    )
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Build for job '{}' already exists in this evaluation",
          input.job_name
        ))
      } else {
        CiError::Database(e)
      }
    })?;
  Build::try_from(row)
}

/// Find a succeeded build by derivation path (for build result caching).
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_completed_by_drv_path(
  pool: &PgPool,
  drv_path: &str,
) -> Result<Option<Build>> {
  let client = pool.get().await?;
  q::get_completed_by_drv_path()
    .bind(&client, &drv_path)
    .opt()
    .await?
    .map(Build::try_from)
    .transpose()
}

/// Resolve the project a build belongs to.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn project_id_for_build(
  pool: &PgPool,
  id: Uuid,
) -> Result<Option<Uuid>> {
  let client = pool.get().await?;
  Ok(q::project_id_for_build().bind(&client, &id).opt().await?)
}

/// Get a build by ID.
///
/// # Errors
///
/// Returns error if database query fails or build not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Build> {
  let client = pool.get().await?;
  q::get()
    .bind(&client, &id)
    .opt()
    .await?
    .map(Build::try_from)
    .transpose()?
    .ok_or_else(|| CiError::NotFound(format!("Build {id} not found")))
}

/// List all builds for a given evaluation.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_evaluation(
  pool: &PgPool,
  evaluation_id: Uuid,
) -> Result<Vec<Build>> {
  let client = pool.get().await?;
  let rows = q::list_for_evaluation()
    .bind(&client, &evaluation_id)
    .all()
    .await?;
  rows.into_iter().map(Build::try_from).collect()
}

/// List builds for a jobset across a bounded set of evaluations.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_jobset_evaluations(
  pool: &PgPool,
  jobset_id: Uuid,
  evaluation_ids: &[Uuid],
) -> Result<Vec<Build>> {
  if evaluation_ids.is_empty() {
    return Ok(Vec::new());
  }

  let client = pool.get().await?;
  let rows = q::list_for_jobset_evaluations()
    .bind(&client, &jobset_id, &evaluation_ids)
    .all()
    .await?;
  rows.into_iter().map(Build::try_from).collect()
}

/// List pending builds, prioritizing constrained jobs before fungible ones so
/// scarce builder capabilities are reserved for the work that needs them.
///
/// `schedulable_capacity` is the fair-share denominator, so it needs to
/// include agent slots as well as local workers.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn list_pending(
  pool: &PgPool,
  limit: i64,
  schedulable_capacity: i32,
) -> Result<Vec<Build>> {
  let client = pool.get().await?;
  let rows = q::list_pending()
    .bind(&client, &schedulable_capacity, &limit)
    .all()
    .await?;
  rows.into_iter().map(Build::try_from).collect()
}

/// Atomically claim a pending build by setting it to running. The advisory
/// lock and the running twin check keep duplicate pending builds of one
/// `drv_path` from both dispatching.
///
/// # Returns
///
/// Returns `None` if the build was already claimed by another worker.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn start(pool: &PgPool, id: Uuid) -> Result<Option<Build>> {
  let client = pool.get().await?;
  q::start()
    .bind(&client, &id)
    .opt()
    .await?
    .map(Build::try_from)
    .transpose()
}

/// Record that a build-start notification has been attempted.
///
/// Requeues keep this marker because they are infrastructure retries, whereas
/// manual restarts clear it because they are user-visible new runs.
///
/// # Errors
///
/// Returns an error if the database update fails.
pub async fn mark_started_notified(pool: &PgPool, id: Uuid) -> Result<bool> {
  let client = pool.get().await?;
  let claimed = q::mark_started_notified().bind(&client, &id).opt().await?;
  Ok(claimed.is_some())
}

/// Return a running build to the pending queue without counting a retry.
///
/// Use this for infrastructure loss rather than build failure. The status
/// guard protects builds that finished or were cancelled meanwhile.
///
/// # Errors
///
/// Returns an error if the database update fails.
pub async fn requeue(pool: &PgPool, id: Uuid) -> Result<Option<Build>> {
  let client = pool.get().await?;
  q::requeue()
    .bind(&client, &id)
    .opt()
    .await?
    .map(Build::try_from)
    .transpose()
}

/// Return a failed build to the pending queue, counting a retry and clearing
/// dispatch-time effective features so they are recomputed on redispatch.
///
/// # Errors
///
/// Returns an error if the database update fails.
pub async fn retry(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  q::retry().bind(&client, &id).await?;
  Ok(())
}

/// Mark a build as completed with final status and outputs.
///
/// # Errors
///
/// Returns an error if the database update fails or the build was not found.
pub async fn complete(
  pool: &PgPool,
  id: Uuid,
  status: BuildStatus,
  log_path: Option<&str>,
  build_output_path: Option<&str>,
  error_message: Option<&str>,
) -> Result<Build> {
  let client = pool.get().await?;
  q::complete()
    .bind(
      &client,
      &status.as_db_str(),
      &log_path,
      &build_output_path,
      &error_message,
      &id,
    )
    .opt()
    .await?
    .map(Build::try_from)
    .transpose()?
    .ok_or_else(|| CiError::NotFound(format!("Build {id} not found")))
}

/// List pending builds in scheduler order: highest priority first, then
/// constrained builds before fungible builds, then oldest first. Mirrors the
/// ordering the queue runner uses (minus the share-deficit factor, which
/// depends on live worker counts) so the dashboard queue page can show builds
/// in the order they will be picked.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_pending_in_scheduler_order(
  pool: &PgPool,
  limit: i64,
  offset: i64,
) -> Result<Vec<Build>> {
  list_pending_in_scheduler_order_filtered(pool, None, None, limit, offset)
    .await
}

/// Pending builds in scheduler order, optionally narrowed by system and a
/// case-insensitive job-name substring.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_pending_in_scheduler_order_filtered(
  pool: &PgPool,
  system: Option<&str>,
  job_name: Option<&str>,
  limit: i64,
  offset: i64,
) -> Result<Vec<Build>> {
  let client = pool.get().await?;
  let rows = q::list_pending_in_scheduler_order()
    .bind(&client, &system, &job_name, &limit, &offset)
    .all()
    .await?;
  rows.into_iter().map(Build::try_from).collect()
}

/// List up to 512 pending builds for any of the given systems, in dispatch
/// priority order. Used by the ephemeral-agent autoscaler to estimate demand.
///
/// # Errors
///
/// Returns error if the database query fails.
pub async fn list_pending_for_systems(
  pool: &PgPool,
  systems: &[String],
) -> Result<Vec<Build>> {
  let client = pool.get().await?;
  let rows = q::list_pending_for_systems()
    .bind(&client, &systems)
    .all()
    .await?;
  rows.into_iter().map(Build::try_from).collect()
}

/// The set of system features any pending build for `system` requires.
///
/// This is used by capability-preserving scheduling to decide which builder
/// capabilities are currently contended, so a versatile builder is not handed
/// fungible work while a build that needs one of its scarce features is queued.
///
/// # Errors
///
/// Returns error if the database query fails.
pub async fn pending_feature_demand(
  pool: &PgPool,
  system: &str,
) -> Result<HashSet<String>> {
  let client = pool.get().await?;
  let rows = q::pending_feature_demand()
    .bind(&client, &system)
    .all()
    .await?;
  Ok(rows.into_iter().collect())
}

/// Atomically increment a pending build's priority by `delta`. The row is
/// only updated if the build is still in the pending state, so a build
/// the scheduler has already claimed is left alone.
///
/// # Returns
///
/// The updated `Build` row, or `None` if the build does not exist or is
/// no longer pending.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn bump_priority(
  pool: &PgPool,
  id: Uuid,
  delta: i32,
) -> Result<Option<Build>> {
  let client = pool.get().await?;
  q::bump_priority()
    .bind(&client, &delta, &id)
    .opt()
    .await?
    .map(Build::try_from)
    .transpose()
}

/// List recent builds ordered by creation time.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_recent(pool: &PgPool, limit: i64) -> Result<Vec<Build>> {
  let client = pool.get().await?;
  let rows = q::list_recent().bind(&client, &limit).all().await?;
  rows.into_iter().map(Build::try_from).collect()
}

/// List all builds for a project.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_project(
  pool: &PgPool,
  project_id: Uuid,
) -> Result<Vec<Build>> {
  let client = pool.get().await?;
  let rows = q::list_for_project()
    .bind(&client, &project_id)
    .all()
    .await?;
  rows.into_iter().map(Build::try_from).collect()
}

/// Get aggregate build statistics.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_stats(pool: &PgPool) -> Result<BuildStats> {
  let client = pool.get().await?;
  match q::get_stats().bind(&client).opt().await {
    Ok(Some(stats)) => Ok(BuildStats::from(stats)),
    Ok(None) => {
      tracing::warn!(
        "build_stats view returned no rows, returning default stats"
      );
      Ok(BuildStats::default())
    },
    Err(e) => {
      tracing::error!(error = %e, "Failed to fetch build stats");
      Err(CiError::Database(e))
    },
  }
}

/// Reset builds that were left in 'running' state, excluding IDs that are
/// known to still be active in the current runner process.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn reset_orphaned_excluding(
  pool: &PgPool,
  older_than_secs: i64,
  excluded_ids: &[Uuid],
) -> Result<u64> {
  let client = pool.get().await?;
  Ok(
    q::reset_orphaned()
      .bind(&client, &older_than_secs, &excluded_ids)
      .await?,
  )
}

/// Reset builds that were left in 'running' state (orphaned by a crashed
/// runner). Resets every row older than the threshold; the caller is
/// expected to run this periodically so a crash that orphans many builds
/// converges rather than stranding the tail.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn reset_orphaned(
  pool: &PgPool,
  older_than_secs: i64,
) -> Result<u64> {
  reset_orphaned_excluding(pool, older_than_secs, &[]).await
}

/// List builds with optional `evaluation_id`, status, system, and `job_name`
/// filters, with pagination.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_filtered(
  pool: &PgPool,
  evaluation_id: Option<Uuid>,
  status: Option<&str>,
  system: Option<&str>,
  job_name: Option<&str>,
  limit: i64,
  offset: i64,
) -> Result<Vec<Build>> {
  let client = pool.get().await?;
  let rows = q::list_filtered()
    .bind(
      &client,
      &evaluation_id,
      &status,
      &system,
      &job_name,
      &limit,
      &offset,
    )
    .all()
    .await?;
  rows.into_iter().map(Build::try_from).collect()
}

/// Count builds matching filter criteria.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_filtered(
  pool: &PgPool,
  evaluation_id: Option<Uuid>,
  status: Option<&str>,
  system: Option<&str>,
  job_name: Option<&str>,
) -> Result<i64> {
  let client = pool.get().await?;
  Ok(
    q::count_filtered()
      .bind(&client, &evaluation_id, &status, &system, &job_name)
      .one()
      .await?,
  )
}

/// Return the subset of the given build IDs whose status is 'cancelled'.
/// Used by the cancel-checker loop to detect builds cancelled while running.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_cancelled_among(
  pool: &PgPool,
  build_ids: &[Uuid],
) -> Result<Vec<Uuid>> {
  if build_ids.is_empty() {
    return Ok(Vec::new());
  }
  let client = pool.get().await?;
  Ok(
    q::get_cancelled_among()
      .bind(&client, &build_ids)
      .all()
      .await?,
  )
}

/// Cancel a build.
///
/// # Errors
///
/// Returns error if database update fails or build not in cancellable state.
pub async fn cancel(pool: &PgPool, id: Uuid) -> Result<Build> {
  let client = pool.get().await?;
  q::cancel()
    .bind(&client, &id)
    .opt()
    .await?
    .map(Build::try_from)
    .transpose()?
    .ok_or_else(|| {
      CiError::NotFound(format!(
        "Build {id} not found or not in a cancellable state"
      ))
    })
}

/// Cancel a build and all its transitive dependents.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn cancel_cascade(pool: &PgPool, id: Uuid) -> Result<Vec<Build>> {
  let mut cancelled = Vec::new();

  // Cancel the target build
  if let Ok(build) = cancel(pool, id).await {
    cancelled.push(build);
  }

  // Find and cancel all dependents recursively
  let mut to_cancel: Vec<Uuid> = vec![id];
  while let Some(build_id) = to_cancel.pop() {
    let dependents = {
      let client = pool.get().await?;
      q::cancel_cascade_dependents()
        .bind(&client, &build_id)
        .all()
        .await?
    };

    for dep_id in dependents {
      if let Ok(build) = cancel(pool, dep_id).await {
        to_cancel.push(dep_id);
        cancelled.push(build);
      }
    }
  }

  Ok(cancelled)
}

/// Restart a build by resetting it to pending state.
/// Only works for failed, succeeded, cancelled, or `cached_failure` builds.
///
/// # Errors
///
/// Returns error if database update fails or build not in restartable state.
pub async fn restart(pool: &PgPool, id: Uuid) -> Result<Build> {
  let build = {
    let client = pool.get().await?;
    q::restart()
      .bind(&client, &id)
      .opt()
      .await?
      .map(Build::try_from)
      .transpose()?
      .ok_or_else(|| {
        CiError::NotFound(format!(
          "Build {id} not found or not in a restartable state"
        ))
      })?
  };

  if let Err(e) =
    super::failed_paths_cache::invalidate(pool, &build.drv_path).await
  {
    tracing::warn!(build_id = %id, "Failed to invalidate failed paths cache: {e}");
  }

  Ok(build)
}

/// Persist the dispatch-time effective features for a build.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn set_effective_features(
  pool: &PgPool,
  id: Uuid,
  features: &[String],
) -> Result<()> {
  let client = pool.get().await?;
  q::set_effective_features()
    .bind(&client, &features, &id)
    .await?;
  Ok(())
}

/// Mark a build's outputs as signed.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn mark_signed(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  q::mark_signed().bind(&client, &id).await?;
  Ok(())
}

/// Batch-fetch completed builds by derivation paths.
///
/// # Returns
///
/// Returns a map from `drv_path` to Build for deduplication.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_completed_by_drv_paths(
  pool: &PgPool,
  drv_paths: &[String],
) -> Result<HashMap<String, Build>> {
  if drv_paths.is_empty() {
    return Ok(HashMap::new());
  }
  let client = pool.get().await?;
  let rows = q::get_completed_by_drv_paths()
    .bind(&client, &drv_paths)
    .all()
    .await?;

  rows
    .into_iter()
    .map(|r| Build::try_from(r).map(|b| (b.drv_path.clone(), b)))
    .collect()
}

/// Return the set of build IDs that have `keep = true` (GC-pinned).
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_pinned_ids(pool: &PgPool) -> Result<HashSet<Uuid>> {
  let client = pool.get().await?;
  let rows = q::list_pinned_ids().bind(&client).all().await?;
  Ok(rows.into_iter().collect())
}

/// Set the `keep` (GC pin) flag on a build.
///
/// # Errors
///
/// Returns error if database update fails or build not found.
pub async fn set_keep(pool: &PgPool, id: Uuid, keep: bool) -> Result<Build> {
  let client = pool.get().await?;
  q::set_keep()
    .bind(&client, &keep, &id)
    .opt()
    .await?
    .map(Build::try_from)
    .transpose()?
    .ok_or_else(|| CiError::NotFound(format!("Build {id} not found")))
}

/// Set the `builder_id` for a build.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn set_builder(
  pool: &PgPool,
  id: Uuid,
  builder_id: Uuid,
) -> Result<()> {
  let client = pool.get().await?;
  q::set_builder().bind(&client, &builder_id, &id).await?;
  Ok(())
}

/// Set the `agent_machine_id` for a build.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn set_agent(
  pool: &PgPool,
  id: Uuid,
  machine_id: Uuid,
) -> Result<()> {
  let client = pool.get().await?;
  q::set_agent().bind(&client, &machine_id, &id).await?;
  Ok(())
}

/// List constituent builds of an aggregate build.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_constituents(
  pool: &PgPool,
  build_id: Uuid,
) -> Result<Vec<Build>> {
  let client = pool.get().await?;
  let rows = q::list_constituents()
    .bind(&client, &build_id)
    .all()
    .await?;
  rows.into_iter().map(Build::try_from).collect()
}

/// Delete a build by ID.
///
/// # Errors
///
/// Returns error if database query fails or build not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete().bind(&client, &id).await?;
  if affected == 0 {
    return Err(CiError::NotFound(format!("Build {id} not found")));
  }
  Ok(())
}
