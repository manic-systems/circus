//! Read/write of the `narinfo_cache` table.
//!
//! The runner's RPC server upserts a row here every time an agent
//! reports a successful presigned-NAR upload via
//! `Runner.notifyUploadComplete`. The server's cache route reads from
//! here when answering `<hash>.narinfo` queries, so a path uploaded by
//! any agent in the cluster is immediately visible to substituters.

use chrono::{DateTime, Utc};
use circus_codegen::queries::narinfo_cache as q;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
  db::PgPool,
  error::{CiError, Result},
};

#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl From<q::NarinfoCacheRow> for NarInfo {
  fn from(r: q::NarinfoCacheRow) -> Self {
    Self {
      store_path:      r.store_path,
      nar_hash:        r.nar_hash,
      nar_size:        r.nar_size,
      file_hash:       r.file_hash,
      file_size:       r.file_size,
      compression:     r.compression,
      url:             r.url,
      deriver:         r.deriver,
      references:      r.references,
      sig:             r.sig,
      ca:              r.ca,
      build_id:        r.build_id,
      project_id:      r.project_id,
      created_at:      r.created_at,
      updated_at:      r.updated_at,
      last_fetched_at: r.last_fetched_at,
    }
  }
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
/// Returns the underlying database error.
pub async fn upsert(pool: &PgPool, info: UpsertNarInfo<'_>) -> Result<()> {
  // One transaction so a row never lands without its project association.
  let mut client = pool.get().await?;
  let tx = client.transaction().await?;
  q::upsert()
    .bind(
      &tx,
      &info.store_path,
      &info.nar_hash,
      &info.nar_size,
      &info.file_hash,
      &info.file_size,
      &info.compression,
      &info.url,
      &info.deriver,
      &info.references,
      &info.sig,
      &info.ca,
      &info.build_id,
      &info.project_id,
    )
    .await?;
  if let Some(project_id) = info.project_id {
    q::upsert_project_owner()
      .bind(&tx, &info.store_path, &project_id, &info.build_id)
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
/// underlying database errors.
pub async fn get(pool: &PgPool, store_path: &str) -> Result<NarInfo> {
  let client = pool.get().await?;
  q::get()
    .bind(&client, &store_path)
    .opt()
    .await?
    .map(NarInfo::from)
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
  let client = pool.get().await?;
  let pattern = format!("/nix/store/{hash_part}-%");
  q::get_by_hash_part()
    .bind(&client, &pattern, &project_id)
    .opt()
    .await?
    .map(NarInfo::from)
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
  let client = pool.get().await?;
  q::get_by_url()
    .bind(&client, &url, &project_id)
    .opt()
    .await?
    .map(NarInfo::from)
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
/// Returns the underlying database error.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count().bind(&client).one().await?)
}

/// Aggregate storage figures for one cache scope.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheStorageSummary {
  /// Number of stored NARs (narinfo rows).
  pub nar_count:          i64,
  /// Sum of uncompressed NAR sizes in bytes.
  pub uncompressed_bytes: i64,
  /// Sum of on-disk file sizes in bytes. NARs without a recorded `file_size`
  /// (stored uncompressed) contribute their `nar_size`.
  pub compressed_bytes:   i64,
}

/// Storage totals for a cache scope. `project_id = None` covers the unscoped
/// global view, a concrete id scopes to one project.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn storage_summary(
  pool: &PgPool,
  project_id: Option<Uuid>,
) -> Result<CacheStorageSummary> {
  let client = pool.get().await?;
  let row = q::storage_summary()
    .bind(&client, &project_id)
    .one()
    .await?;
  Ok(CacheStorageSummary {
    nar_count:          row.nar_count,
    uncompressed_bytes: row.uncompressed_bytes,
    compressed_bytes:   row.compressed_bytes,
  })
}

/// Newest upload and oldest fetch timestamps for the NARs stat strip.
/// Both are `None` when the scope holds no rows (or none fetched yet).
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn storage_extremes(
  pool: &PgPool,
  project_id: Option<Uuid>,
) -> Result<(Option<DateTime<Utc>>, Option<DateTime<Utc>>)> {
  let client = pool.get().await?;
  let row = q::storage_extremes()
    .bind(&client, &project_id)
    .one()
    .await?;
  Ok((row.last_uploaded, row.oldest_fetched))
}

/// A single NAR row prepared for the Caches dashboard listing. `package_name`
/// is the store-path name with the 32-char hash stripped.
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl From<q::ListFiltered> for NarListItem {
  fn from(r: q::ListFiltered) -> Self {
    Self {
      store_path:      r.store_path,
      package_name:    r.package_name,
      nar_size:        r.nar_size,
      file_size:       r.file_size,
      compression:     r.compression,
      created_at:      r.created_at,
      last_fetched_at: r.last_fetched_at,
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
/// Returns the underlying database error.
pub async fn list_filtered(
  pool: &PgPool,
  project_id: Option<Uuid>,
  hash_prefix: Option<&str>,
  package_query: Option<&str>,
  limit: i64,
  offset: i64,
) -> Result<Vec<NarListItem>> {
  let client = pool.get().await?;
  let rows = q::list_filtered()
    .bind(
      &client,
      &project_id,
      &hash_prefix,
      &package_query,
      &limit,
      &offset,
    )
    .all()
    .await?;
  Ok(rows.into_iter().map(NarListItem::from).collect())
}

/// Count NARs matching the same filters as [`list_filtered`].
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn count_filtered(
  pool: &PgPool,
  project_id: Option<Uuid>,
  hash_prefix: Option<&str>,
  package_query: Option<&str>,
) -> Result<i64> {
  let client = pool.get().await?;
  Ok(
    q::count_filtered()
      .bind(&client, &project_id, &hash_prefix, &package_query)
      .one()
      .await?,
  )
}

/// Best-effort stamp of `last_fetched_at` for one served store path. Fired
/// and forgotten on the serve path, so failures are swallowed by the caller.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn touch_last_fetched(pool: &PgPool, store_path: &str) -> Result<()> {
  let client = pool.get().await?;
  q::touch_last_fetched().bind(&client, &store_path).await?;
  Ok(())
}

/// A cache entry removed by [`delete_stale`]: what the caller needs to delete
/// the backing object and report reclaimed bytes.
#[derive(Debug, Clone)]
pub struct DeletedNar {
  pub store_path: String,
  /// Cache-relative object URL (`nar/...`).
  pub url:        String,
  /// On-disk bytes reclaimed (`file_size`, falling back to `nar_size`).
  pub bytes:      i64,
}

impl From<q::DeletedNarRow> for DeletedNar {
  fn from(r: q::DeletedNarRow) -> Self {
    Self {
      store_path: r.store_path,
      url:        r.url,
      bytes:      r.bytes,
    }
  }
}

/// Delete cache entries for a scope. With a `cutoff`, only entries neither
/// fetched nor created since that instant are removed. `project_id = None`
/// operates on the global scope and removes matching entries outright; a
/// concrete id removes the project's association and drops only entries no
/// other project still references.
///
/// # Returns
///
/// The removed entries, so the caller can delete their backing objects.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn delete_stale(
  pool: &PgPool,
  project_id: Option<Uuid>,
  cutoff: Option<DateTime<Utc>>,
) -> Result<Vec<DeletedNar>> {
  let mut client = pool.get().await?;
  let tx = client.transaction().await?;
  let rows = if let Some(project_id) = project_id {
    q::delete_stale_project_owners()
      .bind(&tx, &project_id, &cutoff)
      .await?;
    q::delete_stale_for_project()
      .bind(&tx, &project_id, &cutoff)
      .all()
      .await?
  } else {
    q::delete_stale_global().bind(&tx, &cutoff).all().await?
  };
  tx.commit().await?;
  Ok(rows.into_iter().map(DeletedNar::from).collect())
}
