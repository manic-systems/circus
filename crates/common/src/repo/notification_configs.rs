use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  config::DeclarativeNotification,
  error::{CiError, Result},
  models::{CreateNotificationConfig, NotificationConfig},
};

/// Create a new notification config.
///
/// # Errors
///
/// Returns error if database insert fails or config already exists.
pub async fn create(
  pool: &PgPool,
  input: CreateNotificationConfig,
) -> Result<NotificationConfig> {
  sqlx::query_as::<_, NotificationConfig>(
    "INSERT INTO notification_configs (project_id, notification_type, config) \
     VALUES ($1, $2, $3) RETURNING *",
  )
  .bind(input.project_id)
  .bind(&input.notification_type)
  .bind(&input.config)
  .fetch_one(pool)
  .await
  .map_err(|e| {
    match &e {
      sqlx::Error::Database(db_err) if db_err.is_unique_violation() => {
        CiError::Conflict(format!(
          "Notification config '{}' already exists for this project",
          input.notification_type
        ))
      },
      _ => CiError::Database(e),
    }
  })
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
  sqlx::query_as::<_, NotificationConfig>(
    "SELECT * FROM notification_configs WHERE project_id = $1 AND enabled = \
     true ORDER BY created_at DESC",
  )
  .bind(project_id)
  .fetch_all(pool)
  .await
  .map_err(CiError::Database)
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
  let result = sqlx::query(
    "DELETE FROM notification_configs WHERE project_id = $1 AND id = $2",
  )
  .bind(project_id)
  .bind(id)
  .execute(pool)
  .await?;
  if result.rows_affected() == 0 {
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
  notification_type: &str,
  config: &serde_json::Value,
  enabled: bool,
) -> Result<NotificationConfig> {
  sqlx::query_as::<_, NotificationConfig>(
    "INSERT INTO notification_configs (project_id, notification_type, config, \
     enabled) VALUES ($1, $2, $3, $4) ON CONFLICT (project_id, \
     notification_type) DO UPDATE SET config = EXCLUDED.config, enabled = \
     EXCLUDED.enabled RETURNING *",
  )
  .bind(project_id)
  .bind(notification_type)
  .bind(config)
  .bind(enabled)
  .fetch_one(pool)
  .await
  .map_err(CiError::Database)
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
  let types: Vec<&str> = notifications
    .iter()
    .map(|n| n.notification_type.as_str())
    .collect();

  // Delete notification configs not in declarative config
  sqlx::query(
    "DELETE FROM notification_configs WHERE project_id = $1 AND \
     notification_type != ALL($2::text[])",
  )
  .bind(project_id)
  .bind(&types)
  .execute(pool)
  .await
  .map_err(CiError::Database)?;

  // Upsert each notification config
  for notification in notifications {
    upsert(
      pool,
      project_id,
      &notification.notification_type,
      &notification.config,
      notification.enabled,
    )
    .await?;
  }

  Ok(())
}
