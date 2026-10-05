//! Hercules-compatible per-project state files for Effects.

use circus_codegen::queries::state_files as q;
use uuid::Uuid;

use crate::{db::PgPool, error::Result};

/// Longest accepted state file name, matching the table's CHECK.
pub const MAX_NAME_LEN: usize = 255;

/// Whether `name` is a valid state file name.
#[must_use]
pub fn is_valid_name(name: &str) -> bool {
  !name.is_empty()
    && name.len() <= MAX_NAME_LEN
    && !name.starts_with('.')
    && name
      .bytes()
      .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// Read a project's state file.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn get(
  pool: &PgPool,
  project_id: Uuid,
  name: &str,
) -> Result<Option<Vec<u8>>> {
  let client = pool.get().await?;
  Ok(q::get().bind(&client, &project_id, &name).opt().await?)
}

/// Replace a project's state file, last write wins.
///
/// # Errors
///
/// Returns an error if the write fails.
pub async fn put(
  pool: &PgPool,
  project_id: Uuid,
  name: &str,
  data: &[u8],
  build_id: Uuid,
) -> Result<()> {
  let client = pool.get().await?;
  q::put()
    .bind(&client, &project_id, &name, &data, &build_id)
    .await?;
  Ok(())
}
