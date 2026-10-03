CREATE INDEX idx_builds_job_succeeded ON builds (job_name, completed_at DESC)
WHERE
  status = 'succeeded'
  AND started_at IS NOT NULL;
