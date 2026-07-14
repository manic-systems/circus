use circus_codegen::queries::remote_builders as q;
use circus_config::{BuilderSchedulingStrategy, DeclarativeRemoteBuilder};
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{CreateRemoteBuilder, RemoteBuilder},
};

impl From<q::RemoteBuilderRow> for RemoteBuilder {
  fn from(r: q::RemoteBuilderRow) -> Self {
    Self {
      id:                   r.id,
      name:                 r.name,
      ssh_uri:              r.ssh_uri,
      systems:              r.systems,
      max_jobs:             r.max_jobs,
      speed_factor:         r.speed_factor,
      supported_features:   r.supported_features,
      mandatory_features:   r.mandatory_features,
      enabled:              r.enabled,
      public_host_key:      r.public_host_key,
      ssh_key_file:         r.ssh_key_file,
      created_at:           r.created_at,
      consecutive_failures: r.consecutive_failures,
      disabled_until:       r.disabled_until,
      last_failure:         r.last_failure,
      cpu_cores:            r.cpu_cores,
    }
  }
}

/// Create a new remote builder.
///
/// # Errors
///
/// Returns error if database insert fails or builder already exists.
pub async fn create(
  pool: &PgPool,
  input: CreateRemoteBuilder,
) -> Result<RemoteBuilder> {
  let client = pool.get().await?;
  let max_jobs = input.max_jobs.unwrap_or(1);
  let speed_factor = input.speed_factor.unwrap_or(1);
  let supported_features = input.supported_features.unwrap_or_default();
  let mandatory_features = input.mandatory_features.unwrap_or_default();
  q::create()
    .bind(
      &client,
      &input.name,
      &input.ssh_uri,
      &input.systems,
      &max_jobs,
      &speed_factor,
      &supported_features,
      &mandatory_features,
      &input.public_host_key,
      &input.ssh_key_file,
    )
    .one()
    .await
    .map(RemoteBuilder::from)
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Remote builder '{}' already exists",
          input.name
        ))
      } else {
        CiError::Database(e)
      }
    })
}

/// Get a remote builder by ID.
///
/// # Errors
///
/// Returns error if database query fails or builder not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<RemoteBuilder> {
  let client = pool.get().await?;
  q::get()
    .bind(&client, &id)
    .opt()
    .await?
    .map(RemoteBuilder::from)
    .ok_or_else(|| CiError::NotFound(format!("Remote builder {id} not found")))
}

/// List all remote builders.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list(pool: &PgPool) -> Result<Vec<RemoteBuilder>> {
  let client = pool.get().await?;
  let rows = q::list().bind(&client).all().await?;
  Ok(rows.into_iter().map(RemoteBuilder::from).collect())
}

/// List all enabled remote builders.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_enabled(pool: &PgPool) -> Result<Vec<RemoteBuilder>> {
  let client = pool.get().await?;
  let rows = q::list_enabled().bind(&client).all().await?;
  Ok(rows.into_iter().map(RemoteBuilder::from).collect())
}

/// Find a suitable builder for the given system.
/// Excludes builders that are temporarily disabled due to consecutive failures.
/// The ordering is determined by the `strategy` parameter.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn find_for_system(
  pool: &PgPool,
  system: &str,
  strategy: &BuilderSchedulingStrategy,
) -> Result<Vec<RemoteBuilder>> {
  let client = pool.get().await?;
  let rows = match strategy {
    BuilderSchedulingStrategy::SpeedFactorOnly => {
      q::find_for_system_speed_factor()
        .bind(&client, &system)
        .all()
        .await?
    },
    BuilderSchedulingStrategy::CpuCoreCountWithSpeedFactor => {
      q::find_for_system_cpu_weighted()
        .bind(&client, &system)
        .all()
        .await?
    },
    BuilderSchedulingStrategy::Dynamic => {
      q::find_for_system_dynamic()
        .bind(&client, &system)
        .all()
        .await?
    },
  };
  Ok(rows.into_iter().map(RemoteBuilder::from).collect())
}

/// Record a build failure for a remote builder.
///
/// Increments `consecutive_failures` (capped at 4), sets `last_failure`,
/// and computes `disabled_until` with exponential backoff.
/// Backoff formula (from Hydra): delta = 60 * 3^(min(failures, 4) - 1) seconds.
///
/// # Errors
///
/// Returns error if database update fails or builder not found.
pub async fn record_failure(pool: &PgPool, id: Uuid) -> Result<RemoteBuilder> {
  let client = pool.get().await?;
  q::record_failure()
    .bind(&client, &id)
    .opt()
    .await?
    .map(RemoteBuilder::from)
    .ok_or_else(|| CiError::NotFound(format!("Remote builder {id} not found")))
}

/// Record a build success for a remote builder.
/// Resets `consecutive_failures` and clears `disabled_until`.
///
/// # Errors
///
/// Returns error if database update fails or builder not found.
pub async fn record_success(pool: &PgPool, id: Uuid) -> Result<RemoteBuilder> {
  let client = pool.get().await?;
  q::record_success()
    .bind(&client, &id)
    .opt()
    .await?
    .map(RemoteBuilder::from)
    .ok_or_else(|| CiError::NotFound(format!("Remote builder {id} not found")))
}

/// Update a remote builder with partial fields.
///
/// # Errors
///
/// Returns error if database update fails or builder not found.
pub async fn update(
  pool: &PgPool,
  id: Uuid,
  input: crate::models::UpdateRemoteBuilder,
) -> Result<RemoteBuilder> {
  // Dynamic update using COALESCE pattern
  let client = pool.get().await?;
  q::update()
    .bind(
      &client,
      &input.name,
      &input.ssh_uri,
      &input.systems,
      &input.max_jobs,
      &input.speed_factor,
      &input.supported_features,
      &input.mandatory_features,
      &input.enabled,
      &input.public_host_key,
      &input.ssh_key_file,
      &id,
    )
    .opt()
    .await?
    .map(RemoteBuilder::from)
    .ok_or_else(|| CiError::NotFound(format!("Remote builder {id} not found")))
}

/// Delete a remote builder.
///
/// # Errors
///
/// Returns error if database delete fails or builder not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete().bind(&client, &id).await?;
  if affected == 0 {
    return Err(CiError::NotFound(format!("Remote builder {id} not found")));
  }
  Ok(())
}

/// Count total remote builders.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count().bind(&client).one().await?)
}

/// Upsert a remote builder (insert or update on conflict by name).
///
/// # Errors
///
/// Returns error if database operation fails.
pub async fn upsert(
  pool: &PgPool,
  params: &crate::models::RemoteBuilderParams<'_>,
) -> Result<RemoteBuilder> {
  let client = pool.get().await?;
  Ok(
    q::upsert()
      .bind(
        &client,
        &params.name,
        &params.ssh_uri,
        &params.systems,
        &params.max_jobs,
        &params.speed_factor,
        &params.supported_features,
        &params.mandatory_features,
        &params.enabled,
        &params.public_host_key,
        &params.ssh_key_file,
      )
      .one()
      .await
      .map(RemoteBuilder::from)?,
  )
}

/// Sync remote builders from declarative config.
/// Deletes builders not in the declarative list and upserts those that are.
///
/// # Errors
///
/// Returns error if database operations fail.
pub async fn sync_all(
  pool: &PgPool,
  builders: &[DeclarativeRemoteBuilder],
) -> Result<()> {
  // Get builder names from declarative config
  let names: Vec<&str> = builders.iter().map(|b| b.name.as_str()).collect();

  // Delete builders not in declarative config
  {
    let client = pool.get().await?;
    q::sync_all_delete().bind(&client, &names).await?;
  }

  // Upsert each builder
  for builder in builders {
    let params = crate::models::RemoteBuilderParams {
      name:               &builder.name,
      ssh_uri:            &builder.ssh_uri,
      systems:            &builder.systems,
      max_jobs:           builder.max_jobs,
      speed_factor:       builder.speed_factor,
      supported_features: &builder.supported_features,
      mandatory_features: &builder.mandatory_features,
      enabled:            builder.enabled,
      public_host_key:    builder.public_host_key.as_deref(),
      ssh_key_file:       builder.ssh_key_file.as_deref(),
    };
    upsert(pool, &params).await?;
  }

  Ok(())
}
