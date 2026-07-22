//! Durable completion events for Effect notification delivery.

use chrono::{DateTime, Utc};
use circus_codegen::queries::effect_completion_events as q;
use uuid::Uuid;

use crate::{
  db::PgPool,
  error::{CiError, Result},
  models::Build,
};

/// One terminal Effect attempt waiting for notification delivery.
#[derive(Debug, Clone)]
pub struct EffectCompletionEvent {
  pub build:      Build,
  pub attempt:    i32,
  pub revision:   i64,
  pub created_at: DateTime<Utc>,
}

impl TryFrom<q::EffectCompletionEventRow> for EffectCompletionEvent {
  type Error = CiError;

  fn try_from(row: q::EffectCompletionEventRow) -> Result<Self> {
    let build =
      serde_json::from_value(row.build_snapshot).map_err(|error| {
        CiError::Internal(format!(
          "Effect completion event {} attempt {} has an invalid Build \
           snapshot: {error}",
          row.build_id, row.retry_count
        ))
      })?;
    Ok(Self {
      build,
      attempt: row.retry_count,
      revision: row.revision,
      created_at: row.created_at,
    })
  }
}

/// List terminal Effect attempts that have not been acknowledged.
///
/// # Errors
///
/// Returns an error if the query fails or a persisted snapshot is invalid.
pub async fn list_pending(
  pool: &PgPool,
  limit: i64,
) -> Result<Vec<EffectCompletionEvent>> {
  let client = pool.get().await?;
  q::list_pending()
    .bind(&client, &limit)
    .all()
    .await?
    .into_iter()
    .map(EffectCompletionEvent::try_from)
    .collect()
}

/// Acknowledge the exact revision that was delivered.
///
/// # Errors
///
/// Returns an error if the database query fails.
pub async fn ack(
  pool: &PgPool,
  build_id: Uuid,
  attempt: i32,
  revision: i64,
) -> Result<bool> {
  let client = pool.get().await?;
  Ok(
    q::ack()
      .bind(&client, &build_id, &attempt, &revision)
      .opt()
      .await?
      .is_some(),
  )
}
