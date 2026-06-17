use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result, SqlxResultExt},
  models::{BuildStep, CreateBuildStep},
};

/// Create a build step record.
///
/// # Errors
///
/// Returns error if database insert fails or step already exists.
pub async fn create(
  pool: &PgPool,
  input: CreateBuildStep,
) -> Result<BuildStep> {
  sqlx::query_as::<_, BuildStep>(
    "INSERT INTO build_steps (build_id, step_number, command) VALUES ($1, $2, \
     $3) RETURNING *",
  )
  .bind(input.build_id)
  .bind(input.step_number)
  .bind(&input.command)
  .fetch_one(pool)
  .await
  .on_unique_violation(|| {
    format!(
      "Build step {} already exists for this build",
      input.step_number
    )
  })
}

/// Mark a build step as completed.
///
/// # Errors
///
/// Returns error if database update fails or step not found.
pub async fn complete(
  pool: &PgPool,
  id: Uuid,
  exit_code: i32,
  output: Option<&str>,
  error_output: Option<&str>,
) -> Result<BuildStep> {
  sqlx::query_as::<_, BuildStep>(
    "UPDATE build_steps SET completed_at = NOW(), exit_code = $1, output = \
     $2, error_output = $3 WHERE id = $4 RETURNING *",
  )
  .bind(exit_code)
  .bind(output)
  .bind(error_output)
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Build step {id} not found")))
}

/// List all build steps for a build.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_build(
  pool: &PgPool,
  build_id: Uuid,
) -> Result<Vec<BuildStep>> {
  Ok(
    sqlx::query_as::<_, BuildStep>(
      "SELECT * FROM build_steps WHERE build_id = $1 ORDER BY step_number ASC",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?,
  )
}
