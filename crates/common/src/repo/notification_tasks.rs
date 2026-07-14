//! Database operations for notification task retry queue

use circus_codegen::queries::notification_tasks as q;
use uuid::Uuid;

use crate::{
  db::PgPool,
  error::{CiError, Result},
  models::{NotificationTask, NotificationType},
};

impl TryFrom<q::NotificationTaskRow> for NotificationTask {
  type Error = CiError;

  fn try_from(r: q::NotificationTaskRow) -> Result<Self> {
    Ok(Self {
      id:                r.id,
      notification_type: r
        .notification_type
        .parse()
        .map_err(CiError::Internal)?,
      payload:           r.payload,
      status:            r.status.parse().map_err(CiError::Internal)?,
      attempts:          r.attempts,
      max_attempts:      r.max_attempts,
      next_retry_at:     r.next_retry_at,
      last_error:        r.last_error,
      created_at:        r.created_at,
      completed_at:      r.completed_at,
    })
  }
}

/// Create a new notification task for later delivery
///
/// # Errors
///
/// Returns error if database insert fails.
pub async fn create(
  pool: &PgPool,
  notification_type: NotificationType,
  payload: serde_json::Value,
  max_attempts: i32,
) -> Result<NotificationTask> {
  let client = pool.get().await?;
  let row = q::create()
    .bind(
      &client,
      &notification_type.as_str(),
      &payload,
      &max_attempts,
    )
    .one()
    .await?;
  NotificationTask::try_from(row)
}

/// Fetch pending tasks that are ready for retry
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_pending(
  pool: &PgPool,
  limit: i32,
) -> Result<Vec<NotificationTask>> {
  let client = pool.get().await?;
  let limit = i64::from(limit);
  let rows = q::list_pending().bind(&client, &limit).all().await?;
  rows.into_iter().map(NotificationTask::try_from).collect()
}

/// Atomically claim pending tasks that are ready for delivery.
///
/// This is safe for multiple retry workers: each worker locks a distinct set of
/// rows and immediately marks them running in the same statement.
///
/// # Errors
///
/// Returns error if the database query fails.
pub async fn claim_pending(
  pool: &PgPool,
  limit: i32,
) -> Result<Vec<NotificationTask>> {
  let client = pool.get().await?;
  let limit = i64::from(limit);
  let rows = q::claim_pending().bind(&client, &limit).all().await?;
  rows.into_iter().map(NotificationTask::try_from).collect()
}

/// List recent notification tasks for operator visibility.
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn list_recent(
  pool: &PgPool,
  limit: i32,
) -> Result<Vec<NotificationTask>> {
  let client = pool.get().await?;
  let limit = i64::from(limit);
  let rows = q::list_recent().bind(&client, &limit).all().await?;
  rows.into_iter().map(NotificationTask::try_from).collect()
}

/// Mark a task as running (claimed by worker)
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn mark_running(pool: &PgPool, task_id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  q::mark_running().bind(&client, &task_id).await?;
  Ok(())
}

/// Mark a task as completed successfully
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn mark_completed(pool: &PgPool, task_id: Uuid) -> Result<()> {
  let client = pool.get().await?;
  q::mark_completed().bind(&client, &task_id).await?;
  Ok(())
}

/// Mark a task as failed and schedule retry with exponential backoff
/// Backoff formula: 1s, 2s, 4s, 8s, 16s...
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn mark_failed_and_retry(
  pool: &PgPool,
  task_id: Uuid,
  error: &str,
) -> Result<()> {
  let client = pool.get().await?;
  q::mark_failed_and_retry()
    .bind(&client, &Some(error), &task_id)
    .await?;
  Ok(())
}

/// Requeue a failed notification task for manual retry.
///
/// # Errors
///
/// Returns error if database update fails or the task is not failed.
pub async fn requeue_failed(
  pool: &PgPool,
  task_id: Uuid,
) -> Result<NotificationTask> {
  let client = pool.get().await?;
  let row = q::requeue_failed()
    .bind(&client, &task_id)
    .opt()
    .await?
    .ok_or_else(|| {
      CiError::Validation(
        "Notification task not found or not failed".to_string(),
      )
    })?;
  NotificationTask::try_from(row)
}

/// Get task by ID
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get(pool: &PgPool, task_id: Uuid) -> Result<NotificationTask> {
  let client = pool.get().await?;
  let row = q::get().bind(&client, &task_id).one().await?;
  NotificationTask::try_from(row)
}

/// Clean up old completed/failed tasks (older than retention days)
///
/// # Errors
///
/// Returns error if database delete fails.
pub async fn cleanup_old_tasks(
  pool: &PgPool,
  retention_days: i64,
) -> Result<u64> {
  let client = pool.get().await?;
  let retention_days = retention_days.to_string();
  Ok(
    q::cleanup_old_tasks()
      .bind(&client, &retention_days)
      .await?,
  )
}

/// Count pending tasks (for monitoring)
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_pending(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count_pending().bind(&client).one().await?)
}

/// Count failed tasks (for monitoring)
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_failed(pool: &PgPool) -> Result<i64> {
  let client = pool.get().await?;
  Ok(q::count_failed().bind(&client).one().await?)
}
