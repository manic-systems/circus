use std::collections::HashSet;

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result, SqlxResultExt},
  models::{Build, BuildStats, BuildStatus, CreateBuild},
};

/// Create a new build record in pending state.
///
/// # Errors
///
/// Returns error if database insert fails or job already exists.
pub async fn create(pool: &PgPool, input: CreateBuild) -> Result<Build> {
  let is_aggregate = input.is_aggregate.unwrap_or(false);
  let is_fod = input.is_fod.unwrap_or(false);
  sqlx::query_as::<_, Build>(
    "INSERT INTO builds (evaluation_id, job_name, drv_path, status, system, \
     outputs, is_aggregate, constituents, is_fod, fod_hash, meta_description, \
     meta_license, meta_homepage, meta_maintainers, required_features) VALUES \
     ($1, $2, $3, 'pending', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14) \
     RETURNING *",
  )
  .bind(input.evaluation_id)
  .bind(&input.job_name)
  .bind(&input.drv_path)
  .bind(&input.system)
  .bind(&input.outputs)
  .bind(is_aggregate)
  .bind(&input.constituents)
  .bind(is_fod)
  .bind(&input.fod_hash)
  .bind(&input.meta_description)
  .bind(&input.meta_license)
  .bind(&input.meta_homepage)
  .bind(&input.meta_maintainers)
  .bind(&input.required_features)
  .fetch_one(pool)
  .await
  .on_unique_violation(|| {
    format!(
      "Build for job '{}' already exists in this evaluation",
      input.job_name
    )
  })
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
  Ok(
    sqlx::query_as::<_, Build>(
      "SELECT * FROM builds WHERE drv_path = $1 AND status = 'succeeded' \
       LIMIT 1",
    )
    .bind(drv_path)
    .fetch_optional(pool)
    .await?,
  )
}

/// Get a build by ID.
///
/// # Errors
///
/// Returns error if database query fails or build not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Build> {
  sqlx::query_as::<_, Build>("SELECT * FROM builds WHERE id = $1")
    .bind(id)
    .fetch_optional(pool)
    .await?
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
  Ok(
    sqlx::query_as::<_, Build>(
      "SELECT * FROM builds WHERE evaluation_id = $1 ORDER BY created_at DESC",
    )
    .bind(evaluation_id)
    .fetch_all(pool)
    .await?,
  )
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

  Ok(
    sqlx::query_as::<_, Build>(
      "SELECT b.* FROM builds b JOIN evaluations e ON b.evaluation_id = e.id \
       WHERE e.jobset_id = $1 AND b.evaluation_id = ANY($2) ORDER BY \
       b.job_name ASC, e.evaluation_time DESC",
    )
    .bind(jobset_id)
    .bind(evaluation_ids)
    .fetch_all(pool)
    .await?,
  )
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
  Ok(
    sqlx::query_as::<_, Build>(
      "WITH eligible_pending AS ( SELECT b.* FROM builds b WHERE b.status = \
       'pending' AND NOT EXISTS ( SELECT 1 FROM build_dependencies bd JOIN \
       builds dep ON dep.id = bd.dependency_build_id WHERE bd.build_id = b.id \
       AND dep.status != 'succeeded' ) ), running_counts AS ( SELECT \
       e.jobset_id, COUNT(*) AS running FROM builds b JOIN evaluations e ON \
       b.evaluation_id = e.id WHERE b.status = 'running' GROUP BY e.jobset_id \
       ), active_shares AS ( SELECT j.id AS jobset_id, j.scheduling_shares, \
       COALESCE(rc.running, 0) AS running, SUM(j.scheduling_shares) OVER () \
       AS total_shares FROM jobsets j JOIN evaluations e2 ON e2.jobset_id = \
       j.id JOIN eligible_pending b2 ON b2.evaluation_id = e2.id LEFT JOIN \
       running_counts rc ON rc.jobset_id = j.id WHERE j.scheduling_shares > 0 \
       GROUP BY j.id, j.scheduling_shares, rc.running ) SELECT b.* FROM \
       eligible_pending b JOIN evaluations e ON b.evaluation_id = e.id JOIN \
       active_shares ash ON ash.jobset_id = e.jobset_id ORDER BY b.priority \
       DESC, cardinality(COALESCE(b.effective_features, b.required_features)) \
       DESC, (ash.scheduling_shares::float / GREATEST(ash.total_shares, 1) - \
       ash.running::float / GREATEST($2, 1)) DESC, b.created_at ASC, b.id ASC \
       LIMIT $1",
    )
    .bind(limit)
    .bind(schedulable_capacity)
    .fetch_all(pool)
    .await?,
  )
}

/// Atomically claim a pending build by setting it to running.
///
/// # Returns
///
/// Returns `None` if the build was already claimed by another worker.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn start(pool: &PgPool, id: Uuid) -> Result<Option<Build>> {
  Ok(
    sqlx::query_as::<_, Build>(
      "WITH candidate AS ( SELECT id FROM builds WHERE id = $1 AND status = \
       'pending' FOR UPDATE SKIP LOCKED ) UPDATE builds SET status = \
       'running', started_at = NOW() FROM candidate WHERE builds.id = \
       candidate.id RETURNING builds.*",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?,
  )
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
  let claimed = sqlx::query_scalar::<_, Uuid>(
    "UPDATE builds SET started_notified_at = NOW() WHERE id = $1 AND \
     started_notified_at IS NULL RETURNING id",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?;
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
  Ok(
    sqlx::query_as::<_, Build>(
      "WITH bumped AS ( UPDATE builds SET status = 'pending', started_at = \
       NULL, completed_at = NULL, effective_features = NULL WHERE id = $1 AND \
       status = 'running' RETURNING * ), cleared AS ( DELETE FROM build_steps \
       WHERE build_id = $1 AND EXISTS (SELECT 1 FROM bumped) ) SELECT * FROM \
       bumped",
    )
    .bind(id)
    .fetch_optional(pool)
    .await?,
  )
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
  sqlx::query_as::<_, Build>(
    "UPDATE builds SET status = $1, completed_at = NOW(), log_path = $2, \
     build_output_path = $3, error_message = $4 WHERE id = $5 RETURNING *",
  )
  .bind(status)
  .bind(log_path)
  .bind(build_output_path)
  .bind(error_message)
  .bind(id)
  .fetch_optional(pool)
  .await?
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
  Ok(
    sqlx::query_as::<_, Build>(
      "SELECT * FROM builds WHERE status = 'pending' ORDER BY priority DESC, \
       cardinality(COALESCE(effective_features, required_features)) DESC, \
       created_at ASC, id ASC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?,
  )
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
  let rows = sqlx::query_as(
    "SELECT DISTINCT unnest(COALESCE(effective_features, required_features)) \
     FROM builds WHERE status = 'pending' AND system = $1",
  )
  .bind(system)
  .fetch_all(pool)
  .await?;
  Ok(rows.into_iter().map(|(feature,)| feature).collect())
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
  Ok(
    sqlx::query_as::<_, Build>(
      "UPDATE builds SET priority = priority + $2 WHERE id = $1 AND status = \
       'pending' RETURNING *",
    )
    .bind(id)
    .bind(delta)
    .fetch_optional(pool)
    .await?,
  )
}

/// List recent builds ordered by creation time.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_recent(pool: &PgPool, limit: i64) -> Result<Vec<Build>> {
  Ok(
    sqlx::query_as::<_, Build>(
      "SELECT * FROM builds ORDER BY created_at DESC LIMIT $1",
    )
    .bind(limit)
    .fetch_all(pool)
    .await?,
  )
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
  Ok(
    sqlx::query_as::<_, Build>(
      "SELECT b.* FROM builds b JOIN evaluations e ON b.evaluation_id = e.id \
       JOIN jobsets j ON e.jobset_id = j.id WHERE j.project_id = $1 ORDER BY \
       b.created_at DESC",
    )
    .bind(project_id)
    .fetch_all(pool)
    .await?,
  )
}

/// Get aggregate build statistics.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_stats(pool: &PgPool) -> Result<BuildStats> {
  match sqlx::query_as::<_, BuildStats>("SELECT * FROM build_stats")
    .fetch_optional(pool)
    .await
  {
    Ok(Some(stats)) => Ok(stats),
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
  let result = sqlx::query(
    "UPDATE builds SET status = 'pending', started_at = NULL, \
     effective_features = NULL WHERE status = 'running' AND started_at < \
     NOW() - make_interval(secs => $1)",
  )
  .bind(older_than_secs)
  .execute(pool)
  .await?;

  Ok(result.rows_affected())
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
  Ok(
    sqlx::query_as::<_, Build>(
      "SELECT * FROM builds WHERE ($1::uuid IS NULL OR evaluation_id = $1) \
       AND ($2::text IS NULL OR status = $2) AND ($3::text IS NULL OR system \
       = $3) AND ($4::text IS NULL OR job_name ILIKE '%' || $4 || '%') ORDER \
       BY created_at DESC LIMIT $5 OFFSET $6",
    )
    .bind(evaluation_id)
    .bind(status)
    .bind(system)
    .bind(job_name)
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?,
  )
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
  let row: (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM builds WHERE ($1::uuid IS NULL OR evaluation_id = \
     $1) AND ($2::text IS NULL OR status = $2) AND ($3::text IS NULL OR \
     system = $3) AND ($4::text IS NULL OR job_name ILIKE '%' || $4 || '%')",
  )
  .bind(evaluation_id)
  .bind(status)
  .bind(system)
  .bind(job_name)
  .fetch_one(pool)
  .await?;
  Ok(row.0)
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
  let rows: Vec<(Uuid,)> = sqlx::query_as(
    "SELECT id FROM builds WHERE id = ANY($1) AND status = 'cancelled'",
  )
  .bind(build_ids)
  .fetch_all(pool)
  .await?;

  Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Cancel a build.
///
/// # Errors
///
/// Returns error if database update fails or build not in cancellable state.
pub async fn cancel(pool: &PgPool, id: Uuid) -> Result<Build> {
  sqlx::query_as::<_, Build>(
    "UPDATE builds SET status = 'cancelled', completed_at = NOW() WHERE id = \
     $1 AND status IN ('pending', 'running') RETURNING *",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
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
    let dependents: Vec<(Uuid,)> = sqlx::query_as(
      "SELECT build_id FROM build_dependencies WHERE dependency_build_id = $1",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?;

    for (dep_id,) in dependents {
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
  let build = sqlx::query_as::<_, Build>(
    "UPDATE builds SET status = 'pending', started_at = NULL, completed_at = \
     NULL, log_path = NULL, build_output_path = NULL, error_message = NULL, \
     started_notified_at = NULL, effective_features = NULL, retry_count = \
     retry_count + 1 WHERE id = $1 AND status IN ('failed', 'succeeded', \
     'cancelled', 'cached_failure') RETURNING *",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| {
    CiError::NotFound(format!(
      "Build {id} not found or not in a restartable state"
    ))
  })?;

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
  sqlx::query("UPDATE builds SET effective_features = $1 WHERE id = $2")
    .bind(features)
    .bind(id)
    .execute(pool)
    .await?;
  Ok(())
}

/// Mark a build's outputs as signed.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn mark_signed(pool: &PgPool, id: Uuid) -> Result<()> {
  sqlx::query("UPDATE builds SET signed = true WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
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
) -> Result<std::collections::HashMap<String, Build>> {
  if drv_paths.is_empty() {
    return Ok(std::collections::HashMap::new());
  }
  let builds = sqlx::query_as::<_, Build>(
    "SELECT DISTINCT ON (drv_path) * FROM builds WHERE drv_path = ANY($1) AND \
     status = 'succeeded' ORDER BY drv_path, completed_at DESC",
  )
  .bind(drv_paths)
  .fetch_all(pool)
  .await?;

  Ok(
    builds
      .into_iter()
      .map(|b| (b.drv_path.clone(), b))
      .collect(),
  )
}

/// Return the set of build IDs that have `keep = true` (GC-pinned).
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_pinned_ids(pool: &PgPool) -> Result<HashSet<Uuid>> {
  let rows: Vec<(Uuid,)> =
    sqlx::query_as("SELECT id FROM builds WHERE keep = true")
      .fetch_all(pool)
      .await?;
  Ok(rows.into_iter().map(|(id,)| id).collect())
}

/// Set the `keep` (GC pin) flag on a build.
///
/// # Errors
///
/// Returns error if database update fails or build not found.
pub async fn set_keep(pool: &PgPool, id: Uuid, keep: bool) -> Result<Build> {
  sqlx::query_as::<_, Build>(
    "UPDATE builds SET keep = $1 WHERE id = $2 RETURNING *",
  )
  .bind(keep)
  .bind(id)
  .fetch_optional(pool)
  .await?
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
  sqlx::query("UPDATE builds SET builder_id = $1 WHERE id = $2")
    .bind(builder_id)
    .bind(id)
    .execute(pool)
    .await?;
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
  sqlx::query("UPDATE builds SET agent_machine_id = $1 WHERE id = $2")
    .bind(machine_id)
    .bind(id)
    .execute(pool)
    .await?;
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
  Ok(
    sqlx::query_as::<_, Build>(
      "SELECT b.* FROM builds b JOIN build_dependencies bd ON b.id = \
       bd.dependency_build_id WHERE bd.build_id = $1 ORDER BY b.created_at",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?,
  )
}

/// Delete a build by ID.
///
/// # Errors
///
/// Returns error if database query fails or build not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM builds WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;

  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("Build {id} not found")));
  }

  Ok(())
}
