use circus_codegen::queries::failed_paths_cache as q;
use uuid::Uuid;

use crate::{db::PgPool, error::Result, models::BuildStatus};

/// Check if a derivation path is in the failed paths cache.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn is_cached_failure(pool: &PgPool, drv_path: &str) -> Result<bool> {
  let client = pool.get().await?;
  Ok(
    q::is_cached_failure()
      .bind(&client, &drv_path)
      .opt()
      .await?
      .is_some(),
  )
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
  let client = pool.get().await?;
  q::insert()
    .bind(
      &client,
      &drv_path,
      &Some(source_build_id),
      &Some(failure_status.as_db_str()),
    )
    .await?;
  Ok(())
}

/// Remove a derivation path from the failed paths cache.
///
/// # Errors
///
/// Returns error if database delete fails.
pub async fn invalidate(pool: &PgPool, drv_path: &str) -> Result<()> {
  let client = pool.get().await?;
  q::invalidate().bind(&client, &drv_path).await?;
  Ok(())
}

/// Remove expired entries from the failed paths cache.
///
/// # Errors
///
/// Returns error if database delete fails.
pub async fn cleanup_expired(pool: &PgPool, ttl_seconds: u64) -> Result<u64> {
  let client = pool.get().await?;
  Ok(
    q::cleanup_expired()
      .bind(&client, &(ttl_seconds as f64))
      .await?,
  )
}
