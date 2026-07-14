use circus_codegen::queries::notification_configs as q;
use circus_config::DeclarativeNotification;
use uuid::Uuid;

use crate::{
  db::{PgPool, is_unique_violation},
  error::{CiError, Result},
  models::{CreateNotificationConfig, NotificationConfig, NotificationType},
};

impl TryFrom<q::NotificationConfigRow> for NotificationConfig {
  type Error = CiError;

  fn try_from(r: q::NotificationConfigRow) -> Result<Self> {
    let notification_type =
      r.notification_type.parse().map_err(CiError::Internal)?;
    Ok(Self {
      id: r.id,
      project_id: r.project_id,
      notification_type,
      config: r.config,
      enabled: r.enabled,
      created_at: r.created_at,
    })
  }
}

/// Create a new notification config.
///
/// The `config` blob is stored verbatim. Validation and secret encryption are
/// the caller's responsibility, keeping this crate free of any dependency on
/// the notification crate.
///
/// # Errors
///
/// Returns error if the database insert fails or the config already exists.
pub async fn create(
  pool: &PgPool,
  input: CreateNotificationConfig,
) -> Result<NotificationConfig> {
  let client = pool.get().await?;
  q::create()
    .bind(
      &client,
      &input.project_id,
      &input.notification_type.as_str(),
      &input.config,
    )
    .one()
    .await
    .map_err(|e| {
      if is_unique_violation(&e) {
        CiError::Conflict(format!(
          "Notification config '{}' already exists for this project",
          input.notification_type
        ))
      } else {
        CiError::Database(e)
      }
    })?
    .try_into()
}

/// List all enabled notification configs for a project.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_for_project(
  pool: &PgPool,
  project_id: Uuid,
) -> Result<Vec<NotificationConfig>> {
  let client = pool.get().await?;
  let rows = q::list_for_project()
    .bind(&client, &project_id)
    .all()
    .await?;
  rows.into_iter().map(NotificationConfig::try_from).collect()
}

/// Delete a notification config for a project.
///
/// # Errors
///
/// Returns error if database delete fails or config not found.
pub async fn delete_for_project(
  pool: &PgPool,
  project_id: Uuid,
  id: Uuid,
) -> Result<()> {
  let client = pool.get().await?;
  let affected = q::delete_for_project()
    .bind(&client, &project_id, &id)
    .await?;
  if affected == 0 {
    return Err(CiError::NotFound(format!(
      "Notification config {id} not found"
    )));
  }
  Ok(())
}

/// Upsert a notification config (insert or update on conflict).
///
/// # Errors
///
/// Returns error if database operation fails.
pub async fn upsert(
  pool: &PgPool,
  project_id: Uuid,
  notification_type: NotificationType,
  config: &serde_json::Value,
  enabled: bool,
) -> Result<NotificationConfig> {
  let client = pool.get().await?;
  q::upsert()
    .bind(
      &client,
      &project_id,
      &notification_type.as_str(),
      config,
      &enabled,
    )
    .one()
    .await?
    .try_into()
}

/// Sync notification configs from declarative config.
/// Deletes configs not in the declarative list and upserts those that are.
///
/// # Errors
///
/// Returns error if database operations fail.
pub async fn sync_for_project(
  pool: &PgPool,
  project_id: Uuid,
  notifications: &[DeclarativeNotification],
) -> Result<()> {
  // Get notification types from declarative config
  let type_strings: Vec<&str> = notifications
    .iter()
    .map(|n| n.notification_type.as_str())
    .collect();

  // Delete notification configs not in declarative config
  {
    let client = pool.get().await?;
    q::sync_for_project_delete()
      .bind(&client, &project_id, &type_strings)
      .await?;
  }

  // Upsert each notification config
  for notification in notifications {
    upsert(
      pool,
      project_id,
      notification.notification_type,
      &notification.config,
      notification.enabled,
    )
    .await?;
  }

  Ok(())
}
