//! Database operations for notification task retry queue

use sqlx::PgPool;
use uuid::Uuid;

use crate::{
  error::Result,
  models::{NotificationTask, NotificationType},
};

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
  let task = sqlx::query_as::<_, NotificationTask>(
    r"
    INSERT INTO notification_tasks (notification_type, payload, max_attempts)
    VALUES ($1, $2, $3)
    RETURNING *
    ",
  )
  .bind(notification_type)
  .bind(payload)
  .bind(max_attempts)
  .fetch_one(pool)
  .await?;

  Ok(task)
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
  let tasks = sqlx::query_as::<_, NotificationTask>(
    r"
    SELECT *
    FROM notification_tasks
    WHERE status = 'pending'
      AND next_retry_at <= NOW()
    ORDER BY next_retry_at ASC
    LIMIT $1
    ",
  )
  .bind(limit)
  .fetch_all(pool)
  .await?;

  Ok(tasks)
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
  let tasks = sqlx::query_as::<_, NotificationTask>(
    r"
    WITH claimed AS (
      SELECT id
      FROM notification_tasks
      WHERE status = 'pending'
        AND next_retry_at <= NOW()
      ORDER BY next_retry_at ASC
      LIMIT $1
      FOR UPDATE SKIP LOCKED
    )
    UPDATE notification_tasks nt
    SET status = 'running',
        attempts = attempts + 1
    FROM claimed
    WHERE nt.id = claimed.id
    RETURNING nt.*
    ",
  )
  .bind(limit)
  .fetch_all(pool)
  .await?;

  Ok(tasks)
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
  let tasks = sqlx::query_as::<_, NotificationTask>(
    r"
    SELECT *
    FROM notification_tasks
    ORDER BY created_at DESC
    LIMIT $1
    ",
  )
  .bind(limit)
  .fetch_all(pool)
  .await?;

  Ok(tasks)
}

/// Mark a task as running (claimed by worker)
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn mark_running(pool: &PgPool, task_id: Uuid) -> Result<()> {
  sqlx::query(
    r"
    UPDATE notification_tasks
    SET status = 'running',
        attempts = attempts + 1
    WHERE id = $1
    ",
  )
  .bind(task_id)
  .execute(pool)
  .await?;

  Ok(())
}

/// Mark a task as completed successfully
///
/// # Errors
///
/// Returns error if database update fails.
pub async fn mark_completed(pool: &PgPool, task_id: Uuid) -> Result<()> {
  sqlx::query(
    r"
    UPDATE notification_tasks
    SET status = 'completed',
        completed_at = NOW()
    WHERE id = $1
    ",
  )
  .bind(task_id)
  .execute(pool)
  .await?;

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
  sqlx::query(
    r"
    UPDATE notification_tasks
    SET status = CASE
        WHEN attempts >= max_attempts THEN 'failed'::varchar
        ELSE 'pending'::varchar
      END,
        last_error = $2,
        next_retry_at = CASE
          WHEN attempts >= max_attempts THEN NOW()
          ELSE NOW() + (POWER(2, attempts - 1) || ' seconds')::interval
        END,
        completed_at = CASE
          WHEN attempts >= max_attempts THEN NOW()
          ELSE NULL
        END
    WHERE id = $1
    ",
  )
  .bind(task_id)
  .bind(error)
  .execute(pool)
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
  let task = sqlx::query_as::<_, NotificationTask>(
    r"
    UPDATE notification_tasks
    SET status = 'pending',
        attempts = 0,
        next_retry_at = NOW(),
        last_error = NULL,
        completed_at = NULL
    WHERE id = $1 AND status = 'failed'
    RETURNING *
    ",
  )
  .bind(task_id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| {
    crate::CiError::Validation(
      "Notification task not found or not failed".to_string(),
    )
  })?;

  Ok(task)
}

/// Get task by ID
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn get(pool: &PgPool, task_id: Uuid) -> Result<NotificationTask> {
  let task = sqlx::query_as::<_, NotificationTask>(
    r"
    SELECT * FROM notification_tasks WHERE id = $1
    ",
  )
  .bind(task_id)
  .fetch_one(pool)
  .await?;

  Ok(task)
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
  let result = sqlx::query(
    r"
    DELETE FROM notification_tasks
    WHERE status IN ('completed', 'failed')
      AND (completed_at < NOW() - ($1 || ' days')::interval
           OR created_at < NOW() - ($1 || ' days')::interval)
    ",
  )
  .bind(retention_days)
  .execute(pool)
  .await?;

  Ok(result.rows_affected())
}

/// Count pending tasks (for monitoring)
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_pending(pool: &PgPool) -> Result<i64> {
  let count: (i64,) = sqlx::query_as(
    r"
    SELECT COUNT(*) FROM notification_tasks WHERE status = 'pending'
    ",
  )
  .fetch_one(pool)
  .await?;

  Ok(count.0)
}

/// Count failed tasks (for monitoring)
///
/// # Errors
///
/// Returns error if database query fails.
pub async fn count_failed(pool: &PgPool) -> Result<i64> {
  let count: (i64,) = sqlx::query_as(
    r"
    SELECT COUNT(*) FROM notification_tasks WHERE status = 'failed'
    ",
  )
  .fetch_one(pool)
  .await?;

  Ok(count.0)
}
