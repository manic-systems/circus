use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{Result, SqlxResultExt},
  models::BuildOutput,
};

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
  sqlx::query_as::<_, BuildOutput>(
    "INSERT INTO build_outputs (build, name, path) VALUES ($1, $2, $3) \
     RETURNING *",
  )
  .bind(build)
  .bind(name)
  .bind(path)
  .fetch_one(pool)
  .await
  .on_unique_violation(|| {
    format!("Build output with name '{name}' already exists for build {build}")
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
  Ok(
    sqlx::query_as::<_, BuildOutput>(
      "SELECT * FROM build_outputs WHERE build = $1 ORDER BY name ASC",
    )
    .bind(build)
    .fetch_all(pool)
    .await?,
  )
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
  Ok(
    sqlx::query_as::<_, BuildOutput>(
      "SELECT * FROM build_outputs WHERE path = $1 ORDER BY build, name",
    )
    .bind(path)
    .fetch_all(pool)
    .await?,
  )
}

/// Delete all build outputs for a build.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn delete_for_build(pool: &PgPool, build: Uuid) -> Result<u64> {
  let result = sqlx::query("DELETE FROM build_outputs WHERE build = $1")
    .bind(build)
    .execute(pool)
    .await?;

  Ok(result.rows_affected())
}
