ALTER TABLE builds
DROP CONSTRAINT IF EXISTS builds_status_check;

ALTER TABLE builds
ADD CONSTRAINT builds_status_check CHECK (
  status IN (
    'pending',
    'running',
    'succeeded',
    'failed',
    'dependency_failed',
    'aborted',
    'cancelled',
    'failed_with_output',
    'timeout',
    'cached_failure',
    'unsupported_system',
    'log_limit_exceeded',
    'nar_size_limit_exceeded',
    'non_deterministic',
    'oom_killed'
  )
);

ALTER TABLE notification_tasks
DROP CONSTRAINT IF EXISTS notification_tasks_notification_type_check;

ALTER TABLE notification_tasks
ADD CONSTRAINT notification_tasks_notification_type_check CHECK (
  notification_type IN (
    'webhook',
    'github_status',
    'gitea_status',
    'forgejo_status',
    'gitlab_status',
    'email',
    'slack'
  )
);
