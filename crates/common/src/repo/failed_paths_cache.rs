use sqlx::PgPool;
use uuid::Uuid;

use crate::{error::Result, models::BuildStatus};

/// Check if a derivation path is in the failed paths cache.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn is_cached_failure(pool: &PgPool, drv_path: &str) -> Result<bool> {
  let row: Option<(bool,)> =
    sqlx::query_as("SELECT true FROM failed_paths_cache WHERE drv_path = $1")
      .bind(drv_path)
      .fetch_optional(pool)
      .await?;

  Ok(row.is_some())
}

/// Insert a failed derivation path into the cache.
///
/// # Errors
///
/// Returns error if database insert fails.
pub async fn insert(
  pool: &PgPool,
  drv_path: &str,
  failure_status: BuildStatus,
  source_build_id: Uuid,
) -> Result<()> {
  let status_str = failure_status.to_string();
  sqlx::query(
    "INSERT INTO failed_paths_cache (drv_path, source_build_id, \
     failure_status, failed_at) VALUES ($1, $2, $3, NOW()) ON CONFLICT \
     (drv_path) DO UPDATE SET source_build_id = $2, failure_status = $3, \
     failed_at = NOW()",
  )
  .bind(drv_path)
  .bind(source_build_id)
  .bind(&status_str)
  .execute(pool)
  .await?;

  Ok(())
}

/// Remove a derivation path from the failed paths cache.
///
/// # Errors
///
/// Returns error if database delete fails.
pub async fn invalidate(pool: &PgPool, drv_path: &str) -> Result<()> {
  sqlx::query("DELETE FROM failed_paths_cache WHERE drv_path = $1")
    .bind(drv_path)
    .execute(pool)
    .await?;

  Ok(())
}

/// Remove expired entries from the failed paths cache.
///
/// # Errors
///
/// Returns error if database delete fails.
pub async fn cleanup_expired(pool: &PgPool, ttl_seconds: u64) -> Result<u64> {
  let result = sqlx::query(
    "DELETE FROM failed_paths_cache WHERE failed_at < NOW() - \
     make_interval(secs => $1)",
  )
  .bind(ttl_seconds as f64)
  .execute(pool)
  .await?;

  Ok(result.rows_affected())
}
