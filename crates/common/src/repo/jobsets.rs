use circus_codegen::queries::jobsets as q;
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{ActiveJobset, CreateJobset, Jobset, JobsetState, UpdateJobset},
  validate::Validate,
};

impl TryFrom<q::JobsetRow> for Jobset {
  type Error = CiError;

  fn try_from(r: q::JobsetRow) -> Result<Self> {
    Ok(Self {
      id:                r.id,
      project_id:        r.project_id,
      name:              r.name,
      nix_expression:    r.nix_expression,
      enabled:           r.enabled,
      flake_mode:        r.flake_mode,
      check_interval:    r.check_interval,
      trigger_mode:      r.trigger_mode.parse().map_err(CiError::Internal)?,
      branch:            r.branch,
      branch_pattern:    r.branch_pattern,
      tag_pattern:       r.tag_pattern,
      scheduling_shares: r.scheduling_shares,
      created_at:        r.created_at,
      updated_at:        r.updated_at,
      state:             r.state.parse().map_err(CiError::Internal)?,
      last_checked_at:   r.last_checked_at,
      keep_nr:           r.keep_nr,
    })
  }
}

impl TryFrom<q::ActiveJobsetRow> for ActiveJobset {
  type Error = CiError;

  fn try_from(r: q::ActiveJobsetRow) -> Result<Self> {
    Ok(Self {
      id:                r.id,
      project_id:        r.project_id,
      name:              r.name,
      nix_expression:    r.nix_expression,
      enabled:           r.enabled,
      flake_mode:        r.flake_mode,
      check_interval:    r.check_interval,
      trigger_mode:      r.trigger_mode.parse().map_err(CiError::Internal)?,
      branch:            r.branch,
      branch_pattern:    r.branch_pattern,
      tag_pattern:       r.tag_pattern,
      scheduling_shares: r.scheduling_shares,
      created_at:        r.created_at,
      updated_at:        r.updated_at,
      state:             r.state.parse().map_err(CiError::Internal)?,
      last_checked_at:   r.last_checked_at,
      keep_nr:           r.keep_nr,
      project_name:      r.project_name,
      repository_url:    r.repository_url,
    })
  }
}

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

  let client = pool.get().await?;
  let row = q::create()
    .bind(
      &client,
      &input.project_id,
      &input.name,
      &input.nix_expression,
      &enabled,
      &flake_mode,
      &check_interval,
      &trigger_mode.as_str(),
      &input.branch,
      &input.branch_pattern,
      &input.tag_pattern,
      &scheduling_shares,
      &state.as_str(),
      &keep_nr,
    )
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Jobset '{}' already exists in this project",
          input.name
        ))
      } else {
        CiError::Database(e)
      }
    })?;
  Jobset::try_from(row)
}

/// Get a jobset by ID.
///
/// # Errors
///
/// Returns error if database query fails or jobset not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Jobset> {
  let client = pool.get().await?;
  q::get()
    .bind(&client, &id)
    .opt()
    .await?
    .map(Jobset::try_from)
    .transpose()?
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
  let client = pool.get().await?;
  let rows = q::list_for_project()
    .bind(&client, &project_id, &limit, &offset)
    .all()
    .await?;
  rows.into_iter().map(Jobset::try_from).collect()
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
  let client = pool.get().await?;
  let rows = q::list_all_for_project()
    .bind(&client, &project_id)
    .all()
    .await?;
  rows.into_iter().map(Jobset::try_from).collect()
}

/// Count all jobsets.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count().bind(&client).one().await?)
}

/// Count jobsets for a project.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_for_project(pool: &PgPool, project_id: Uuid) -> Result<i64> {
  let client = pool.get().await?;
  Ok(
    q::count_for_project()
      .bind(&client, &project_id)
      .one()
      .await?,
  )
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
  let branch_pattern = input.branch_pattern.or(existing.branch_pattern);
  let tag_pattern = input.tag_pattern.or(existing.tag_pattern);
  let scheduling_shares = input
    .scheduling_shares
    .unwrap_or(existing.scheduling_shares);
  let keep_nr = input.keep_nr.unwrap_or(existing.keep_nr);

  let client = pool.get().await?;
  let row = q::update()
    .bind(
      &client,
      &name,
      &nix_expression,
      &enabled,
      &flake_mode,
      &check_interval,
      &trigger_mode.as_str(),
      &branch,
      &branch_pattern,
      &tag_pattern,
      &scheduling_shares,
      &state.as_str(),
      &keep_nr,
      &id,
    )
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Jobset '{name}' already exists in this project"
        ))
      } else {
        CiError::Database(e)
      }
    })?;
  Jobset::try_from(row)
}

/// Delete a jobset.
///
/// # Errors
///
/// Returns error if database delete fails or jobset not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete().bind(&client, &id).await?;
  if affected == 0 {
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

  let client = pool.get().await?;
  let row = q::upsert()
    .bind(
      &client,
      &input.project_id,
      &input.name,
      &input.nix_expression,
      &enabled,
      &flake_mode,
      &check_interval,
      &trigger_mode.as_str(),
      &input.branch,
      &input.branch_pattern,
      &input.tag_pattern,
      &scheduling_shares,
      &state.as_str(),
      &keep_nr,
    )
    .one()
    .await?;
  Jobset::try_from(row)
}

/// List all active jobsets with project info.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_active(pool: &PgPool) -> Result<Vec<ActiveJobset>> {
  let client = pool.get().await?;
  let rows = q::list_active().bind(&client).all().await?;
  rows.into_iter().map(ActiveJobset::try_from).collect()
}

/// Mark a one-shot jobset as complete without losing its one-shot state.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn mark_one_shot_complete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  q::mark_one_shot_complete().bind(&client, &id).await?;
  Ok(())
}

/// Update the `last_checked_at` timestamp for a jobset.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn update_last_checked(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  q::update_last_checked().bind(&client, &id).await?;
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
  let client = pool.get().await?;
  let count = q::has_running_builds()
    .bind(&client, &jobset_id)
    .one()
    .await?;
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
  let client = pool.get().await?;
  let count = q::has_unfinished_work()
    .bind(&client, &jobset_id)
    .one()
    .await?;
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
  let client = pool.get().await?;
  let rows = q::list_due_for_eval().bind(&client, &limit).all().await?;
  rows.into_iter().map(ActiveJobset::try_from).collect()
}
