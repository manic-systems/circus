use circus_codegen::queries::projects as q;
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{BinaryCacheUpstreams, CreateProject, Project, UpdateProject},
  validate::Validate,
};

impl TryFrom<q::ProjectRow> for Project {
  type Error = CiError;

  fn try_from(r: q::ProjectRow) -> Result<Self> {
    Ok(Self {
      id:                    r.id,
      name:                  r.name,
      description:           r.description,
      repository_url:        r.repository_url,
      cache_enabled:         r.cache_enabled,
      cache_url:             r.cache_url,
      cache_upstreams:       serde_json::from_value(r.cache_upstreams)?,
      managed_declaratively: r.managed_declaratively,
      created_at:            r.created_at,
      updated_at:            r.updated_at,
    })
  }
}

fn upstreams_to_value(
  upstreams: &BinaryCacheUpstreams,
) -> Result<serde_json::Value> {
  Ok(serde_json::to_value(upstreams)?)
}

/// Create a new project.
///
/// # Errors
///
/// Returns error if database insert fails or project name already exists.
pub async fn create(pool: &PgPool, input: CreateProject) -> Result<Project> {
  input.validate().map_err(CiError::Validation)?;
  let cache_upstreams = upstreams_to_value(&input.cache_upstreams)?;
  let client = pool.get().await?;
  let row = q::create()
    .bind(
      &client,
      &input.name,
      &input.description,
      &input.repository_url,
      &input.cache_enabled,
      &input.cache_url,
      &cache_upstreams,
    )
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!("Project '{}' already exists", input.name))
      } else {
        CiError::Database(e)
      }
    })?;
  Project::try_from(row)
}

/// Get a project by ID.
///
/// # Errors
///
/// Returns error if database query fails or project not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<Project> {
  let client = pool.get().await?;
  q::get()
    .bind(&client, &id)
    .opt()
    .await?
    .map(Project::try_from)
    .transpose()?
    .ok_or_else(|| CiError::NotFound(format!("Project {id} not found")))
}

/// Get a project by name.
///
/// # Errors
///
/// Returns error if database query fails or project not found.
pub async fn get_by_name(pool: &PgPool, name: &str) -> Result<Project> {
  let client = pool.get().await?;
  q::get_by_name()
    .bind(&client, &name)
    .opt()
    .await?
    .map(Project::try_from)
    .transpose()?
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
  let client = pool.get().await?;
  let rows = q::list().bind(&client, &limit, &offset).all().await?;
  rows.into_iter().map(Project::try_from).collect()
}

/// Count total number of projects.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count().bind(&client).one().await?)
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
  // Read-modify-write so omitted fields keep their existing value
  let existing = get(pool, id).await?;

  let name = input.name.unwrap_or(existing.name);
  let description = input.description.or(existing.description);
  let repository_url = input.repository_url.unwrap_or(existing.repository_url);
  let cache_enabled = input.cache_enabled.unwrap_or(existing.cache_enabled);
  let cache_url = input.cache_url.or(existing.cache_url);
  let cache_upstreams =
    input.cache_upstreams.unwrap_or(existing.cache_upstreams);
  let cache_upstreams = upstreams_to_value(&cache_upstreams)?;

  let client = pool.get().await?;
  let row = q::update()
    .bind(
      &client,
      &name,
      &description,
      &repository_url,
      &cache_enabled,
      &cache_url,
      &cache_upstreams,
      &id,
    )
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!("Project '{name}' already exists"))
      } else {
        CiError::Database(e)
      }
    })?;
  Project::try_from(row)
}

/// Insert or update a project by name.
///
/// # Errors
///
/// Returns error if database operation fails.
pub async fn upsert(pool: &PgPool, input: CreateProject) -> Result<Project> {
  input.validate().map_err(CiError::Validation)?;
  let cache_upstreams = upstreams_to_value(&input.cache_upstreams)?;
  let client = pool.get().await?;
  let row = q::upsert()
    .bind(
      &client,
      &input.name,
      &input.description,
      &input.repository_url,
      &input.cache_enabled,
      &input.cache_url,
      &cache_upstreams,
    )
    .one()
    .await?;
  Project::try_from(row)
}

/// Insert or update a project managed by declarative configuration.
///
/// # Errors
///
/// Returns error if validation or the database operation fails.
pub async fn upsert_declarative(
  pool: &PgPool,
  input: CreateProject,
) -> Result<Project> {
  input.validate().map_err(CiError::Validation)?;
  let cache_upstreams = upstreams_to_value(&input.cache_upstreams)?;
  let client = pool.get().await?;
  let row = q::upsert_declarative()
    .bind(
      &client,
      &input.name,
      &input.description,
      &input.repository_url,
      &input.cache_enabled,
      &input.cache_url,
      &cache_upstreams,
    )
    .one()
    .await?;
  Project::try_from(row)
}

/// Delete declaratively managed projects not present in `names`.
///
/// # Errors
///
/// Returns error if the database operation fails.
pub async fn delete_declarative_except(
  pool: &PgPool,
  names: &[&str],
) -> Result<u64> {
  let client = pool.get().await?;
  Ok(q::delete_declarative_except().bind(&client, &names).await?)
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
  let client = pool.get().await?;
  let rows = q::list_without_active_jobsets().bind(&client).all().await?;
  rows.into_iter().map(Project::try_from).collect()
}

/// Delete a project by ID.
///
/// # Errors
///
/// Returns error if database delete fails or project not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete().bind(&client, &id).await?;
  if affected == 0 {
    return Err(CiError::NotFound(format!("Project {id} not found")));
  }
  Ok(())
}
