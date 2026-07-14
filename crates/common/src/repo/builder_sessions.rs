//! Read/list of persistent builder agent sessions.
//!
//! The runner upserts these rows directly from the capnp-rpc server
//! (no insert path here) because the schema is hot-path on every
//! register/heartbeat. This module is the read side: admin endpoints,
//! the dashboard, and metrics consume it.
//!
//! See `crates/migrations/migrations/0012_builder_sessions.sql`.

use chrono::{DateTime, Utc};
use circus_codegen::queries::builder_sessions as q;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
  db::PgPool,
  error::{CiError, Result},
};

/// One row in `builder_sessions`.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
  /// Single-session CI runner that never reconnects (see
  /// `prune_stale_ephemeral`).
  pub ephemeral:            bool,
  /// How the agent authenticated on register: `"token"` or `"oidc"`.
  pub auth_kind:            String,
  pub created_at:           DateTime<Utc>,
  pub updated_at:           DateTime<Utc>,
}

impl From<q::BuilderSessionRow> for BuilderSession {
  fn from(r: q::BuilderSessionRow) -> Self {
    Self {
      machine_id:           r.machine_id,
      name:                 r.name,
      hostname:             r.hostname,
      systems:              r.systems,
      supported_features:   r.supported_features,
      mandatory_features:   r.mandatory_features,
      speed_factor:         r.speed_factor,
      cpu_count:            r.cpu_count,
      max_jobs:             r.max_jobs,
      proto_version:        r.proto_version,
      last_seen:            r.last_seen,
      current_jobs:         r.current_jobs,
      load1:                r.load1,
      load5:                r.load5,
      load15:               r.load15,
      mem_total:            r.mem_total,
      mem_used:             r.mem_used,
      store_free:           r.store_free,
      build_dir_free:       r.build_dir_free,
      cpu_psi_avg10:        r.cpu_psi_avg10,
      mem_psi_avg10:        r.mem_psi_avg10,
      io_psi_avg10:         r.io_psi_avg10,
      connected:            r.connected,
      builds_succeeded:     r.builds_succeeded,
      builds_failed:        r.builds_failed,
      consecutive_failures: r.consecutive_failures,
      disabled_until:       r.disabled_until,
      ephemeral:            r.ephemeral,
      auth_kind:            r.auth_kind,
      created_at:           r.created_at,
      updated_at:           r.updated_at,
    }
  }
}

/// All recorded agent sessions, newest activity first.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn list(pool: &PgPool) -> Result<Vec<BuilderSession>> {
  let client = pool.get().await?;
  let rows = q::list().bind(&client).all().await?;
  Ok(rows.into_iter().map(BuilderSession::from).collect())
}

/// Only the sessions that are currently connected (the in-memory
/// `AgentPool` would contain these). Useful for the dashboard's
/// "live agents" panel.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn list_connected(pool: &PgPool) -> Result<Vec<BuilderSession>> {
  let client = pool.get().await?;
  let rows = q::list_connected().bind(&client).all().await?;
  Ok(rows.into_iter().map(BuilderSession::from).collect())
}

/// One session by its stable `machine_id`.
///
/// # Errors
///
/// `CiError::NotFound` when no row matches, `CiError::Database` for
/// underlying database errors.
pub async fn get(pool: &PgPool, machine_id: Uuid) -> Result<BuilderSession> {
  let client = pool.get().await?;
  q::get()
    .bind(&client, &machine_id)
    .opt()
    .await?
    .map(BuilderSession::from)
    .ok_or_else(|| {
      CiError::NotFound(format!("Builder session {machine_id} not found"))
    })
}

/// Record a final outcome of a build dispatched to a connected agent.
/// Used by the runner's RPC `ResultSink` to keep per-agent counters in
/// sync with the in-memory `AgentPool`.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn record_outcome(
  pool: &PgPool,
  machine_id: Uuid,
  succeeded: bool,
) -> Result<()> {
  let client = pool.get().await?;
  if succeeded {
    q::record_outcome_succeeded()
      .bind(&client, &machine_id)
      .await?;
  } else {
    // Exponential backoff matches the SSH path:
    // 60 * 3^(min(consecutive_failures + 1, 4) - 1) seconds + jitter.
    q::record_outcome_failed()
      .bind(&client, &machine_id)
      .await?;
  }
  Ok(())
}

/// Whether a live agent should receive new work right now.
///
/// A failed agent is temporarily disabled through `disabled_until`; the
/// in-memory pool tracks connectivity, while this row tracks failure backoff.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn is_schedulable(pool: &PgPool, machine_id: Uuid) -> Result<bool> {
  let client = pool.get().await?;
  let row = q::is_schedulable().bind(&client, &machine_id).opt().await?;
  Ok(row.is_some_and(|schedulable| schedulable))
}

/// Delete stale ephemeral sessions. A force-killed runner never flips
/// connected to false, so also reap connected rows whose `last_seen` is
/// older than `ttl_secs`. A null `last_seen` is kept, and persistent agents are
/// never touched.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn prune_stale_ephemeral(
  pool: &PgPool,
  ttl_secs: i64,
) -> Result<u64> {
  let client = pool.get().await?;
  let ttl = ttl_secs as f64;
  Ok(q::prune_stale_ephemeral().bind(&client, &ttl).await?)
}

/// Mark every row disconnected. Called on runner startup to clean up
/// after a crash where the `connected` flag did not get flipped.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn reset_all_connected(pool: &PgPool) -> Result<u64> {
  let client = pool.get().await?;
  Ok(q::reset_all_connected().bind(&client).await?)
}

/// Everything an agent reports at registration time.
pub struct RegisterSession<'a> {
  pub machine_id:         Uuid,
  pub name:               &'a str,
  pub hostname:           &'a str,
  pub systems:            &'a [String],
  pub supported_features: &'a [String],
  pub mandatory_features: &'a [String],
  pub speed_factor:       f32,
  pub cpu_count:          i32,
  pub max_jobs:           i32,
  pub proto_version:      &'a str,
  pub ephemeral:          bool,
  pub auth_kind:          &'a str,
}

/// Upsert an agent's session on register, marking it connected.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn register(
  pool: &PgPool,
  session: RegisterSession<'_>,
) -> Result<()> {
  let client = pool.get().await?;
  q::register()
    .bind(
      &client,
      &session.machine_id,
      &session.name,
      &session.hostname,
      &session.systems,
      &session.supported_features,
      &session.mandatory_features,
      &session.speed_factor,
      &session.cpu_count,
      &session.max_jobs,
      &session.proto_version,
      &session.ephemeral,
      &session.auth_kind,
    )
    .await?;
  Ok(())
}

/// Flip an agent's session to disconnected when its RPC connection drops.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn mark_disconnected(pool: &PgPool, machine_id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  q::mark_disconnected().bind(&client, &machine_id).await?;
  Ok(())
}

/// Refresh a session's liveness timestamp when work is dispatched to it.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn touch(pool: &PgPool, machine_id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  q::touch().bind(&client, &machine_id).await?;
  Ok(())
}

/// The metrics an agent reports on every heartbeat ping.
pub struct Heartbeat {
  pub machine_id:     Uuid,
  pub load1:          f32,
  pub load5:          f32,
  pub load15:         f32,
  pub cpu_psi_avg10:  f32,
  pub mem_psi_avg10:  f32,
  pub io_psi_avg10:   f32,
  pub current_jobs:   i32,
  pub mem_total:      i64,
  pub mem_used:       i64,
  pub store_free:     i64,
  pub build_dir_free: i64,
}

/// Persist an agent's heartbeat metrics and bump its liveness timestamps.
///
/// # Errors
///
/// Returns the underlying database error.
pub async fn heartbeat(pool: &PgPool, hb: Heartbeat) -> Result<()> {
  let client = pool.get().await?;
  q::heartbeat()
    .bind(
      &client,
      &hb.load1,
      &hb.load5,
      &hb.load15,
      &hb.cpu_psi_avg10,
      &hb.mem_psi_avg10,
      &hb.io_psi_avg10,
      &hb.current_jobs,
      &hb.mem_total,
      &hb.mem_used,
      &hb.store_free,
      &hb.build_dir_free,
      &hb.machine_id,
    )
    .await?;
  Ok(())
}
