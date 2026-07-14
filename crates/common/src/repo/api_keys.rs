use circus_codegen::queries::api_keys as q;
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::ApiKey,
  roles::GlobalRole,
};

impl TryFrom<q::ApiKeyRow> for ApiKey {
  type Error = CiError;

  fn try_from(r: q::ApiKeyRow) -> Result<Self> {
    Ok(Self {
      id:           r.id,
      name:         r.name,
      key_hash:     r.key_hash,
      role:         r.role.parse().map_err(CiError::Internal)?,
      user_id:      r.user_id,
      created_at:   r.created_at,
      last_used_at: r.last_used_at,
    })
  }
}

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
  let client = pool.get().await?;
  let row = q::create()
    .bind(&client, &name, &key_hash, &role.as_str())
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict("API key with this hash already exists".to_string())
      } else {
        CiError::Database(e)
      }
    })?;
  ApiKey::try_from(row)
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
  let client = pool.get().await?;
  let row = q::upsert()
    .bind(&client, &name, &key_hash, &role.as_str())
    .one()
    .await?;
  ApiKey::try_from(row)
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
  let client = pool.get().await?;
  q::get_by_hash()
    .bind(&client, &key_hash)
    .opt()
    .await?
    .map(ApiKey::try_from)
    .transpose()
}

/// List all API keys.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list(pool: &PgPool) -> Result<Vec<ApiKey>> {
  let client = pool.get().await?;
  let rows = q::list().bind(&client).all().await?;
  rows.into_iter().map(ApiKey::try_from).collect()
}

/// Delete an API key by ID.
///
/// # Errors
///
/// Returns error if database delete fails or key not found.
pub async fn delete(pool: &PgPool, id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete().bind(&client, &id).await?;
  if affected == 0 {
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
  let client = pool.get().await?;
  q::touch_last_used().bind(&client, &id).await?;
  Ok(())
}
