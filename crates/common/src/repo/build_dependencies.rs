use sqlx::{Executor, PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::{
  Build,
  error::{Result, SqlxResultExt},
  models::BuildDependency,
};

/// Create a build dependency relationship.
///
/// # Errors
///
/// Returns error if database insert fails or dependency already exists.
pub async fn create(
  pool: &PgPool,
  build_id: Uuid,
  dependency_build_id: Uuid,
) -> Result<BuildDependency> {
  create_with(pool, build_id, dependency_build_id).await
}

/// Create a build dependency relationship within an existing transaction.
///
/// # Errors
///
/// Returns an error if database insert fails or dependency already exists.
pub async fn create_in_transaction(
  tx: &mut Transaction<'_, Postgres>,
  build_id: Uuid,
  dependency_build_id: Uuid,
) -> Result<BuildDependency> {
  create_with(&mut **tx, build_id, dependency_build_id).await
}

async fn create_with<'e, E>(
  executor: E,
  build_id: Uuid,
  dependency_build_id: Uuid,
) -> Result<BuildDependency>
where
  E: Executor<'e, Database = Postgres>,
{
  sqlx::query_as::<_, BuildDependency>(
    "INSERT INTO build_dependencies (build_id, dependency_build_id) VALUES \
     ($1, $2) RETURNING *",
  )
  .bind(build_id)
  .bind(dependency_build_id)
  .fetch_one(executor)
  .await
  .on_unique_violation(|| {
    format!(
      "Dependency from {build_id} to {dependency_build_id} already exists"
    )
  })
}

/// List all dependencies for a build.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_build(
  pool: &PgPool,
  build_id: Uuid,
) -> Result<Vec<BuildDependency>> {
  Ok(
    sqlx::query_as::<_, BuildDependency>(
      "SELECT * FROM build_dependencies WHERE build_id = $1",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?,
  )
}

/// List the build records that a build depends on.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_dependency_builds(
  pool: &PgPool,
  build_id: Uuid,
) -> Result<Vec<Build>> {
  Ok(
    sqlx::query_as::<_, Build>(
      "SELECT b.* FROM build_dependencies bd JOIN builds b ON b.id = \
       bd.dependency_build_id WHERE bd.build_id = $1 ORDER BY b.job_name",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?,
  )
}

/// List build records that depend on the given build.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_dependent_builds(
  pool: &PgPool,
  build_id: Uuid,
) -> Result<Vec<Build>> {
  Ok(
    sqlx::query_as::<_, Build>(
      "SELECT b.* FROM build_dependencies bd JOIN builds b ON b.id = \
       bd.build_id WHERE bd.dependency_build_id = $1 ORDER BY b.job_name",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?,
  )
}

/// Batch check if all dependency builds are completed for multiple builds at
/// once. Returns a map from `build_id` to whether all deps are completed.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn check_deps_for_builds(
  pool: &PgPool,
  build_ids: &[Uuid],
) -> Result<std::collections::HashMap<Uuid, bool>> {
  if build_ids.is_empty() {
    return Ok(std::collections::HashMap::new());
  }

  // Find build_ids that have incomplete deps
  let rows: Vec<(Uuid,)> = sqlx::query_as(
    "SELECT DISTINCT bd.build_id FROM build_dependencies bd JOIN builds b ON \
     bd.dependency_build_id = b.id WHERE bd.build_id = ANY($1) AND b.status \
     != 'succeeded'",
  )
  .bind(build_ids)
  .fetch_all(pool)
  .await?;

  let incomplete: std::collections::HashSet<Uuid> =
    rows.into_iter().map(|(id,)| id).collect();

  Ok(
    build_ids
      .iter()
      .map(|id| (*id, !incomplete.contains(id)))
      .collect(),
  )
}

/// Check if all dependency builds for a given build are completed.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn all_deps_completed(pool: &PgPool, build_id: Uuid) -> Result<bool> {
  let row: (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM build_dependencies bd JOIN builds b ON \
     bd.dependency_build_id = b.id WHERE bd.build_id = $1 AND b.status != \
     'succeeded'",
  )
  .bind(build_id)
  .fetch_one(pool)
  .await?;

  Ok(row.0 == 0)
}
