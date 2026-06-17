use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::{CiError, Result},
  models::{BuildProduct, BuildStatus, CreateBuildProduct},
};

#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct PinnedBuildProduct {
  pub build_id:           Uuid,
  pub job_name:           String,
  pub system:             String,
  pub status:             BuildStatus,
  pub build_created_at:   DateTime<Utc>,
  pub product_id:         Uuid,
  pub product_name:       String,
  pub path:               String,
  pub gc_root_path:       Option<String>,
  pub product_created_at: DateTime<Utc>,
}

/// Create a build product record.
///
/// # Errors
///
/// Returns error if database insert fails.
pub async fn create(
  pool: &PgPool,
  input: CreateBuildProduct,
) -> Result<BuildProduct> {
  Ok(
    sqlx::query_as::<_, BuildProduct>(
      "INSERT INTO build_products (build_id, name, path, sha256_hash, \
       file_size, content_type, is_directory) VALUES ($1, $2, $3, $4, $5, $6, \
       $7) RETURNING *",
    )
    .bind(input.build_id)
    .bind(&input.name)
    .bind(&input.path)
    .bind(&input.sha256_hash)
    .bind(input.file_size)
    .bind(&input.content_type)
    .bind(input.is_directory)
    .fetch_one(pool)
    .await?,
  )
}

/// Get a build product by ID.
///
/// # Errors
///
/// Returns error if database query fails or product not found.
pub async fn get(pool: &PgPool, id: Uuid) -> Result<BuildProduct> {
  sqlx::query_as::<_, BuildProduct>(
    "SELECT * FROM build_products WHERE id = $1",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| CiError::NotFound(format!("Build product {id} not found")))
}

/// List all build products for a build.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_build(
  pool: &PgPool,
  build_id: Uuid,
) -> Result<Vec<BuildProduct>> {
  Ok(
    sqlx::query_as::<_, BuildProduct>(
      "SELECT * FROM build_products WHERE build_id = $1 ORDER BY created_at \
       ASC",
    )
    .bind(build_id)
    .fetch_all(pool)
    .await?,
  )
}

/// List products whose build has `keep = true`.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_pinned(
  pool: &PgPool,
  limit: i64,
  offset: i64,
) -> Result<Vec<PinnedBuildProduct>> {
  Ok(
    sqlx::query_as::<_, PinnedBuildProduct>(
      "SELECT b.id AS build_id, b.job_name, b.system, b.status, b.created_at \
       AS build_created_at, bp.id AS product_id, bp.name AS product_name, \
       bp.path, bp.gc_root_path, bp.created_at AS product_created_at FROM \
       builds b JOIN build_products bp ON bp.build_id = b.id WHERE b.keep = \
       true ORDER BY b.created_at DESC, bp.created_at ASC LIMIT $1 OFFSET $2",
    )
    .bind(limit)
    .bind(offset)
    .fetch_all(pool)
    .await?,
  )
}

/// Count products whose build has `keep = true`.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_pinned(pool: &PgPool) -> Result<i64> {
  let (count,): (i64,) = sqlx::query_as(
    "SELECT COUNT(*) FROM builds b JOIN build_products bp ON bp.build_id = \
     b.id WHERE b.keep = true",
  )
  .fetch_one(pool)
  .await?;
  Ok(count)
}

/// List pinned products without pagination for GC preservation.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_pinned_for_gc(
  pool: &PgPool,
) -> Result<Vec<PinnedBuildProduct>> {
  Ok(
    sqlx::query_as::<_, PinnedBuildProduct>(
      "SELECT b.id AS build_id, b.job_name, b.system, b.status, b.created_at \
       AS build_created_at, bp.id AS product_id, bp.name AS product_name, \
       bp.path, bp.gc_root_path, bp.created_at AS product_created_at FROM \
       builds b JOIN build_products bp ON bp.build_id = b.id WHERE b.keep = \
       true ORDER BY b.created_at DESC, bp.created_at ASC",
    )
    .fetch_all(pool)
    .await?,
  )
}
