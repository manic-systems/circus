//! Effects that asked for a forge-minted `GitToken` secret.

use circus_codegen::queries::effect_git_token_requests as q;
use uuid::Uuid;

use crate::{
  db::{DbTransaction, PgPool},
  error::Result,
};

/// Record that the effect build wants a `GitToken`.
///
/// # Errors
///
/// Returns an error if the write fails.
pub async fn insert_in_transaction(
  tx: &DbTransaction<'_>,
  build_id: Uuid,
) -> Result<()> {
  q::insert().bind(tx, &build_id).await?;
  Ok(())
}

/// Whether the effect build asked for a `GitToken`.
///
/// # Errors
///
/// Returns an error if the query fails.
pub async fn requested(pool: &PgPool, build_id: Uuid) -> Result<bool> {
  let client = pool.get().await?;
  Ok(q::requested().bind(&client, &build_id).one().await?)
}
