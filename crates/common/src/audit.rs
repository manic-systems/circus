//! Append-only audit log for security-relevant actions.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use uuid::Uuid;

use crate::error::{CiError, Result};

/// Identity of the actor performing an audited action.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Actor {
  /// `"api_key"`, `"user"`, or `"anonymous"`.
  pub kind: ActorKind,
  /// Database id of the underlying `api_key` or user row when known.
  pub id:   Option<Uuid>,
  /// Display name at the time of the action; preserved so the log remains
  /// readable after the referenced row is deleted.
  pub name: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ActorKind {
  ApiKey,
  User,
  Anonymous,
}

impl ActorKind {
  #[must_use]
  pub const fn as_str(&self) -> &'static str {
    match self {
      Self::ApiKey => "api_key",
      Self::User => "user",
      Self::Anonymous => "anonymous",
    }
  }
}

impl Actor {
  #[must_use]
  pub const fn anonymous() -> Self {
    Self {
      kind: ActorKind::Anonymous,
      id:   None,
      name: None,
    }
  }

  #[must_use]
  pub fn api_key(id: Uuid, name: impl Into<String>) -> Self {
    Self {
      kind: ActorKind::ApiKey,
      id:   Some(id),
      name: Some(name.into()),
    }
  }

  #[must_use]
  pub fn user(id: Uuid, name: impl Into<String>) -> Self {
    Self {
      kind: ActorKind::User,
      id:   Some(id),
      name: Some(name.into()),
    }
  }
}

/// One entry in the audit log, as read back from the database.
#[derive(Debug, Clone, Serialize, Deserialize, sqlx::FromRow)]
pub struct AuditEntry {
  pub id:          Uuid,
  pub occurred_at: DateTime<Utc>,
  pub actor_kind:  String,
  pub actor_id:    Option<Uuid>,
  pub actor_name:  Option<String>,
  pub action:      String,
  pub target_kind: Option<String>,
  pub target_id:   Option<String>,
  pub details:     serde_json::Value,
  pub remote_addr: Option<String>,
}

/// A record about to be written to the audit log.
#[derive(Debug, Clone)]
pub struct AuditRecord<'a> {
  pub actor:       &'a Actor,
  /// Stable, uppercase action code. Examples: `LOGIN_SUCCESS`,
  /// `LOGIN_FAILURE`, `BUILDER_CREATE`, `BUILDER_DELETE`, `CONFIG_UPDATE`,
  /// `API_KEY_CREATE`, `API_KEY_DELETE`, `USER_CREATE`, `USER_UPDATE`,
  /// `USER_DELETE`, `USER_PASSWORD_CHANGE`, `PROJECT_DELETE`.
  pub action:      &'a str,
  pub target_kind: Option<&'a str>,
  pub target_id:   Option<&'a str>,
  pub details:     serde_json::Value,
  pub remote_addr: Option<&'a str>,
}

/// Insert an audit row. Failure does NOT propagate to the caller's
/// response: audit writes are best-effort. The caller passes a `PgPool`
/// reference; if the database is gone the underlying action has likely
/// failed too. We log the failure at WARN.
///
/// Returns `true` on success, `false` if the write failed (already logged).
pub async fn record(pool: &PgPool, entry: AuditRecord<'_>) -> bool {
  let res = sqlx::query(
    "INSERT INTO audit_log (actor_kind, actor_id, actor_name, action, \
     target_kind, target_id, details, remote_addr) VALUES ($1, $2, $3, $4, \
     $5, $6, $7, $8)",
  )
  .bind(entry.actor.kind.as_str())
  .bind(entry.actor.id)
  .bind(entry.actor.name.as_deref())
  .bind(entry.action)
  .bind(entry.target_kind)
  .bind(entry.target_id)
  .bind(&entry.details)
  .bind(entry.remote_addr)
  .execute(pool)
  .await;

  match res {
    Ok(_) => true,
    Err(e) => {
      tracing::warn!(
        action = entry.action,
        actor = entry.actor.name.as_deref().unwrap_or("?"),
        "audit log write failed: {e}"
      );
      false
    },
  }
}

/// List audit entries, newest first, paginated.
///
/// # Errors
///
/// Returns error if the database query fails.
pub async fn list(
  pool: &PgPool,
  limit: i64,
  offset: i64,
) -> Result<Vec<AuditEntry>> {
  sqlx::query_as::<_, AuditEntry>(
    "SELECT id, occurred_at, actor_kind, actor_id, actor_name, action, \
     target_kind, target_id, details, remote_addr FROM audit_log ORDER BY \
     occurred_at DESC LIMIT $1 OFFSET $2",
  )
  .bind(limit)
  .bind(offset)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
}

/// Count total audit entries (for pagination UIs).
///
/// # Errors
///
/// Returns error if the database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let row: (i64,) = sqlx::query_as("SELECT COUNT(*) FROM audit_log")
    .fetch_one(pool)
    .await
    .map_err(CiError::Database)?;
  Ok(row.0)
}
