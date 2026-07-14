use circus_codegen::queries::build_outputs as q;
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::BuildOutput,
};

impl From<q::BuildOutputRow> for BuildOutput {
  fn from(r: q::BuildOutputRow) -> Self {
    Self {
      build: r.build,
      name:  r.name,
      path:  r.path,
    }
  }
}

/// Create a build output record.
///
/// # Errors
///
/// Returns error if database insert fails or if a duplicate (build, name) pair
/// exists.
pub async fn create(
  pool: &PgPool,
  build: Uuid,
  name: &str,
  path: Option<&str>,
) -> Result<BuildOutput> {
  let client = pool.get().await?;
  q::create()
    .bind(&client, &build, &name, &path)
    .one()
    .await
    .map(BuildOutput::from)
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Build output with name '{name}' already exists for build {build}"
        ))
      } else {
        CiError::Database(e)
      }
    })
}

/// List all build outputs for a build, ordered by name.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_build(
  pool: &PgPool,
  build: Uuid,
) -> Result<Vec<BuildOutput>> {
  let client = pool.get().await?;
  let rows = q::list_for_build().bind(&client, &build).all().await?;
  Ok(rows.into_iter().map(BuildOutput::from).collect())
}

/// Find build outputs by path.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn find_by_path(
  pool: &PgPool,
  path: &str,
) -> Result<Vec<BuildOutput>> {
  let client = pool.get().await?;
  let rows = q::find_by_path().bind(&client, &path).all().await?;
  Ok(rows.into_iter().map(BuildOutput::from).collect())
}

/// Delete all build outputs for a build.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn delete_for_build(pool: &PgPool, build: Uuid) -> Result<u64> {
  let client = pool.get().await?;
  Ok(q::delete_for_build().bind(&client, &build).await?)
}
