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
use uuid::Uuid;

use crate::error::{CiError, Result};

#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NarInfo {
  pub store_path:      String,
  pub nar_hash:        String,
  pub nar_size:        i64,
  pub file_hash:       Option<String>,
  pub file_size:       Option<i64>,
  pub compression:     String,
  pub url:             String,
  pub deriver:         Option<String>,
  pub references:      Vec<String>,
  pub sig:             Option<String>,
  pub ca:              Option<String>,
  pub build_id:        Option<Uuid>,
  pub project_id:      Option<Uuid>,
  pub created_at:      DateTime<Utc>,
  pub updated_at:      DateTime<Utc>,
  pub last_fetched_at: Option<DateTime<Utc>>,
}

pub struct UpsertNarInfo<'a> {
  pub store_path:  &'a str,
  pub nar_hash:    &'a str,
  pub nar_size:    i64,
  pub file_hash:   Option<&'a str>,
  pub file_size:   Option<i64>,
  pub compression: &'a str,
  pub url:         &'a str,
  pub deriver:     Option<&'a str>,
  pub references:  &'a [String],
  pub sig:         Option<&'a str>,
  pub ca:          Option<&'a str>,
  pub build_id:    Option<Uuid>,
  pub project_id:  Option<Uuid>,
}

/// Insert or replace the narinfo for one store path.
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn upsert(pool: &PgPool, info: UpsertNarInfo<'_>) -> Result<()> {
  // One transaction so a row never lands without its project association.
  let mut tx = pool.begin().await?;
  sqlx::query(
    "INSERT INTO narinfo_cache (store_path, nar_hash, nar_size, file_hash, \
     file_size, compression, url, deriver, \"references\", sig, ca, build_id, \
     project_id, updated_at) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, \
     $11, $12, $13, NOW()) ON CONFLICT (store_path) DO UPDATE SET nar_hash = \
     EXCLUDED.nar_hash, nar_size = EXCLUDED.nar_size, file_hash = \
     EXCLUDED.file_hash, file_size = EXCLUDED.file_size, compression = \
     EXCLUDED.compression, url = EXCLUDED.url, deriver = EXCLUDED.deriver, \
     \"references\" = EXCLUDED.\"references\", sig = EXCLUDED.sig, ca = \
     EXCLUDED.ca, build_id = COALESCE(narinfo_cache.build_id, \
     EXCLUDED.build_id), project_id = COALESCE(narinfo_cache.project_id, \
     EXCLUDED.project_id), updated_at = NOW()",
  )
  .bind(info.store_path)
  .bind(info.nar_hash)
  .bind(info.nar_size)
  .bind(info.file_hash)
  .bind(info.file_size)
  .bind(info.compression)
  .bind(info.url)
  .bind(info.deriver)
  .bind(info.references)
  .bind(info.sig)
  .bind(info.ca)
  .bind(info.build_id)
  .bind(info.project_id)
  .execute(&mut *tx)
  .await?;

  if let Some(project_id) = info.project_id {
    sqlx::query(
      "INSERT INTO narinfo_cache_projects (store_path, project_id, build_id, \
       updated_at) VALUES ($1, $2, $3, NOW()) ON CONFLICT (store_path, \
       project_id) DO UPDATE SET build_id = COALESCE(EXCLUDED.build_id, \
       narinfo_cache_projects.build_id), updated_at = NOW()",
    )
    .bind(info.store_path)
    .bind(project_id)
    .bind(info.build_id)
    .execute(&mut *tx)
    .await?;
  }
  tx.commit().await?;
  Ok(())
}

/// Read the narinfo for one store path.
///
/// # Errors
///
/// `CiError::NotFound` when no row matches, `CiError::Database` for
/// underlying sqlx errors.
pub async fn get(pool: &PgPool, store_path: &str) -> Result<NarInfo> {
  sqlx::query_as::<_, NarInfo>(
    "SELECT * FROM narinfo_cache WHERE store_path = $1",
  )
  .bind(store_path)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("narinfo for {store_path}")))
}

/// Lookup by the first 32 base32 characters of the store path's hash.
/// Substituters query `<hash>.narinfo`; this resolves that to a row.
///
/// # Errors
///
/// Same as [`get`].
pub async fn get_by_hash_part(
  pool: &PgPool,
  hash_part: &str,
  project_id: Option<Uuid>,
) -> Result<NarInfo> {
  // Nix store paths are `/nix/store/<32-chars>-<name>`; we match on the
  // 32-char hash part right after the prefix.
  sqlx::query_as::<_, NarInfo>(
    "SELECT * FROM narinfo_cache n WHERE n.store_path LIKE $1 AND ($2::uuid \
     IS NULL OR n.project_id = $2 OR EXISTS (SELECT 1 FROM \
     narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND \
     ncp.project_id = $2)) ORDER BY n.updated_at DESC LIMIT 1",
  )
  .bind(format!("/nix/store/{hash_part}-%"))
  .bind(project_id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| narinfo_not_found("hash", hash_part, project_id))
}

/// Lookup by the narinfo `URL` field, e.g. `nar/<hash>.nar.zst`.
///
/// This is used by the server's `/nix-cache/nar/...` route to resolve NARs
/// uploaded by agents through the presigned S3 flow.
///
/// # Errors
///
/// Same as [`get`].
pub async fn get_by_url(
  pool: &PgPool,
  url: &str,
  project_id: Option<Uuid>,
) -> Result<NarInfo> {
  sqlx::query_as::<_, NarInfo>(
    "SELECT * FROM narinfo_cache n WHERE n.url = $1 AND ($2::uuid IS NULL OR \
     n.project_id = $2 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp \
     WHERE ncp.store_path = n.store_path AND ncp.project_id = $2)) ORDER BY \
     n.updated_at DESC LIMIT 1",
  )
  .bind(url)
  .bind(project_id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| narinfo_not_found("URL", url, project_id))
}

fn narinfo_not_found(
  kind: &str,
  value: &str,
  project_id: Option<Uuid>,
) -> CiError {
  let scope = project_id.map_or_else(String::new, |id| format!(" in {id}"));
  CiError::NotFound(format!("narinfo for {kind} {value}{scope}"))
}

/// Total rows. Cheap for admin and metrics surfaces.
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let (n,) = sqlx::query_as::<_, (i64,)>("SELECT COUNT(*) FROM narinfo_cache")
    .fetch_one(pool)
    .await?;
  Ok(n)
}

/// Aggregate storage figures for one cache scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStorageSummary {
  /// Number of stored NARs (narinfo rows).
  pub nar_count:          i64,
  /// Sum of uncompressed NAR sizes in bytes.
  pub uncompressed_bytes: i64,
  /// Sum of on-disk (compressed) file sizes in bytes. NARs without a recorded
  /// `file_size` contribute nothing.
  pub compressed_bytes:   i64,
}

/// Storage totals for a cache scope. `project_id = None` covers the unscoped
/// global view, a concrete id scopes to one project.
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn storage_summary(
  pool: &PgPool,
  project_id: Option<Uuid>,
) -> Result<CacheStorageSummary> {
  let (nar_count, uncompressed_bytes, compressed_bytes) =
    sqlx::query_as::<_, (i64, i64, i64)>(
      "WITH uploaded AS (SELECT store_path, nar_size, file_size FROM \
       narinfo_cache n WHERE ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS \
       (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = \
       n.store_path AND ncp.project_id = $1))), local AS (SELECT DISTINCT ON \
       (path) path AS store_path, COALESCE(file_size, 0) AS nar_size, \
       NULL::bigint AS file_size FROM (SELECT bp.path, bp.file_size, \
       bp.created_at FROM build_products bp JOIN builds b ON b.id = \
       bp.build_id JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets \
       j ON j.id = e.jobset_id WHERE b.status = 'succeeded' AND b.signed = \
       true AND ($1::uuid IS NULL OR j.project_id = $1) UNION ALL SELECT \
       b.build_output_path AS path, NULL::bigint AS file_size, \
       COALESCE(b.completed_at, b.created_at) AS created_at FROM builds b \
       JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = \
       e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND \
       b.build_output_path IS NOT NULL AND ($1::uuid IS NULL OR j.project_id \
       = $1)) candidates WHERE NOT EXISTS (SELECT 1 FROM narinfo_cache n \
       WHERE n.store_path = candidates.path AND ($1::uuid IS NULL OR \
       n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp \
       WHERE ncp.store_path = n.store_path AND ncp.project_id = $1))) ORDER \
       BY path, created_at DESC), inventory AS (SELECT * FROM uploaded UNION \
       ALL SELECT * FROM local) SELECT COUNT(*), COALESCE(SUM(nar_size), \
       0)::bigint, COALESCE(SUM(file_size), 0)::bigint FROM inventory",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
  Ok(CacheStorageSummary {
    nar_count,
    uncompressed_bytes,
    compressed_bytes,
  })
}

/// Newest upload and oldest fetch timestamps for the NARs stat strip.
/// Both are `None` when the scope holds no rows (or none fetched yet).
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn storage_extremes(
  pool: &PgPool,
  project_id: Option<Uuid>,
) -> Result<(Option<DateTime<Utc>>, Option<DateTime<Utc>>)> {
  let (last_uploaded, oldest_fetched) =
    sqlx::query_as::<_, (Option<DateTime<Utc>>, Option<DateTime<Utc>>)>(
      "WITH uploaded AS (SELECT store_path, created_at, last_fetched_at FROM \
       narinfo_cache n WHERE ($1::uuid IS NULL OR n.project_id = $1 OR EXISTS \
       (SELECT 1 FROM narinfo_cache_projects ncp WHERE ncp.store_path = \
       n.store_path AND ncp.project_id = $1))), local AS (SELECT DISTINCT ON \
       (path) path AS store_path, created_at, NULL::timestamptz AS \
       last_fetched_at FROM (SELECT bp.path, bp.created_at FROM \
       build_products bp JOIN builds b ON b.id = bp.build_id JOIN evaluations \
       e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE \
       b.status = 'succeeded' AND b.signed = true AND ($1::uuid IS NULL OR \
       j.project_id = $1) UNION ALL SELECT b.build_output_path AS path, \
       COALESCE(b.completed_at, b.created_at) AS created_at FROM builds b \
       JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = \
       e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND \
       b.build_output_path IS NOT NULL AND ($1::uuid IS NULL OR j.project_id \
       = $1)) candidates WHERE NOT EXISTS (SELECT 1 FROM narinfo_cache n \
       WHERE n.store_path = candidates.path AND ($1::uuid IS NULL OR \
       n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp \
       WHERE ncp.store_path = n.store_path AND ncp.project_id = $1))) ORDER \
       BY path, created_at DESC), inventory AS (SELECT * FROM uploaded UNION \
       ALL SELECT * FROM local) SELECT MAX(created_at), MIN(last_fetched_at) \
       FROM inventory",
    )
    .bind(project_id)
    .fetch_one(pool)
    .await?;
  Ok((last_uploaded, oldest_fetched))
}

/// A single NAR row prepared for the Caches dashboard listing. `package_name`
/// is the store-path name with the 32-char hash stripped.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct NarListItem {
  pub store_path:      String,
  pub package_name:    String,
  pub nar_size:        i64,
  pub file_size:       Option<i64>,
  pub compression:     String,
  pub created_at:      DateTime<Utc>,
  pub last_fetched_at: Option<DateTime<Utc>>,
}

impl From<NarInfo> for NarListItem {
  fn from(row: NarInfo) -> Self {
    let package_name = package_name_from_store_path(&row.store_path);
    Self {
      store_path: row.store_path,
      package_name,
      nar_size: row.nar_size,
      file_size: row.file_size,
      compression: row.compression,
      created_at: row.created_at,
      last_fetched_at: row.last_fetched_at,
    }
  }
}

/// Derive the human-facing package name from a `/nix/store/<hash>-<name>`
/// path: drop the prefix and the 32-char hash, returning `<name>`. Falls back
/// to the raw path when it does not match the expected shape.
#[must_use]
pub fn package_name_from_store_path(store_path: &str) -> String {
  store_path
    .strip_prefix("/nix/store/")
    .and_then(|rest| rest.split_once('-'))
    .map_or_else(|| store_path.to_owned(), |(_hash, name)| name.to_owned())
}

/// List NARs for a scope, filtered by store-path hash prefix and/or a
/// substring of the post-hash package name. Ordered newest-first.
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn list_filtered(
  pool: &PgPool,
  project_id: Option<Uuid>,
  hash_prefix: Option<&str>,
  package_query: Option<&str>,
  limit: i64,
  offset: i64,
) -> Result<Vec<NarListItem>> {
  let rows = sqlx::query_as::<_, NarListItem>(
    "WITH uploaded AS (SELECT store_path, nar_size, file_size, compression, \
     created_at, last_fetched_at FROM narinfo_cache n WHERE ($1::uuid IS NULL \
     OR n.project_id = $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp \
     WHERE ncp.store_path = n.store_path AND ncp.project_id = $1))), local AS \
     (SELECT DISTINCT ON (path) path AS store_path, COALESCE(file_size, 0) AS \
     nar_size, NULL::bigint AS file_size, 'none' AS compression, created_at, \
     NULL::timestamptz AS last_fetched_at FROM (SELECT bp.path, bp.file_size, \
     bp.created_at FROM build_products bp JOIN builds b ON b.id = bp.build_id \
     JOIN evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = \
     e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND \
     ($1::uuid IS NULL OR j.project_id = $1) UNION ALL SELECT \
     b.build_output_path AS path, NULL::bigint AS file_size, \
     COALESCE(b.completed_at, b.created_at) AS created_at FROM builds b JOIN \
     evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = \
     e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND \
     b.build_output_path IS NOT NULL AND ($1::uuid IS NULL OR j.project_id = \
     $1)) candidates WHERE NOT EXISTS (SELECT 1 FROM narinfo_cache n WHERE \
     n.store_path = candidates.path AND ($1::uuid IS NULL OR n.project_id = \
     $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE \
     ncp.store_path = n.store_path AND ncp.project_id = $1))) ORDER BY path, \
     created_at DESC), inventory AS (SELECT * FROM uploaded UNION ALL SELECT \
     * FROM local) SELECT store_path, COALESCE(substring(store_path from \
     '^/nix/store/[^-]+-(.*)$'), store_path) AS package_name, nar_size, \
     file_size, compression, created_at, last_fetched_at FROM inventory WHERE \
     ($2::text IS NULL OR store_path LIKE '/nix/store/' || $2 || '%') AND \
     ($3::text IS NULL OR store_path LIKE '%-%' || $3 || '%') ORDER BY \
     created_at DESC LIMIT $4 OFFSET $5",
  )
  .bind(project_id)
  .bind(hash_prefix)
  .bind(package_query)
  .bind(limit)
  .bind(offset)
  .fetch_all(pool)
  .await?;
  Ok(rows)
}

/// Count NARs matching the same filters as [`list_filtered`].
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn count_filtered(
  pool: &PgPool,
  project_id: Option<Uuid>,
  hash_prefix: Option<&str>,
  package_query: Option<&str>,
) -> Result<i64> {
  let (n,) = sqlx::query_as::<_, (i64,)>(
    "WITH uploaded AS (SELECT store_path FROM narinfo_cache n WHERE ($1::uuid \
     IS NULL OR n.project_id = $1 OR EXISTS (SELECT 1 FROM \
     narinfo_cache_projects ncp WHERE ncp.store_path = n.store_path AND \
     ncp.project_id = $1))), local AS (SELECT DISTINCT ON (path) path AS \
     store_path FROM (SELECT bp.path, bp.created_at FROM build_products bp \
     JOIN builds b ON b.id = bp.build_id JOIN evaluations e ON e.id = \
     b.evaluation_id JOIN jobsets j ON j.id = e.jobset_id WHERE b.status = \
     'succeeded' AND b.signed = true AND ($1::uuid IS NULL OR j.project_id = \
     $1) UNION ALL SELECT b.build_output_path AS path, \
     COALESCE(b.completed_at, b.created_at) AS created_at FROM builds b JOIN \
     evaluations e ON e.id = b.evaluation_id JOIN jobsets j ON j.id = \
     e.jobset_id WHERE b.status = 'succeeded' AND b.signed = true AND \
     b.build_output_path IS NOT NULL AND ($1::uuid IS NULL OR j.project_id = \
     $1)) candidates WHERE NOT EXISTS (SELECT 1 FROM narinfo_cache n WHERE \
     n.store_path = candidates.path AND ($1::uuid IS NULL OR n.project_id = \
     $1 OR EXISTS (SELECT 1 FROM narinfo_cache_projects ncp WHERE \
     ncp.store_path = n.store_path AND ncp.project_id = $1))) ORDER BY path, \
     created_at DESC), inventory AS (SELECT * FROM uploaded UNION ALL SELECT \
     * FROM local) SELECT COUNT(*) FROM inventory WHERE ($2::text IS NULL OR \
     store_path LIKE '/nix/store/' || $2 || '%') AND ($3::text IS NULL OR \
     store_path LIKE '%-%' || $3 || '%')",
  )
  .bind(project_id)
  .bind(hash_prefix)
  .bind(package_query)
  .fetch_one(pool)
  .await?;
  Ok(n)
}

/// Best-effort stamp of `last_fetched_at` for one served store path. Fired
/// and forgotten on the serve path, so failures are swallowed by the caller.
///
/// # Errors
///
/// Returns the underlying sqlx error.
pub async fn touch_last_fetched(pool: &PgPool, store_path: &str) -> Result<()> {
  sqlx::query(
    "UPDATE narinfo_cache SET last_fetched_at = NOW() WHERE store_path = $1",
  )
  .bind(store_path)
  .execute(pool)
  .await?;
  Ok(())
}
