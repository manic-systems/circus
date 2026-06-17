use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::ApiKey,
  roles::GlobalRole,
};

/// Create a new API key.
///
/// # Errors
///
/// Returns error if database insert fails or key already exists.
pub async fn create(
  pool: &PgPool,
  name: &str,
  key_hash: &str,
  role: GlobalRole,
) -> Result<ApiKey> {
  sqlx::query_as::<_, ApiKey>(
    "INSERT INTO api_keys (name, key_hash, role) VALUES ($1, $2, $3) \
     RETURNING *",
  )
  .bind(name)
  .bind(key_hash)
  .bind(role)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict("API key with this hash already exists".to_string())
      },
      _ => CiError::Database(e),
    }
  })
}

/// Insert or update an API key by hash.
///
/// # Errors
///
/// Returns error if database operation fails.
pub async fn upsert(
  pool: &PgPool,
  name: &str,
  key_hash: &str,
  role: GlobalRole,
) -> Result<ApiKey> {
  sqlx::query_as::<_, ApiKey>(
    "INSERT INTO api_keys (name, key_hash, role) VALUES ($1, $2, $3) ON \
     CONFLICT (key_hash) DO UPDATE SET name = EXCLUDED.name, role = \
     EXCLUDED.role RETURNING *",
  )
  .bind(name)
  .bind(key_hash)
  .bind(role)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)
}

/// Find an API key by its hash.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get_by_hash(
  pool: &PgPool,
  key_hash: &str,
) -> Result<Option<ApiKey>> {
  sqlx::query_as::<_, ApiKey>("SELECT * FROM api_keys WHERE key_hash = $1")
    .bind(key_hash)
    .fetch_optional(pool)
    .await
    .map_err(CiError::Database)
}

/// List all API keys.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list(pool: &PgPool) -> Result<Vec<ApiKey>> {
  sqlx::query_as::<_, ApiKey>("SELECT * FROM api_keys ORDER BY created_at DESC")
    .fetch_all(pool)
    .await
    .map_err(CiError::Database)
}

/// Delete an API key by ID.
///
/// # Errors
///
/// Returns error if database delete fails or key not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let result = sqlx::query("DELETE FROM api_keys WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  if result.rows_affected() == 0 {
    return Err(CiError::NotFound(format!("API key {id} not found")));
  }
  Ok(())
}

/// Update the `last_used_at` timestamp for an API key.
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn touch_last_used(pool: &PgPool, id: Uuid) -> Result<()> {
  sqlx::query("UPDATE api_keys SET last_used_at = NOW() WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await
    .map_err(CiError::Database)?;
  Ok(())
}
