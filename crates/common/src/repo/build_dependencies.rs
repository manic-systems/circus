use circus_codegen::queries::build_dependencies as q;
use uuid::Uuid;

use crate::{
  Build,
  db::{DbTransaction, GenericClient, PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{BuildDependency, BuildStatus},
};

impl From<q::BuildDependencyRow> for BuildDependency {
  fn from(r: q::BuildDependencyRow) -> Self {
    Self {
      id:                  r.id,
      build_id:            r.build_id,
      dependency_build_id: r.dependency_build_id,
    }
  }
}

impl TryFrom<q::BuildRow> for Build {
  type Error = CiError;

  fn try_from(r: q::BuildRow) -> Result<Self> {
    let status = r.status.parse::<BuildStatus>().map_err(|e| {
      CiError::Internal(format!("build {} in the database has {e}", r.id))
    })?;
    Ok(Self {
      id: r.id,
      evaluation_id: r.evaluation_id,
      job_name: r.job_name,
      drv_path: r.drv_path,
      status,
      started_at: r.started_at,
      completed_at: r.completed_at,
      log_path: r.log_path,
      build_output_path: r.build_output_path,
      error_message: r.error_message,
      system: r.system,
      priority: r.priority,
      retry_count: r.retry_count,
      max_retries: r.max_retries,
      notification_pending_since: r.notification_pending_since,
      created_at: r.created_at,
      outputs: r.outputs,
      is_aggregate: r.is_aggregate,
      constituents: r.constituents,
      builder_id: r.builder_id,
      agent_machine_id: r.agent_machine_id,
      signed: r.signed,
      keep: r.keep,
      is_fod: r.is_fod,
      fod_hash: r.fod_hash,
      meta_description: r.meta_description,
      meta_license: r.meta_license,
      meta_homepage: r.meta_homepage,
      meta_maintainers: r.meta_maintainers,
      required_features: r.required_features,
      effective_features: r.effective_features,
    })
  }
}

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
  let client = pool.get().await?;
  create_with(&client, build_id, dependency_build_id).await
}

/// Create a build dependency relationship within an existing transaction.
///
/// # Errors
///
/// Returns an error if database insert fails or dependency already exists.
pub async fn create_in_transaction(
  tx: &DbTransaction<'_>,
  build_id: Uuid,
  dependency_build_id: Uuid,
) -> Result<BuildDependency> {
  create_with(tx, build_id, dependency_build_id).await
}

async fn create_with<C: GenericClient>(
  client: &C,
  build_id: Uuid,
  dependency_build_id: Uuid,
) -> Result<BuildDependency> {
  q::create()
    .bind(client, &build_id, &dependency_build_id)
    .one()
    .await
    .map(BuildDependency::from)
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Dependency from {build_id} to {dependency_build_id} already exists"
        ))
      } else {
        CiError::Database(e)
      }
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
  let client = pool.get().await?;
  let rows = q::list_for_build().bind(&client, &build_id).all().await?;
  Ok(rows.into_iter().map(BuildDependency::from).collect())
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
  let client = pool.get().await?;
  let rows = q::list_dependency_builds()
    .bind(&client, &build_id)
    .all()
    .await?;
  rows.into_iter().map(Build::try_from).collect()
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
  let client = pool.get().await?;
  let rows = q::list_dependent_builds()
    .bind(&client, &build_id)
    .all()
    .await?;
  rows.into_iter().map(Build::try_from).collect()
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

  let client = pool.get().await?;
  // Find build_ids that have incomplete deps
  let rows = q::check_deps_for_builds()
    .bind(&client, &build_ids)
    .all()
    .await?;

  let incomplete: std::collections::HashSet<Uuid> = rows.into_iter().collect();

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
  let client = pool.get().await?;
  let count = q::all_deps_completed()
    .bind(&client, &build_id)
    .one()
    .await?;
  Ok(count == 0)
}
