//! Read/write of the `narinfo_cache` table.
//!
//! The runner's RPC server upserts a row here every time an agent
//! reports a successful presigned-NAR upload via
//! `Runner.notifyUploadComplete`. The server's cache route reads from
//! here when answering `<hash>.narinfo` queries, so a path uploaded by
//! any agent in the cluster is immediately visible to substituters.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};

use crate::error::{CiError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NarInfo {
  pub store_path:  String,
  pub nar_hash:    String,
  pub nar_size:    i64,
  pub file_hash:   Option<String>,
  pub file_size:   Option<i64>,
  pub compression: String,
  pub url:         String,
  pub deriver:     Option<String>,
  pub references:  Vec<String>,
  pub sig:         Option<String>,
  pub ca:          Option<String>,
  pub created_at:  DateTime<Utc>,
  pub updated_at:  DateTime<Utc>,
}

/// Insert or replace the narinfo for one store path.
///
/// # Errors
/// Returns the underlying sqlx error.
#[allow(clippy::too_many_arguments)]
pub async fn upsert(
  pool: &PgPool,
  store_path: &str,
  nar_hash: &str,
  nar_size: i64,
  file_hash: Option<&str>,
  file_size: Option<i64>,
  compression: &str,
  url: &str,
  deriver: Option<&str>,
  references: &[String],
  sig: Option<&str>,
  ca: Option<&str>,
) -> Result<()> {
  sqlx::query(
    "INSERT INTO narinfo_cache (store_path, nar_hash, nar_size, file_hash, \
     file_size, compression, url, deriver, \"references\", sig, ca, \
     updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, NOW()) \
     ON CONFLICT (store_path) DO UPDATE SET nar_hash = EXCLUDED.nar_hash, \
     nar_size = EXCLUDED.nar_size, file_hash = EXCLUDED.file_hash, file_size \
     = EXCLUDED.file_size, compression = EXCLUDED.compression, url = \
     EXCLUDED.url, deriver = EXCLUDED.deriver, \"references\" = \
     EXCLUDED.\"references\", sig = EXCLUDED.sig, ca = EXCLUDED.ca, \
     updated_at = NOW()",
  )
  .bind(store_path)
  .bind(nar_hash)
  .bind(nar_size)
  .bind(file_hash)
  .bind(file_size)
  .bind(compression)
  .bind(url)
  .bind(deriver)
  .bind(references)
  .bind(sig)
  .bind(ca)
  .execute(pool)
  .await
  .map_err(CiError::Database)?;
  Ok(())
}

/// Read the narinfo for one store path.
///
/// # Errors
/// `CiError::NotFound` when no row matches, `CiError::Database` for
/// underlying sqlx errors.
pub async fn get(pool: &PgPool, store_path: &str) -> Result<NarInfo> {
  sqlx::query_as::<_, NarInfo>(
    "SELECT * FROM narinfo_cache WHERE store_path = $1",
  )
  .bind(store_path)
  .fetch_optional(pool)
  .await
  .map_err(CiError::Database)?
  .ok_or_else(|| CiError::NotFound(format!("narinfo for {store_path}")))
}

/// Lookup by the first 32 base32 characters of the store path's hash.
/// Substituters query `<hash>.narinfo`; this resolves that to a row.
///
/// # Errors
/// Same as [`get`].
pub async fn get_by_hash_part(
  pool: &PgPool,
  hash_part: &str,
) -> Result<NarInfo> {
  // Nix store paths are `/nix/store/<32-chars>-<name>`; we match on the
  // 32-char hash part right after the prefix.
  sqlx::query_as::<_, NarInfo>(
    "SELECT * FROM narinfo_cache WHERE store_path LIKE $1",
  )
  .bind(format!("/nix/store/{hash_part}-%"))
  .fetch_optional(pool)
  .await
  .map_err(CiError::Database)?
  .ok_or_else(|| CiError::NotFound(format!("narinfo for hash {hash_part}")))
}

/// Total rows. Cheap for admin and metrics surfaces.
///
/// # Errors
/// Returns the underlying sqlx error.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let (n,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM narinfo_cache")
    .fetch_one(pool)
    .await
    .map_err(CiError::Database)?;
  Ok(n)
}
