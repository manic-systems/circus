use circus_codegen::queries::build_steps as q;
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{BuildStep, CreateBuildStep},
};

impl From<q::BuildStepRow> for BuildStep {
  fn from(r: q::BuildStepRow) -> Self {
    Self {
      id:           r.id,
      build_id:     r.build_id,
      step_number:  r.step_number,
      command:      r.command,
      output:       r.output,
      error_output: r.error_output,
      started_at:   r.started_at,
      completed_at: r.completed_at,
      exit_code:    r.exit_code,
    }
  }
}

/// Create a build step record.
///
/// # Errors
///
/// Returns error if database insert fails or step already exists.
pub async fn create(
  pool: &PgPool,
  input: CreateBuildStep,
) -> Result<BuildStep> {
  let client = pool.get().await?;
  q::create()
    .bind(&client, &input.build_id, &input.step_number, &input.command)
    .one()
    .await
    .map(BuildStep::from)
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Build step {} already exists for this build",
          input.step_number
        ))
      } else {
        CiError::Database(e)
      }
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
  let client = pool.get().await?;
  q::complete()
    .bind(&client, &exit_code, &output, &error_output, &id)
    .opt()
    .await?
    .map(BuildStep::from)
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
  let client = pool.get().await?;
  let rows = q::list_for_build().bind(&client, &build_id).all().await?;
  Ok(rows.into_iter().map(BuildStep::from).collect())
}

/// Delete all steps for a build, clearing stale steps from a prior
/// requeued attempt.
///
/// # Errors
///
/// Returns error if database delete fails.
pub async fn delete_for_build(pool: &PgPool, build_id: Uuid) -> Result<u64> {
  let client = pool.get().await?;
  Ok(q::delete_for_build().bind(&client, &build_id).await?)
}
