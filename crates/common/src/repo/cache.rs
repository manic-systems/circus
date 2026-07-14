//! Provenance checks for the unauthenticated binary cache. The cache only
//! rebroadcasts paths Circus itself produced, never arbitrary store paths.

use circus_codegen::queries::cache as q;
use uuid::Uuid;

use crate::{db::PgPool, error::Result};

/// Whether `store_path` is a recorded build product or build output,
/// optionally scoped to a project.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn has_circus_build_product(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool> {
  let client = pool.get().await?;
  Ok(
    q::has_circus_build_product()
      .bind(&client, &store_path, &project_id)
      .one()
      .await?,
  )
}

/// The signed fields of a persisted narinfo, enough to recheck its signature.
pub struct PersistedNarinfoSig {
  pub nar_hash:   String,
  pub nar_size:   i64,
  pub references: Vec<String>,
  pub sig:        Option<String>,
}

/// The persisted narinfo for `store_path` when it carries a real signature,
/// limited to rows the project can see.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn signed_persisted_narinfo(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<Option<PersistedNarinfoSig>> {
  let client = pool.get().await?;
  Ok(
    q::signed_narinfo_sig()
      .bind(&client, &store_path, &project_id)
      .opt()
      .await?
      .map(|r| {
        PersistedNarinfoSig {
          nar_hash:   r.nar_hash,
          nar_size:   r.nar_size,
          references: r.references,
          // The query only matches non-empty signatures, so this is always
          // set.
          sig:        Some(r.sig),
        }
      }),
  )
}

/// Like [`has_circus_build_product`], but only counts builds Circus signed.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn has_circus_signed_build_product(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool> {
  let client = pool.get().await?;
  Ok(
    q::has_circus_signed_build_product()
      .bind(&client, &store_path, &project_id)
      .one()
      .await?,
  )
}

/// Whether `store_path` is the .drv of a dispatched build.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn has_circus_derivation_path(
  pool: &PgPool,
  store_path: &str,
  project_id: Option<Uuid>,
) -> Result<bool> {
  let client = pool.get().await?;
  Ok(
    q::has_circus_derivation_path()
      .bind(&client, &store_path, &project_id)
      .one()
      .await?,
  )
}

/// Whether any of `drv_paths` is the .drv of a dispatched build.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn has_circus_derivation_path_any(
  pool: &PgPool,
  drv_paths: &[String],
  project_id: Option<Uuid>,
) -> Result<bool> {
  let client = pool.get().await?;
  Ok(
    q::has_circus_derivation_path_any()
      .bind(&client, &drv_paths, &project_id)
      .one()
      .await?,
  )
}
