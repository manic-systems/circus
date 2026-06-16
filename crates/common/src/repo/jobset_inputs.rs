use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  config::DeclarativeJobsetInput,
  error::{CiError, Result},
  models::JobsetInput,
  nix,
};

/// Create a new jobset input.
///
/// # Errors
///
/// Returns error if database insert fails or input already exists.
pub async fn create(
  pool: &PgPool,
  jobset_id: Uuid,
  name: &str,
  input_type: &str,
  value: &str,
  revision: Option<&str>,
) -> Result<JobsetInput> {
  nix::validate::validate_jobset_input(name, input_type, value, revision)
    .map_err(CiError::Validation)?;
  sqlx::query_as::<_, JobsetInput>(
    "INSERT INTO jobset_inputs (jobset_id, name, input_type, value, revision) \
     VALUES ($1, $2, $3, $4, $5) RETURNING *",
  )
  .bind(jobset_id)
  .bind(name)
  .bind(input_type)
  .bind(value)
  .bind(revision)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Input '{name}' already exists in this jobset"
        ))
      },
      _ => CiError::Database(e),
    }
  })
}

/// List all inputs for a jobset.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_jobset(
  pool: &PgPool,
  jobset_id: Uuid,
) -> Result<Vec<JobsetInput>> {
  sqlx::query_as::<_, JobsetInput>(
    "SELECT * FROM jobset_inputs WHERE jobset_id = $1 ORDER BY name ASC",
  )
  .bind(jobset_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Delete a jobset input.
///
/// # Errors
///
/// Returns error if database delete fails or input not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM jobset_inputs WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("Jobset input {id} not found")));
  }
  Ok(())
}

/// Upsert a jobset input (insert or update on conflict).
///
/// # Errors
///
/// Returns error if database operation fails.
pub async fn upsert(
  pool: &PgPool,
  jobset_id: Uuid,
  name: &str,
  input_type: &str,
  value: &str,
  revision: Option<&str>,
) -> Result<JobsetInput> {
  nix::validate::validate_jobset_input(name, input_type, value, revision)
    .map_err(CiError::Validation)?;
  sqlx::query_as::<_, JobsetInput>(
    "INSERT INTO jobset_inputs (jobset_id, name, input_type, value, revision) \
     VALUES ($1, $2, $3, $4, $5) ON CONFLICT (jobset_id, name) DO UPDATE SET \
     input_type = EXCLUDED.input_type, value = EXCLUDED.value, revision = \
     EXCLUDED.revision RETURNING *",
  )
  .bind(jobset_id)
  .bind(name)
  .bind(input_type)
  .bind(value)
  .bind(revision)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)
}

/// Sync jobset inputs from declarative config.
/// Deletes inputs not in the config and upserts those that are.
///
/// # Errors
///
/// Returns error if database operations fail.
pub async fn sync_for_jobset(
  pool: &PgPool,
  jobset_id: Uuid,
  inputs: &[DeclarativeJobsetInput],
) -> Result<()> {
  // Get names from declarative config
  let names: Vec<&str> = inputs.iter().map(|i| i.name.as_str()).collect();

  // Delete inputs not in declarative config
  sqlx::query(
    "DELETE FROM jobset_inputs WHERE jobset_id = $1 AND name != \
     ALL($2::text[])",
  )
  .bind(jobset_id)
  .bind(&names)
  .execute(pool)
  .await
  .map_err(CiError::Database)?;

  // Upsert each input
  for input in inputs {
    upsert(
      pool,
      jobset_id,
      &input.name,
      &input.input_type,
      &input.value,
      input.revision.as_deref(),
    )
    .await?;
  }

  Ok(())
}
