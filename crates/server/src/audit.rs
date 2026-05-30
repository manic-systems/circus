//! Thin wrapper around `circus_common::audit` that snaps actor identity
//! out of the request's extensions so handlers can call a one-liner.
//!
//! Handlers usually call [`record_action`] with an action code, optional
//! target, and optional details JSON. The function is intentionally not
//! `Result<>`: audit writes are best-effort and must never break the
//! caller's response.

use std::net::SocketAddr;

use axum::{extract::ConnectInfo, http::Extensions};
use circus_common::{
  audit::{self, Actor, AuditRecord},
  models::{ApiKey, User},
};
use serde_json::Value;
use sqlx::PgPool;

/// Extract the acting principal from request extensions. Returns
/// `Actor::anonymous()` when neither a User nor an `ApiKey` is present (for
/// example, a failed login attempt before any session was established).
#[must_use]
pub fn actor_from_extensions(extensions: &Extensions) -> Actor {
  if let Some(user) = extensions.get::<User>() {
    return Actor::user(user.id, user.username.clone());
  }
  if let Some(key) = extensions.get::<ApiKey>() {
    return Actor::api_key(key.id, key.name.clone());
  }
  Actor::anonymous()
}

/// Pull the remote address out of extensions if `ConnectInfo` was wired
/// into the router. Returns `None` when not available.
#[must_use]
pub fn remote_addr_from_extensions(extensions: &Extensions) -> Option<String> {
  extensions
    .get::<ConnectInfo<SocketAddr>>()
    .map(|c| c.0.ip().to_string())
}

/// Build an actor from an [`ApiKey`] resolved by an extractor like
/// `RequireAdmin`. Use this from handlers that have the `ApiKey` in hand
/// already and do not need to inspect raw extensions.
#[must_use]
pub fn actor_from_api_key(key: &ApiKey) -> Actor {
  Actor::api_key(key.id, key.name.clone())
}

/// One-call helper for handlers that have already extracted an
/// authenticated `ApiKey` (typically via `RequireAdmin`).
pub async fn record_for_key(
  pool: &PgPool,
  key: &ApiKey,
  action: &str,
  target_kind: Option<&str>,
  target_id: Option<&str>,
  details: Value,
) {
  let actor = actor_from_api_key(key);
  let _ = audit::record(pool, AuditRecord {
    actor: &actor,
    action,
    target_kind,
    target_id,
    details,
    remote_addr: None,
  })
  .await;
}

/// One-call helper for the common case: take the actor from extensions and
/// write an audit row. Failure is logged at WARN inside the audit module
/// and silently swallowed here; the calling handler proceeds either way.
pub async fn record_action(
  pool: &PgPool,
  extensions: &Extensions,
  action: &str,
  target_kind: Option<&str>,
  target_id: Option<&str>,
  details: Value,
) {
  let actor = actor_from_extensions(extensions);
  let remote = remote_addr_from_extensions(extensions);
  let _ = audit::record(pool, AuditRecord {
    actor: &actor,
    action,
    target_kind,
    target_id,
    details,
    remote_addr: remote.as_deref(),
  })
  .await;
}

/// Record an action with an explicit (non-extension) actor. Useful for
/// authentication events where the actor identity is being established by
/// the very action we are recording (e.g. successful login).
pub async fn record_with_actor(
  pool: &PgPool,
  actor: &Actor,
  remote_addr: Option<&str>,
  action: &str,
  target_kind: Option<&str>,
  target_id: Option<&str>,
  details: Value,
) {
  let _ = audit::record(pool, AuditRecord {
    actor,
    action,
    target_kind,
    target_id,
    details,
    remote_addr,
  })
  .await;
}
