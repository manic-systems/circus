use circus_codegen::queries::jobset_inputs as q;
use circus_config::DeclarativeJobsetInput;
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{InputType, JobsetInput},
};

impl TryFrom<q::JobsetInputRow> for JobsetInput {
  type Error = CiError;

  fn try_from(r: q::JobsetInputRow) -> Result<Self> {
    let input_type = r.input_type.parse().map_err(CiError::Internal)?;
    Ok(Self {
      id: r.id,
      jobset_id: r.jobset_id,
      name: r.name,
      input_type,
      value: r.value,
      revision: r.revision,
      created_at: r.created_at,
    })
  }
}

/// Create a new jobset input.
///
/// # Errors
///
/// Returns error if database insert fails or input already exists.
pub async fn create(
  pool: &PgPool,
  jobset_id: Uuid,
  name: &str,
  input_type: InputType,
  value: &str,
  revision: Option<&str>,
) -> Result<JobsetInput> {
  circus_nix::validate::validate_jobset_input(
    name, input_type, value, revision,
  )
  .map_err(CiError::Validation)?;
  let client = pool.get().await?;
  q::create()
    .bind(
      &client,
      &jobset_id,
      &name,
      &input_type.as_str(),
      &value,
      &revision,
    )
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Input '{name}' already exists in this jobset"
        ))
      } else {
        CiError::Database(e)
      }
    })?
    .try_into()
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
  let client = pool.get().await?;
  let rows = q::list_for_jobset().bind(&client, &jobset_id).all().await?;
  rows.into_iter().map(JobsetInput::try_from).collect()
}

/// Delete a jobset input.
///
/// # Errors
///
/// Returns error if database delete fails or input not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete().bind(&client, &id).await?;
  if affected == 0 {
    return Err(CiError::NotFound(format!("Jobset input {id} not found")));
  }
  Ok(())
}

/// Delete a jobset input belonging to the specified jobset.
///
/// # Errors
///
/// Returns error if the database delete fails or the input does not belong to
/// the jobset.
pub async fn delete_for_jobset(
  pool: &PgPool,
  jobset_id: Uuid,
  id: Uuid,
) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete_for_jobset()
    .bind(&client, &id, &jobset_id)
    .await?;
  if affected == 0 {
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
  input_type: InputType,
  value: &str,
  revision: Option<&str>,
) -> Result<JobsetInput> {
  circus_nix::validate::validate_jobset_input(
    name, input_type, value, revision,
  )
  .map_err(CiError::Validation)?;
  let client = pool.get().await?;
  q::upsert()
    .bind(
      &client,
      &jobset_id,
      &name,
      &input_type.as_str(),
      &value,
      &revision,
    )
    .one()
    .await?
    .try_into()
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
  {
    let client = pool.get().await?;
    q::sync_for_jobset_delete()
      .bind(&client, &jobset_id, &names)
      .await?;
  }

  // Upsert each input
  for input in inputs {
    upsert(
      pool,
      jobset_id,
      &input.name,
      input.input_type,
      &input.value,
      input.revision.as_deref(),
    )
    .await?;
  }

  Ok(())
}
