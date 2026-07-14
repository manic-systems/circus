//! Append-only audit log for security-relevant actions.

use chrono::{DateTime, Utc};
use circus_codegen::queries::audit as q;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
  db::PgPool,
  error::{CiError, Result},
};

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
#[derive(Debug, Clone, Serialize, Deserialize)]
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

impl From<q::AuditLogRow> for AuditEntry {
  fn from(r: q::AuditLogRow) -> Self {
    Self {
      id:          r.id,
      occurred_at: r.occurred_at,
      actor_kind:  r.actor_kind,
      actor_id:    r.actor_id,
      actor_name:  r.actor_name,
      action:      r.action,
      target_kind: r.target_kind,
      target_id:   r.target_id,
      details:     r.details,
      remote_addr: r.remote_addr,
    }
  }
}

/// Insert an audit row. Failure does NOT propagate to the caller's
/// response: audit writes are best-effort. The caller passes a `PgPool`
/// reference; if the database is gone the underlying action has likely
/// failed too. We log the failure at WARN.
///
/// # Returns
///
/// Returns `true` on success, `false` if the write failed (already logged).
pub async fn record(pool: &PgPool, entry: AuditRecord<'_>) -> bool {
  let res = async {
    let client = pool.get().await?;
    q::record()
      .bind(
        &client,
        &entry.actor.kind.as_str(),
        &entry.actor.id,
        &entry.actor.name.as_deref(),
        &entry.action,
        &entry.target_kind,
        &entry.target_id,
        &entry.details,
        &entry.remote_addr,
      )
      .await?;
    Ok::<(), CiError>(())
  }
  .await;

  match res {
    Ok(()) => true,
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
  let client = pool.get().await?;
  let rows = q::list().bind(&client, &limit, &offset).all().await?;
  Ok(rows.into_iter().map(AuditEntry::from).collect())
}

/// Count total audit entries (for pagination UIs).
///
/// # Errors
///
/// Returns error if the database query fails.
pub async fn count(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count().bind(&client).one().await?)
}
