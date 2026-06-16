-- Align the notification_configs.notification_type CHECK with the channel kinds
-- the application actually supports. The original constraint omitted 'slack'
-- even though the dashboard and declarative config offer it, so a per-project
-- Slack notification could not be persisted.
ALTER TABLE notification_configs
DROP CONSTRAINT IF EXISTS notification_configs_notification_type_check;

ALTER TABLE notification_configs
ADD CONSTRAINT notification_configs_notification_type_check CHECK (
  notification_type IN (
    'github_status',
    'gitea_status',
    'forgejo_status',
    'gitlab_status',
    'webhook',
    'email',
    'slack'
  )
);
