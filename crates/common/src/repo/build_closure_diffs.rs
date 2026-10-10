use circus_codegen::queries::build_closure_diffs as q;
use serde::Serialize;
use uuid::Uuid;

use crate::{db::PgPool, error::Result, models::PackageChange};

/// Package changes in a build's closure against an earlier success of its job.
#[derive(Debug, Clone, Serialize)]
pub struct ClosureDiff {
  pub against_build_id: Uuid,
  pub against_commit:   String,
  pub changes:          Vec<PackageChange>,
}

/// # Errors
///
/// Returns error if the changes cannot be encoded or the database write fails.
pub async fn upsert(
  pool: &PgPool,
  build_id: Uuid,
  against_build_id: Uuid,
  changes: &[PackageChange],
) -> Result<()> {
  let encoded = serde_json::to_value(changes)?;
  let client = pool.get().await?;
  q::upsert()
    .bind(&client, &build_id, &against_build_id, &encoded)
    .await?;
  Ok(())
}

/// # Errors
///
/// Returns error if the database query fails or the stored changes are
/// malformed.
pub async fn get(pool: &PgPool, build_id: Uuid) -> Result<Option<ClosureDiff>> {
  let client = pool.get().await?;
  let Some(row) = q::get().bind(&client, &build_id).opt().await? else {
    return Ok(None);
  };
  let changes = serde_json::from_value(row.changes)?;
  Ok(Some(ClosureDiff {
    against_build_id: row.against_build_id,
    against_commit: row.against_commit,
    changes,
  }))
}
