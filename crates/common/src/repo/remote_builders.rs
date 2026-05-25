use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  config::{BuilderSchedulingStrategy, DeclarativeRemoteBuilder},
  error::{CiError, Result},
  models::{CreateRemoteBuilder, RemoteBuilder},
};

/// Create a new remote builder.
///
/// # Errors
///
/// Returns error if database insert fails or builder already exists.
pub async fn create(
  pool: &PgPool,
  input: CreateRemoteBuilder,
) -> Result<RemoteBuilder> {
  sqlx::query_as::<_, RemoteBuilder>(
    "INSERT INTO remote_builders (name, ssh_uri, systems, max_jobs, \
     speed_factor, supported_features, mandatory_features, public_host_key, \
     ssh_key_file) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9) RETURNING *",
  )
  .bind(&input.name)
  .bind(&input.ssh_uri)
  .bind(&input.systems)
  .bind(input.max_jobs.unwrap_or(1))
  .bind(input.speed_factor.unwrap_or(1))
  .bind(input.supported_features.as_deref().unwrap_or(&[]))
  .bind(input.mandatory_features.as_deref().unwrap_or(&[]))
  .bind(&input.public_host_key)
  .bind(&input.ssh_key_file)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Remote builder '{}' already exists",
          input.name
        ))
      },
      _ => CiError::Database(e),
    }
  })
}

/// Get a remote builder by ID.
///
/// # Errors
///
/// Returns error if database query fails or builder not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<RemoteBuilder> {
  sqlx::query_as::<_, RemoteBuilder>(
    "SELECT * FROM remote_builders WHERE id = $1",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Remote builder {id} not found")))
}

/// List all remote builders.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list(pool: &PgPool) -> Result<Vec<RemoteBuilder>> {
  sqlx::query_as::<_, RemoteBuilder>(
    "SELECT * FROM remote_builders ORDER BY speed_factor DESC, name",
  )
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// List all enabled remote builders.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_enabled(pool: &PgPool) -> Result<Vec<RemoteBuilder>> {
  sqlx::query_as::<_, RemoteBuilder>(
    "SELECT * FROM remote_builders WHERE enabled = true ORDER BY speed_factor \
     DESC, name",
  )
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
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
  let query = match strategy {
    BuilderSchedulingStrategy::SpeedFactorOnly => {
      "SELECT * FROM remote_builders WHERE enabled = true AND $1 = \
       ANY(systems) AND (disabled_until IS NULL OR disabled_until < NOW()) \
       ORDER BY speed_factor DESC"
    },
    BuilderSchedulingStrategy::CpuCoreCountWithSpeedFactor => {
      "SELECT * FROM remote_builders WHERE enabled = true AND $1 = \
       ANY(systems) AND (disabled_until IS NULL OR disabled_until < NOW()) \
       ORDER BY COALESCE(cpu_cores, 1) * speed_factor DESC"
    },
    BuilderSchedulingStrategy::Dynamic => {
      "SELECT r.* FROM remote_builders r LEFT JOIN (SELECT builder_id, \
       COUNT(*) AS cnt FROM builds WHERE status = 'running' GROUP BY \
       builder_id) active ON active.builder_id = r.id WHERE r.enabled = true \
       AND $1 = ANY(r.systems) AND (r.disabled_until IS NULL OR \
       r.disabled_until < NOW()) ORDER BY (r.max_jobs - COALESCE(active.cnt, \
       0)) * r.speed_factor DESC"
    },
  };

  sqlx::query_as::<_, RemoteBuilder>(query)
    .bind(system)
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
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
  sqlx::query_as::<_, RemoteBuilder>(
    "UPDATE remote_builders SET consecutive_failures = \
     LEAST(consecutive_failures + 1, 4), last_failure = NOW(), disabled_until \
     = NOW() + make_interval(secs => 60.0 * power(3, \
     LEAST(consecutive_failures + 1, 4) - 1) + (random() * 30)::int ) WHERE \
     id = $1 RETURNING *",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Remote builder {id} not found")))
}

/// Record a build success for a remote builder.
/// Resets `consecutive_failures` and clears `disabled_until`.
///
/// # Errors
///
/// Returns error if database update fails or builder not found.
pub async fn record_success(pool: &PgPool, id: Uuid) -> Result<RemoteBuilder> {
  sqlx::query_as::<_, RemoteBuilder>(
    "UPDATE remote_builders SET consecutive_failures = 0, disabled_until = \
     NULL WHERE id = $1 RETURNING *",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
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
  sqlx::query_as::<_, RemoteBuilder>(
    "UPDATE remote_builders SET name = COALESCE($1, name), ssh_uri = \
     COALESCE($2, ssh_uri), systems = COALESCE($3, systems), max_jobs = \
     COALESCE($4, max_jobs), speed_factor = COALESCE($5, speed_factor), \
     supported_features = COALESCE($6, supported_features), \
     mandatory_features = COALESCE($7, mandatory_features), enabled = \
     COALESCE($8, enabled), public_host_key = COALESCE($9, public_host_key), \
     ssh_key_file = COALESCE($10, ssh_key_file) WHERE id = $11 RETURNING *",
  )
  .bind(&input.name)
  .bind(&input.ssh_uri)
  .bind(&input.systems)
  .bind(input.max_jobs)
  .bind(input.speed_factor)
  .bind(&input.supported_features)
  .bind(&input.mandatory_features)
  .bind(input.enabled)
  .bind(&input.public_host_key)
  .bind(&input.ssh_key_file)
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Remote builder {id} not found")))
}

/// Delete a remote builder.
///
/// # Errors
///
/// Returns error if database delete fails or builder not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM remote_builders WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await
    .map_err(CiError::Database)?;
  if result.rows_affected() == 0 {
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
  let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM remote_builders")
    .fetch_one(pool)
    .await
    .map_err(CiError::Database)?;
  Ok(row.0)
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
  sqlx::query_as::<_, RemoteBuilder>(
    "INSERT INTO remote_builders (name, ssh_uri, systems, max_jobs, \
     speed_factor, supported_features, mandatory_features, enabled, \
     public_host_key, ssh_key_file) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, \
     $9, $10) ON CONFLICT (name) DO UPDATE SET ssh_uri = EXCLUDED.ssh_uri, \
     systems = EXCLUDED.systems, max_jobs = EXCLUDED.max_jobs, speed_factor = \
     EXCLUDED.speed_factor, supported_features = EXCLUDED.supported_features, \
     mandatory_features = EXCLUDED.mandatory_features, enabled = \
     EXCLUDED.enabled, public_host_key = COALESCE(EXCLUDED.public_host_key, \
     remote_builders.public_host_key), ssh_key_file = \
     COALESCE(EXCLUDED.ssh_key_file, remote_builders.ssh_key_file) RETURNING *",
  )
  .bind(params.name)
  .bind(params.ssh_uri)
  .bind(params.systems)
  .bind(params.max_jobs)
  .bind(params.speed_factor)
  .bind(params.supported_features)
  .bind(params.mandatory_features)
  .bind(params.enabled)
  .bind(params.public_host_key)
  .bind(params.ssh_key_file)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)
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
  sqlx::query("DELETE FROM remote_builders WHERE name != ALL($1::text[])")
    .bind(&names)
    .execute(pool)
    .await
    .map_err(CiError::Database)?;

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
