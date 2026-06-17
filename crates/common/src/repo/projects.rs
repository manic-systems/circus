use sqlx::{PgPool, types::Json};
use uuid::Uuid;

use crate::{
  error::{CiError, Result, SqlxResultExt},
  models::{CreateProject, Project, UpdateProject},
  validate::Validate,
};

/// Create a new project.
///
/// # Errors
///
/// Returns error if database insert fails or project name already exists.
pub async fn create(pool: &PgPool, input: CreateProject) -> Result<Project> {
  input.validate().map_err(CiError::Validation)?;
  sqlx::query_as::<_, Project>(
    "INSERT INTO projects (name, description, repository_url, cache_enabled, \
     cache_url, cache_upstreams) VALUES ($1, $2, $3, $4, $5, $6) RETURNING *",
  )
  .bind(&input.name)
  .bind(&input.description)
  .bind(&input.repository_url)
  .bind(input.cache_enabled)
  .bind(&input.cache_url)
  .bind(Json(input.cache_upstreams))
  .fetch_one(pool)
  .await
  .on_unique_violation(|| format!("Project '{}' already exists", input.name))
}

/// Get a project by ID.
///
/// # Errors
///
/// Returns error if database query fails or project not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Project> {
  sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE id = $1")
    .bind(id)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Project {id} not found")))
}

/// Get a project by name.
///
/// # Errors
///
/// Returns error if database query fails or project not found.
pub async fn get_by_name(pool: &PgPool, name: &str) -> Result<Project> {
  sqlx::query_as::<_, Project>("SELECT * FROM projects WHERE name = $1")
    .bind(name)
    .fetch_optional(pool)
    .await?
    .ok_or_else(|| CiError::NotFound(format!("Project '{name}' not found")))
}

/// List projects with pagination.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list(
  pool: &PgPool,
  limit: i64,
  offset: i64,
) -> Result<Vec<Project>> {
  Ok(
    sqlx::query_as::<_, Project>(
      "SELECT * FROM projects ORDER BY created_at DESC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?,
  )
}

/// Count total number of projects.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM projects")
    .fetch_one(pool)
    .await?;
  Ok(row.0)
}

/// Update a project with partial fields.
///
/// # Errors
///
/// Returns error if database update fails or project not found.
pub async fn update(
  pool: &PgPool,
  id: Uuid,
  input: UpdateProject,
) -> Result<Project> {
  input.validate().map_err(CiError::Validation)?;
  // Dynamic update - only set provided fields
  let existing = get(pool, id).await?;

  let name = input.name.unwrap_or(existing.name);
  let description = input.description.or(existing.description);
  let repository_url = input.repository_url.unwrap_or(existing.repository_url);
  let cache_enabled = input.cache_enabled.unwrap_or(existing.cache_enabled);
  let cache_url = input.cache_url.or(existing.cache_url);
  let cache_upstreams =
    input.cache_upstreams.unwrap_or(existing.cache_upstreams.0);

  sqlx::query_as::<_, Project>(
    "UPDATE projects SET name = $1, description = $2, repository_url = $3, \
     cache_enabled = $4, cache_url = $5, cache_upstreams = $6 WHERE id = $7 \
     RETURNING *",
  )
  .bind(&name)
  .bind(&description)
  .bind(&repository_url)
  .bind(cache_enabled)
  .bind(&cache_url)
  .bind(Json(cache_upstreams))
  .bind(id)
  .fetch_one(pool)
  .await
  .on_unique_violation(|| format!("Project '{name}' already exists"))
}

/// Insert or update a project by name.
///
/// # Errors
///
/// Returns error if database operation fails.
pub async fn upsert(pool: &PgPool, input: CreateProject) -> Result<Project> {
  input.validate().map_err(CiError::Validation)?;
  Ok(
    sqlx::query_as::<_, Project>(
      "INSERT INTO projects (name, description, repository_url, \
       cache_enabled, cache_url, cache_upstreams) VALUES ($1, $2, $3, $4, $5, \
       $6) ON CONFLICT (name) DO UPDATE SET description = \
       EXCLUDED.description, repository_url = EXCLUDED.repository_url, \
       cache_enabled = EXCLUDED.cache_enabled, cache_url = \
       EXCLUDED.cache_url, cache_upstreams = EXCLUDED.cache_upstreams \
       RETURNING *",
    )
    .bind(&input.name)
    .bind(&input.description)
    .bind(&input.repository_url)
    .bind(input.cache_enabled)
    .bind(&input.cache_url)
    .bind(Json(input.cache_upstreams))
    .fetch_one(pool)
    .await?,
  )
}

/// List projects that have no active jobsets.
///
/// Used by the evaluator to discover in-repo declarative config for projects
/// that have not yet bootstrapped any jobsets through the server config.
///
/// # Returns
///
/// Projects that have NO jobsets at all. A project with only disabled
/// jobsets is considered intentionally configured and is not re-discovered,
/// honoring the user's choice to disable evaluation without re-cloning.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_without_active_jobsets(
  pool: &PgPool,
) -> Result<Vec<Project>> {
  Ok(
    sqlx::query_as::<_, Project>(
      "SELECT p.* FROM projects p WHERE NOT EXISTS (SELECT 1 FROM jobsets j \
       WHERE j.project_id = p.id)",
    )
    .fetch_all(pool)
    .await?,
  )
}

/// Delete a project by ID.
///
/// # Errors
///
/// Returns error if database delete fails or project not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM projects WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;

  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("Project {id} not found")));
  }

  Ok(())
}
