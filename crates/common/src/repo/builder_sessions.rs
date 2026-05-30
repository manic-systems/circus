//! Read/list of persistent builder agent sessions.
//!
//! The runner upserts these rows directly from the capnp-rpc server
//! (no insert path here) because the schema is hot-path on every
//! register/heartbeat. This module is the read side: admin endpoints,
//! the dashboard, and metrics consume it.
//!
//! See `crates/migrations/migrations/0012_builder_sessions.sql`.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{FromRow, PgPool};
use uuid::Uuid;

use crate::error::{CiError, Result};

/// One row in `builder_sessions`.
#[derive(Debug, Clone, Serialize, Deserialize, FromRow)]
pub struct BuilderSession {
  pub machine_id:           Uuid,
  pub name:                 String,
  pub hostname:             String,
  pub systems:              Vec<String>,
  pub supported_features:   Vec<String>,
  pub mandatory_features:   Vec<String>,
  pub speed_factor:         f32,
  pub cpu_count:            i32,
  pub max_jobs:             i32,
  pub proto_version:        String,
  pub last_seen:            Option<DateTime<Utc>>,
  pub current_jobs:         i32,
  pub load1:                Option<f32>,
  pub load5:                Option<f32>,
  pub load15:               Option<f32>,
  pub mem_total:            Option<i64>,
  pub mem_used:             Option<i64>,
  pub store_free:           Option<i64>,
  pub build_dir_free:       Option<i64>,
  pub cpu_psi_avg10:        Option<f32>,
  pub mem_psi_avg10:        Option<f32>,
  pub io_psi_avg10:         Option<f32>,
  pub connected:            bool,
  pub builds_succeeded:     i64,
  pub builds_failed:        i64,
  pub consecutive_failures: i32,
  pub disabled_until:       Option<DateTime<Utc>>,
  pub created_at:           DateTime<Utc>,
  pub updated_at:           DateTime<Utc>,
}

/// All recorded agent sessions, newest activity first.
///
/// # Errors
/// Returns the underlying sqlx error.
pub async fn list(pool: &PgPool) -> Result<Vec<BuilderSession>> {
  sqlx::query_as::<_, BuilderSession>(
    "SELECT * FROM builder_sessions ORDER BY connected DESC, updated_at DESC",
  )
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Only the sessions that are currently connected (the in-memory
/// `AgentPool` would contain these). Useful for the dashboard's
/// "live agents" panel.
///
/// # Errors
/// Returns the underlying sqlx error.
pub async fn list_connected(pool: &PgPool) -> Result<Vec<BuilderSession>> {
  sqlx::query_as::<_, BuilderSession>(
    "SELECT * FROM builder_sessions WHERE connected = TRUE ORDER BY \
     updated_at DESC",
  )
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// One session by its stable `machine_id`.
///
/// # Errors
/// `CiError::NotFound` when no row matches, `CiError::Database` for
/// underlying sqlx errors.
pub async fn get(pool: &PgPool, machine_id: Uuid) -> Result<BuilderSession> {
  sqlx::query_as::<_, BuilderSession>(
    "SELECT * FROM builder_sessions WHERE machine_id = $1",
  )
  .bind(machine_id)
  .fetch_optional(pool)
  .await
  .map_err(CiError::Database)?
  .ok_or_else(|| {
    CiError::NotFound(format!("Builder session {machine_id} not found"))
  })
}

/// Record a final outcome of a build dispatched to a connected agent.
/// Used by the runner's RPC `ResultSink` to keep per-agent counters in
/// sync with the in-memory `AgentPool`.
///
/// # Errors
/// Returns the underlying sqlx error.
pub async fn record_outcome(
  pool: &PgPool,
  machine_id: Uuid,
  succeeded: bool,
) -> Result<()> {
  let sql = if succeeded {
    "UPDATE builder_sessions SET builds_succeeded = builds_succeeded + 1, \
     consecutive_failures = 0, disabled_until = NULL, updated_at = NOW() WHERE \
     machine_id = $1"
  } else {
    // Exponential backoff matches the SSH path:
    // 60 * 3^(min(consecutive_failures + 1, 4) - 1) seconds + jitter.
    "UPDATE builder_sessions SET builds_failed = builds_failed + 1, \
     consecutive_failures = LEAST(consecutive_failures + 1, 4), disabled_until \
     = NOW() + make_interval(secs => 60.0 * power(3, \
     LEAST(consecutive_failures + 1, 4) - 1) + (random() * 30)::int), \
     updated_at = NOW() WHERE machine_id = $1"
  };
  sqlx::query(sql)
    .bind(machine_id)
    .execute(pool)
    .await
    .map_err(CiError::Database)?;
  Ok(())
}

/// Whether a live agent should receive new work right now.
///
/// A failed agent is temporarily disabled through `disabled_until`; the
/// in-memory pool tracks connectivity, while this row tracks failure backoff.
///
/// # Errors
/// Returns the underlying sqlx error.
pub async fn is_schedulable(pool: &PgPool, machine_id: Uuid) -> Result<bool> {
  let row = sqlx::query_as::<_, (bool,)>(
    "SELECT disabled_until IS NULL OR disabled_until <= NOW() FROM \
     builder_sessions WHERE machine_id = $1",
  )
  .bind(machine_id)
  .fetch_optional(pool)
  .await
  .map_err(CiError::Database)?;
  Ok(row.is_some_and(|(schedulable,)| schedulable))
}

/// Mark every row disconnected. Called on runner startup to clean up
/// after a crash where the `connected` flag did not get flipped.
///
/// # Errors
/// Returns the underlying sqlx error.
pub async fn reset_all_connected(pool: &PgPool) -> Result<u64> {
  let res = sqlx::query(
    "UPDATE builder_sessions SET connected = FALSE WHERE connected = TRUE",
  )
  .execute(pool)
  .await
  .map_err(CiError::Database)?;
  Ok(res.rows_affected())
}
