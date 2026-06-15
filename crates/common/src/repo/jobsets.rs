use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::{ActiveJobset, CreateJobset, Jobset, JobsetState, UpdateJobset},
  validate::Validate,
};

/// Create a new jobset with defaults applied.
///
/// # Errors
///
/// Returns error if database insert fails or jobset already exists.
pub async fn create(pool: &PgPool, input: CreateJobset) -> Result<Jobset> {
  input.validate().map_err(CiError::Validation)?;
  let state = input.state.unwrap_or(JobsetState::Enabled);
  // Sync enabled with state if state was explicitly set, otherwise use
  // input.enabled
  let enabled = if input.state.is_some() {
    state.is_evaluable()
  } else {
    input.enabled.unwrap_or_else(|| state.is_evaluable())
  };
  let flake_mode = input.flake_mode.unwrap_or(true);
  let check_interval = input.check_interval.unwrap_or(60);
  let trigger_mode = input.trigger_mode.unwrap_or_default();
  let scheduling_shares = input.scheduling_shares.unwrap_or(100);
  let keep_nr = input.keep_nr.unwrap_or(3);

  sqlx::query_as::<_, Jobset>(
    "INSERT INTO jobsets (project_id, name, nix_expression, enabled, \
     flake_mode, check_interval, trigger_mode, branch, scheduling_shares, \
     state, keep_nr) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) \
     RETURNING *",
  )
  .bind(input.project_id)
  .bind(&input.name)
  .bind(&input.nix_expression)
  .bind(enabled)
  .bind(flake_mode)
  .bind(check_interval)
  .bind(trigger_mode.as_str())
  .bind(&input.branch)
  .bind(scheduling_shares)
  .bind(state.as_str())
  .bind(keep_nr)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Jobset '{}' already exists in this project",
          input.name
        ))
      },
      _ => CiError::Database(e),
    }
  })
}

/// Get a jobset by ID.
///
/// # Errors
///
/// Returns error if database query fails or jobset not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Jobset> {
  sqlx::query_as::<_, Jobset>("SELECT * FROM jobsets WHERE id = $1")
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Jobset {id} not found")))
}

/// List all jobsets for a project.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_project(
  pool: &PgPool,
  project_id: Uuid,
  limit: i64,
  offset: i64,
) -> Result<Vec<Jobset>> {
  sqlx::query_as::<_, Jobset>(
    "SELECT * FROM jobsets WHERE project_id = $1 ORDER BY created_at DESC \
     LIMIT $2 OFFSET $3",
  )
  .bind(project_id)
  .bind(limit)
  .bind(offset)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// List all jobsets for a project without pagination. Used by webhook
/// fan-out so a project with more than the page-default number of jobsets
/// is not silently truncated.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_all_for_project(
  pool: &PgPool,
  project_id: Uuid,
) -> Result<Vec<Jobset>> {
  sqlx::query_as::<_, Jobset>(
    "SELECT * FROM jobsets WHERE project_id = $1 ORDER BY created_at DESC",
  )
  .bind(project_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Count jobsets for a project.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_for_project(pool: &PgPool, project_id: Uuid) -> Result<i64> {
  let row: (i64,) =
    sqlx::query_as("SELECT COUNT(*) FROM jobsets WHERE project_id = $1")
      .bind(project_id)
      .fetch_one(pool)
      .await
      .map_err(CiError::Database)?;
  Ok(row.0)
}

/// Update a jobset with partial fields.
///
/// # Errors
///
/// Returns error if database update fails or jobset not found.
pub async fn update(
  pool: &PgPool,
  id: Uuid,
  input: UpdateJobset,
) -> Result<Jobset> {
  input.validate().map_err(CiError::Validation)?;
  let existing = get(pool, id).await?;

  let name = input.name.unwrap_or(existing.name);
  let nix_expression = input.nix_expression.unwrap_or(existing.nix_expression);
  let state = input.state.unwrap_or(existing.state);
  // Sync enabled with state if state was explicitly set
  let enabled = if input.state.is_some() {
    state.is_evaluable()
  } else {
    input.enabled.unwrap_or(existing.enabled)
  };
  let flake_mode = input.flake_mode.unwrap_or(existing.flake_mode);
  let check_interval = input.check_interval.unwrap_or(existing.check_interval);
  let trigger_mode = input.trigger_mode.unwrap_or(existing.trigger_mode);
  let branch = input.branch.or(existing.branch);
  let scheduling_shares = input
    .scheduling_shares
    .unwrap_or(existing.scheduling_shares);
  let keep_nr = input.keep_nr.unwrap_or(existing.keep_nr);

  sqlx::query_as::<_, Jobset>(
    "UPDATE jobsets SET name = $1, nix_expression = $2, enabled = $3, \
     flake_mode = $4, check_interval = $5, trigger_mode = $6, branch = $7, \
     scheduling_shares = $8, state = $9, keep_nr = $10 WHERE id = $11 \
     RETURNING *",
  )
  .bind(&name)
  .bind(&nix_expression)
  .bind(enabled)
  .bind(flake_mode)
  .bind(check_interval)
  .bind(trigger_mode.as_str())
  .bind(&branch)
  .bind(scheduling_shares)
  .bind(state.as_str())
  .bind(keep_nr)
  .bind(id)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Jobset '{name}' already exists in this project"
        ))
      },
      _ => CiError::Database(e),
    }
  })
}

/// Delete a jobset.
///
/// # Errors
///
/// Returns error if database delete fails or jobset not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM jobsets WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;

  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("Jobset {id} not found")));
  }

  Ok(())
}

/// Insert or update a jobset by name.
///
/// # Errors
///
/// Returns error if database operation fails.
pub async fn upsert(pool: &PgPool, input: CreateJobset) -> Result<Jobset> {
  input.validate().map_err(CiError::Validation)?;
  let state = input.state.unwrap_or(JobsetState::Enabled);
  // Sync enabled with state if state was explicitly set, otherwise use
  // input.enabled
  let enabled = if input.state.is_some() {
    state.is_evaluable()
  } else {
    input.enabled.unwrap_or_else(|| state.is_evaluable())
  };
  let flake_mode = input.flake_mode.unwrap_or(true);
  let check_interval = input.check_interval.unwrap_or(60);
  let trigger_mode = input.trigger_mode.unwrap_or_default();
  let scheduling_shares = input.scheduling_shares.unwrap_or(100);
  let keep_nr = input.keep_nr.unwrap_or(3);

  sqlx::query_as::<_, Jobset>(
    "INSERT INTO jobsets (project_id, name, nix_expression, enabled, \
     flake_mode, check_interval, trigger_mode, branch, scheduling_shares, \
     state, keep_nr) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11) ON \
     CONFLICT (project_id, name) DO UPDATE SET nix_expression = \
     EXCLUDED.nix_expression, enabled = EXCLUDED.enabled, flake_mode = \
     EXCLUDED.flake_mode, check_interval = EXCLUDED.check_interval, \
     trigger_mode = EXCLUDED.trigger_mode, branch = EXCLUDED.branch, \
     scheduling_shares = EXCLUDED.scheduling_shares, state = EXCLUDED.state, \
     keep_nr = EXCLUDED.keep_nr RETURNING *",
  )
  .bind(input.project_id)
  .bind(&input.name)
  .bind(&input.nix_expression)
  .bind(enabled)
  .bind(flake_mode)
  .bind(check_interval)
  .bind(trigger_mode.as_str())
  .bind(&input.branch)
  .bind(scheduling_shares)
  .bind(state.as_str())
  .bind(keep_nr)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)
}

/// List all active jobsets with project info.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_active(pool: &PgPool) -> Result<Vec<ActiveJobset>> {
  sqlx::query_as::<_, ActiveJobset>("SELECT * FROM active_jobsets")
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

/// Mark a one-shot jobset as complete (set state to disabled).
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn mark_one_shot_complete(pool: &PgPool, id: Uuid) -> Result<()> {
  sqlx::query(
    "UPDATE jobsets SET state = 'disabled', enabled = false WHERE id = $1 AND \
     state = 'one_shot'",
  )
  .bind(id)
  .execute(pool)
  .await
  .map_err(CiError::Database)?;
  Ok(())
}

/// Update the `last_checked_at` timestamp for a jobset.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn update_last_checked(pool: &PgPool, id: Uuid) -> Result<()> {
  sqlx::query("UPDATE jobsets SET last_checked_at = NOW() WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await
    .map_err(CiError::Database)?;
  Ok(())
}

/// Check if a jobset has any running builds.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn has_running_builds(
  pool: &PgPool,
  jobset_id: Uuid,
) -> Result<bool> {
  let (count,): (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM builds b JOIN evaluations e ON b.evaluation_id = \
     e.id WHERE e.jobset_id = $1 AND b.status = 'running'",
  )
  .bind(jobset_id)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)?;
  Ok(count > 0)
}

/// Check if a jobset has any active evaluation or unfinished build work.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn has_unfinished_work(
  pool: &PgPool,
  jobset_id: Uuid,
) -> Result<bool> {
  let (count,): (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM evaluations e LEFT JOIN builds b ON b.evaluation_id \
     = e.id WHERE e.jobset_id = $1 AND (e.status IN ('pending', 'running') OR \
     b.status IN ('pending', 'running'))",
  )
  .bind(jobset_id)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)?;
  Ok(count > 0)
}

/// List jobsets that are due for evaluation based on their `check_interval`.
/// Returns jobsets where `last_checked_at` is NULL or older than
/// `check_interval` seconds.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_due_for_eval(
  pool: &PgPool,
  limit: i64,
) -> Result<Vec<ActiveJobset>> {
  sqlx::query_as::<_, ActiveJobset>(
    "SELECT * FROM active_jobsets WHERE last_checked_at IS NULL OR \
     last_checked_at < NOW() - (check_interval || ' seconds')::interval ORDER \
     BY last_checked_at NULLS FIRST LIMIT $1",
  )
  .bind(limit)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}
