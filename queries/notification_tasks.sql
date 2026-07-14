--: NotificationTaskRow(last_error?, completed_at?)

--! create : NotificationTaskRow
INSERT INTO notification_tasks (notification_type, payload, max_attempts)
VALUES (:notification_type, :payload, :max_attempts)
RETURNING *;

--! list_pending : NotificationTaskRow
SELECT *
FROM notification_tasks
WHERE status = 'pending'
  AND next_retry_at <= NOW()
ORDER BY next_retry_at ASC
LIMIT :limit;

--! claim_pending : NotificationTaskRow
WITH claimed AS (
  SELECT id
  FROM notification_tasks
  WHERE status = 'pending'
    AND next_retry_at <= NOW()
  ORDER BY next_retry_at ASC
  LIMIT :limit
  FOR UPDATE SKIP LOCKED
)
UPDATE notification_tasks nt
SET status = 'running',
    attempts = attempts + 1
FROM claimed
WHERE nt.id = claimed.id
RETURNING nt.*;

--! list_recent : NotificationTaskRow
SELECT *
FROM notification_tasks
ORDER BY created_at DESC
LIMIT :limit;

--! mark_running
UPDATE notification_tasks
SET status = 'running',
    attempts = attempts + 1
WHERE id = :task_id;

--! mark_completed
UPDATE notification_tasks
SET status = 'completed',
    completed_at = NOW()
WHERE id = :task_id;

--! mark_failed_and_retry (error?)
UPDATE notification_tasks
SET status = CASE
      WHEN attempts >= max_attempts THEN 'failed'::varchar
      ELSE 'pending'::varchar
    END,
    last_error = :error,
    next_retry_at = CASE
      WHEN attempts >= max_attempts THEN NOW()
      ELSE NOW() + (POWER(2, attempts - 1) || ' seconds')::interval
    END,
    completed_at = CASE
      WHEN attempts >= max_attempts THEN NOW()
      ELSE NULL
    END
WHERE id = :task_id;

--! requeue_failed : NotificationTaskRow
UPDATE notification_tasks
SET status = 'pending',
    attempts = 0,
    next_retry_at = NOW(),
    last_error = NULL,
    completed_at = NULL
WHERE id = :task_id AND status = 'failed'
RETURNING *;

--! get : NotificationTaskRow
SELECT * FROM notification_tasks WHERE id = :task_id;

--! cleanup_old_tasks
DELETE FROM notification_tasks
WHERE status IN ('completed', 'failed')
  AND (completed_at < NOW() - (:retention_days || ' days')::interval
       OR created_at < NOW() - (:retention_days || ' days')::interval);

--! count_pending
SELECT COUNT(*) FROM notification_tasks WHERE status = 'pending';

--! count_failed
SELECT COUNT(*) FROM notification_tasks WHERE status = 'failed';
